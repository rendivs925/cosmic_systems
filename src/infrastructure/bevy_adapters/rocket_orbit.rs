//! Always-on orbit prediction line (feature: orbit prediction).
//!
//! Samples the patched-conics propagator ([`predict_patched_conics`], Phase 21)
//! over one orbital period and draws it as a gizmo polyline with apoapsis /
//! periapsis markers. Unlike the debug-only osculating-orbit gizmo, this runs
//! in every rocket camera mode and works from the authoritative physics state.
//! The prediction maths is a pure function ([`predicted_orbit`]) so it is
//! unit-testable without a renderer; the Bevy system only converts the
//! planet-centred points to the flight frame and draws them.

use crate::components::rocket::{RocketMissionState, RocketPhysicsState, RocketPlanetBinding};
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::trajectory::{predict_patched_conics, GravityBody};
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Predicted orbit: planet-centred sample points plus apoapsis/periapsis.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitPrediction {
    /// Planet-centred inertial sample positions (meters) along the trajectory.
    pub planet_frame_points: Vec<DVec3>,
    /// Planet-centred apoapsis position, if a bound/apogee was found.
    pub apoapsis: Option<DVec3>,
    /// Planet-centred periapsis position, if a bound/perigee was found.
    pub periapsis: Option<DVec3>,
}

impl OrbitPrediction {
    pub fn empty() -> Self {
        Self {
            planet_frame_points: Vec::new(),
            apoapsis: None,
            periapsis: None,
        }
    }
}

/// Propagate the rocket's planet-centred state for roughly one orbital period
/// using patched conics around a single body. Bound orbits use their period;
/// hyperbolic/sub-orbital states use a generous fixed arc. Returns an empty
/// prediction for a nearly-stationary vehicle (e.g. pad hold) or non-finite
/// state.
pub fn predicted_orbit(
    position_m: DVec3,
    velocity_mps: DVec3,
    planet_mass_kg: f64,
) -> OrbitPrediction {
    let speed = velocity_mps.length();
    if !position_m.length().is_finite() || !speed.is_finite() || speed < 1.0 {
        return OrbitPrediction::empty();
    }

    let mu = gravitational_parameter(planet_mass_kg);
    let r = position_m.length();
    let inv_a = 2.0 / r - speed * speed / mu;
    let semi_major = if inv_a > 1e-12 { 1.0 / inv_a } else { f64::NAN };
    let is_bound = semi_major.is_finite() && semi_major > 0.0;

    let horizon = if is_bound {
        // One orbital period plus a small margin so the loop closes.
        2.0 * std::f64::consts::PI * (semi_major.powi(3) / mu).sqrt() * 1.05
    } else {
        // Sub-orbital / hyperbolic: a few-hour arc so the view is informative
        // but bounded.
        4.0 * 3600.0
    }
    .clamp(60.0, 36_000.0);
    let step = (horizon / 160.0).max(1.0);

    let body = GravityBody::new("central", DVec3::ZERO, planet_mass_kg);
    let pred = predict_patched_conics(&[body], position_m, velocity_mps, horizon, step);

    let mut apoapsis: Option<(DVec3, f64)> = None;
    let mut periapsis: Option<(DVec3, f64)> = None;
    let mut points = Vec::with_capacity(pred.points.len());
    for p in &pred.points {
        let rad = p.position_m.length();
        points.push(p.position_m);
        match apoapsis {
            Some((_, d)) if d >= rad => {}
            _ => apoapsis = Some((p.position_m, rad)),
        }
        match periapsis {
            Some((_, d)) if d <= rad => {}
            _ => periapsis = Some((p.position_m, rad)),
        }
    }

    OrbitPrediction {
        planet_frame_points: points,
        apoapsis: apoapsis.map(|(p, _)| p),
        periapsis: periapsis.map(|(p, _)| p),
    }
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OrbitLineGizmos {}

/// Plugin that draws the always-on orbit prediction line in rocket mode.
pub struct RocketOrbitPlugin;

impl Plugin for RocketOrbitPlugin {
    fn build(&self, app: &mut App) {
        app.init_gizmo_group::<OrbitLineGizmos>()
            .add_systems(Update, draw_orbit_prediction);
    }
}

/// Draw the predicted trajectory (and apoapsis/periapsis markers) in the
/// flight frame. The planet centre in flight units (meters) is at
/// `-render_origin.origin`, because the render origin tracks the rocket's
/// physics position in planet-centred inertial frame.
fn draw_orbit_prediction(
    planet_query: Query<&PlanetComponent>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketMissionState,
    )>,
    render_origin: Res<RenderOrigin>,
    mut gizmos: Gizmos<OrbitLineGizmos>,
) {
    let Some((binding, rocket, mission)) = rocket_query.iter().next() else {
        return;
    };
    if *mission == RocketMissionState::PreLaunch {
        return;
    }
    let Some(planet) = planet_query
        .iter()
        .find(|p| p.domain_planet.name == binding.planet_name)
    else {
        return;
    };

    let pred = predicted_orbit(
        rocket.dynamics.position_m,
        rocket.dynamics.velocity_mps,
        planet.domain_planet.mass_kg,
    );
    if pred.planet_frame_points.len() < 2 {
        return;
    }

    // Planet centre in flight units (meters): render_origin tracks the rocket's
    // physics position, so planet centre = -origin.
    let planet_pos = -render_origin.origin;
    let to_world = |p: DVec3| (planet_pos + p).as_vec3();

    gizmos.linestrip(
        pred.planet_frame_points.iter().copied().map(to_world),
        Color::srgb(0.35, 0.75, 1.0),
    );
    if let Some(ap) = pred.apoapsis {
        gizmos.sphere(to_world(ap), 6.0, Color::srgb(0.2, 1.0, 0.4));
    }
    if let Some(pe) = pred.periapsis {
        gizmos.sphere(to_world(pe), 6.0, Color::srgb(1.0, 0.35, 0.35));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::circular_orbit_speed_mps;

    const EARTH_MASS_KG: f64 = 5.97237e24;
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    #[test]
    fn circular_orbit_prediction_stays_near_radius() {
        // Circular LEO: the predicted one-period polyline stays at the orbit
        // radius and apoapsis ≈ periapsis ≈ radius.
        let r = EARTH_RADIUS_M + 200_000.0;
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        let pred = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
        );
        assert!(pred.planet_frame_points.len() > 20);
        for p in &pred.planet_frame_points {
            assert!(
                (p.length() - r).abs() < r * 0.01,
                "predicted radius {} drifted from {r}",
                p.length()
            );
        }
        let ap = pred.apoapsis.expect("circular orbit has an apoapsis");
        let pe = pred.periapsis.expect("circular orbit has a periapsis");
        assert!((ap.length() - r).abs() < r * 0.01);
        assert!((pe.length() - r).abs() < r * 0.01);
    }

    #[test]
    fn elliptical_orbit_has_distinct_apoapsis_and_periapsis() {
        // A transfer-orbit-like state: high speed at periapsis gives an
        // eccentric orbit whose apoapsis is clearly above periapsis.
        let r = EARTH_RADIUS_M + 300_000.0;
        // ~10% above circular speed -> elliptical.
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r) * 1.10;
        let pred = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
        );
        let ap = pred.apoapsis.expect("apoapsis").length();
        let pe = pred.periapsis.expect("periapsis").length();
        assert!(
            ap > pe,
            "apoapsis {} must exceed periapsis {} for an eccentric orbit",
            ap,
            pe
        );
    }

    #[test]
    fn stationary_vehicle_yields_empty_prediction() {
        let pred = predicted_orbit(
            DVec3::new(EARTH_RADIUS_M + 2.0, 0.0, 0.0),
            DVec3::ZERO,
            EARTH_MASS_KG,
        );
        assert!(pred.planet_frame_points.is_empty());
        assert!(pred.apoapsis.is_none());
    }

    #[test]
    fn prediction_is_deterministic() {
        let r = EARTH_RADIUS_M + 400_000.0;
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        let a = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
        );
        let b = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
        );
        assert_eq!(a, b);
    }
}
