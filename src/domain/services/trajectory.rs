//! Patched-conics trajectory prediction (spec: `telemetry-ui`, "trajectory
//! prediction (patched conics)").
//!
//! Predicts a future trajectory by propagating two-body motion around a single
//! dominant body and switching that body when the spacecraft crosses a
//! sphere-of-influence (SOI) boundary — the classic patched-conics model. The
//! result carries, per step, the state, the index of the central body, and the
//! body-centred ground-track sub-point, plus an explicit record of every SOI
//! crossing. This is the pure-domain kernel behind the trajectory-prediction
//! panel; a Bevy system samples it for `Gizmo`s and the ground-track overlay.
//!
//! The integrator is a fixed-step RK4 on `a = -μ r̂ / r²`. It is deliberately
//! simple and deterministic (AGENTS.md sections 44, 58): prediction is a
//! *display* aid, so stability and reproducibility matter more than long-horizon
//! accuracy. Two-body propagation keeps SOI switching clean and testable.
//!
//! This module is Bevy-free beyond `bevy::math` vector types (consistent with
//! the other domain services) and unit-tested without an app.

use crate::domain::services::cube_sphere::direction_to_lat_lon;
use crate::domain::services::gravity::{gravitational_acceleration, gravitational_parameter};
use bevy::math::DVec3;

/// A gravity source (planet / moon / star) in the shared prediction frame.
#[derive(Debug, Clone)]
pub struct GravityBody {
    pub name: String,
    /// Body centre in the common inertial frame, meters.
    pub position_m: DVec3,
    pub mass_kg: f64,
}

impl GravityBody {
    pub fn new(name: impl Into<String>, position_m: DVec3, mass_kg: f64) -> Self {
        Self {
            name: name.into(),
            position_m,
            mass_kg,
        }
    }

    /// Standard gravitational parameter μ = G·M (m³·s⁻²).
    pub fn mu(&self) -> f64 {
        gravitational_parameter(self.mass_kg)
    }
}

/// Patched-conics sphere-of-influence radius: `a·(m_body/m_parent)^(2/5)`.
/// `orbital_semi_major_m` is the body's semi-major axis around its parent; the
/// result is the radius (from the body centre) inside which the body's gravity
/// dominates over its parent's.
pub fn sphere_of_influence_radius(
    orbital_semi_major_m: f64,
    body_mass_kg: f64,
    parent_mass_kg: f64,
) -> f64 {
    if parent_mass_kg <= 0.0 || !orbital_semi_major_m.is_finite() || orbital_semi_major_m <= 0.0 {
        return 0.0;
    }
    let ratio = (body_mass_kg / parent_mass_kg).powf(2.0 / 5.0);
    orbital_semi_major_m * ratio
}

/// One predicted state along the trajectory.
#[derive(Debug, Clone, PartialEq)]
pub struct PredictionPoint {
    /// Time since the start of the prediction, seconds.
    pub time_s: f64,
    /// Position in the common inertial frame, meters.
    pub position_m: DVec3,
    /// Velocity in the common inertial frame, m/s.
    pub velocity_mps: DVec3,
    /// Index into the body list of the dominant central body at this point.
    pub body_index: usize,
    /// Body-centred sub-point latitude (deg), [−90, 90].
    pub lat_deg: f64,
    /// Body-centred sub-point longitude (deg), [−180, 180].
    pub lon_deg: f64,
}

/// The result of a patched-conics prediction.
#[derive(Debug, Clone, PartialEq)]
pub struct TrajectoryPrediction {
    /// One [`PredictionPoint`] per step, starting at t = 0.
    pub points: Vec<PredictionPoint>,
    /// Indices into `points` at which the dominant (central) body changed — an
    /// SOI crossing. The first point is never listed.
    pub body_transitions: Vec<usize>,
}

/// Index of the body exerting the strongest gravitational acceleration on an
/// object at `position_m`. This is the SOI-picking rule for patched conics.
pub fn dominant_body(position_m: DVec3, bodies: &[GravityBody]) -> usize {
    debug_assert!(!bodies.is_empty(), "need at least one gravity body");
    let mut best = 0usize;
    let mut best_accel = f64::NEG_INFINITY;
    for (i, body) in bodies.iter().enumerate() {
        let r = position_m - body.position_m;
        let r_sq = r.length_squared();
        if r_sq < 1e-6 {
            return i; // effectively at this body's centre
        }
        let accel = body.mu() / r_sq;
        if accel > best_accel {
            best_accel = accel;
            best = i;
        }
    }
    best
}

/// Two-body acceleration at a point under a single central body.
fn central_accel(position_m: DVec3, bodies: &[GravityBody], body_index: usize) -> DVec3 {
    let body = &bodies[body_index];
    gravitational_acceleration(body.mass_kg, position_m, body.position_m)
}

/// Propagate under `bodies` with fixed-step RK4 patched conics, switching the
/// central body each step according to [`dominant_body`]. Deterministic: the
/// same inputs always yield the same points (AGENTS.md section 44).
pub fn predict_patched_conics(
    bodies: &[GravityBody],
    position_m: DVec3,
    velocity_mps: DVec3,
    horizon_s: f64,
    step_s: f64,
) -> TrajectoryPrediction {
    assert!(
        !bodies.is_empty(),
        "patched-conics prediction requires at least one gravity body"
    );
    assert!(step_s > 0.0, "prediction step must be positive");
    let steps = if horizon_s <= 0.0 {
        0
    } else {
        (horizon_s / step_s).round().max(0.0) as usize
    };

    let mut r = position_m;
    let mut v = velocity_mps;
    let mut current_body = dominant_body(r, bodies);
    let mut points = Vec::with_capacity(steps + 1);
    let mut transitions = Vec::new();

    points.push(make_point(0.0, r, v, current_body, bodies));

    for s in 1..=steps {
        let body = dominant_body(r, bodies);
        if body != current_body {
            transitions.push(s);
            current_body = body;
        }
        let (r_next, v_next) = rk4_step(r, v, bodies, current_body, step_s);
        r = r_next;
        v = v_next;
        points.push(make_point(s as f64 * step_s, r, v, current_body, bodies));
    }

    TrajectoryPrediction {
        points,
        body_transitions: transitions,
    }
}

fn rk4_step(
    r: DVec3,
    v: DVec3,
    bodies: &[GravityBody],
    body_index: usize,
    dt: f64,
) -> (DVec3, DVec3) {
    let half = dt * 0.5;
    let a1 = central_accel(r, bodies, body_index);
    let r2 = r + v * half;
    let v2 = v + a1 * half;
    let a2 = central_accel(r2, bodies, body_index);
    let r3 = r + v2 * half;
    let v3 = v + a2 * half;
    let a3 = central_accel(r3, bodies, body_index);
    let r4 = r + v3 * dt;
    let v4 = v + a3 * dt;
    let a4 = central_accel(r4, bodies, body_index);

    let v_next = v + (a1 + a2 + a2 + a3 + a3 + a4) * (dt / 6.0);
    let r_next = r + (v + v2 + v2 + v3 + v3 + v4) * (dt / 6.0);
    (r_next, v_next)
}

fn make_point(
    time_s: f64,
    r: DVec3,
    v: DVec3,
    body_index: usize,
    bodies: &[GravityBody],
) -> PredictionPoint {
    let up = (r - bodies[body_index].position_m).normalize_or_zero();
    let (lat, lon) = direction_to_lat_lon(up);
    PredictionPoint {
        time_s,
        position_m: r,
        velocity_mps: v,
        body_index,
        lat_deg: lat,
        lon_deg: lon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::circular_orbit_speed_mps;

    const EARTH_MASS_KG: f64 = 5.97237e24;
    const SUN_MASS_KG: f64 = 1.989e30;
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    fn earth_at_origin() -> GravityBody {
        GravityBody::new("Earth", DVec3::ZERO, EARTH_MASS_KG)
    }

    #[test]
    fn circular_orbit_stays_on_sphere_after_one_period() {
        // Spec "trajectory prediction": an RK4 two-body conic on a circular
        // Earth orbit should keep a near-constant radius across one revolution.
        let body = earth_at_origin();
        let r = EARTH_RADIUS_M + 200_000.0;
        let v_circ = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        let period = 2.0 * std::f64::consts::PI * (r.powi(3) / body.mu()).sqrt();

        let pred = predict_patched_conics(
            &[body],
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v_circ),
            period,
            30.0,
        );
        assert!(pred.points.len() > 2);

        let (r0, v0) = (pred.points[0].position_m, pred.points[0].velocity_mps);
        let (r_end, v_end) = (
            pred.points.last().unwrap().position_m,
            pred.points.last().unwrap().velocity_mps,
        );
        // RK4 is 4th order: over one revolution the radius drifts ~O(h⁴) —
        // tens of centimetres at a 30 s step. A few metres is the honest
        // display-level bound (patched conics is a prediction aid, AGENTS.md 58).
        assert!(
            (r0.length() - r_end.length()).abs() < 5.0,
            "radial drift {} m over one orbit",
            (r0.length() - r_end.length()).abs()
        );
        assert!(
            (v0.length() - v_end.length()).abs() < 0.1,
            "speed drift {} m/s over one orbit",
            (v0.length() - v_end.length()).abs()
        );
        // No SOI transition with a single body.
        assert!(pred.body_transitions.is_empty());
        // Ground track stays on the sphere (finite lat/lon).
        assert!(pred
            .points
            .iter()
            .all(|p| p.lat_deg.is_finite() && p.lon_deg.is_finite()));
    }

    #[test]
    fn dominant_body_picks_strongest_local_gravity() {
        // A massive but distant body vs a light but close body: raw
        // acceleration (μ/r²) decides, so the close light body wins up close.
        let sun = GravityBody::new("Sun", DVec3::new(-1e11, 0.0, 0.0), SUN_MASS_KG);
        let earth = earth_at_origin();
        let bodies = [sun, earth];
        // Object 2 500 km from Earth's centre (well inside Earth's SOI).
        let near_earth = DVec3::new(2.5e6, 0.0, 0.0);
        assert_eq!(dominant_body(near_earth, &bodies), 1, "expected Earth");
        // Object close to the Sun, far from Earth.
        let near_sun = DVec3::new(-9.9e10, 0.0, 0.0);
        assert_eq!(dominant_body(near_sun, &bodies), 0, "expected Sun");
    }

    #[test]
    fn soi_crossing_switches_central_body() {
        // Spec "multi-body propagation": as the object moves from Earth toward
        // the Sun, the tracked central body must switch exactly once.
        let sun = GravityBody::new("Sun", DVec3::new(-1.5e11, 0.0, 0.0), SUN_MASS_KG);
        let earth = GravityBody::new("Earth", DVec3::ZERO, EARTH_MASS_KG);
        let bodies = [sun, earth];
        // Start 2 000 km above Earth, moving directly away from the Sun.
        let r0 = DVec3::new(EARTH_RADIUS_M + 2_000_000.0, 0.0, 0.0);
        let v0 = DVec3::new(0.0, 0.0, 11_000.0); // ~escape-ish, radial-ish in z
        let pred = predict_patched_conics(&bodies, r0, v0, 200_000.0, 500.0);

        let first_body = pred.points[0].body_index;
        let last_body = pred.points.last().unwrap().body_index;
        assert_eq!(first_body, 1, "must start dominated by Earth");
        assert_eq!(last_body, 0, "must end dominated by the Sun");
        assert!(
            !pred.body_transitions.is_empty(),
            "an SOI crossing must be recorded"
        );
        // Only one crossing in this simplified two-body system.
        assert_eq!(pred.body_transitions.len(), 1);
    }

    #[test]
    fn soi_radius_scales_with_mass_ratio() {
        // A heavier body has a larger sphere of influence.
        let small = sphere_of_influence_radius(1.5e11, EARTH_MASS_KG, SUN_MASS_KG);
        let big = sphere_of_influence_radius(1.5e11, 5.0 * EARTH_MASS_KG, SUN_MASS_KG);
        assert!(big > small, "heavier body must have a larger SOI");
        assert!(small > 0.0 && small.is_finite());
    }

    #[test]
    fn prediction_is_deterministic() {
        let sun = GravityBody::new("Sun", DVec3::new(-1.5e11, 0.0, 0.0), SUN_MASS_KG);
        let earth = earth_at_origin();
        let bodies = [sun, earth];
        let r0 = DVec3::new(EARTH_RADIUS_M + 300_000.0, 0.0, 0.0);
        let v0 = DVec3::new(0.0, 0.0, 7_900.0);
        let a = predict_patched_conics(&bodies, r0, v0, 10_000.0, 200.0);
        let b = predict_patched_conics(&bodies, r0, v0, 10_000.0, 200.0);
        assert_eq!(
            a.points, b.points,
            "identical inputs must yield identical predictions"
        );
        assert_eq!(a.body_transitions, b.body_transitions);
    }
}
