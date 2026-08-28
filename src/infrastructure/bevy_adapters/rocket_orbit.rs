//! Always-on orbit prediction line (feature: orbit prediction).
//!
//! Samples the patched-conics propagator ([`predict_patched_conics`], Phase 21)
//! over one orbital period and draws it as a gizmo polyline with apoapsis /
//! periapsis markers. Unlike the debug-only osculating-orbit gizmo, this runs
//! in every rocket camera mode and works from the authoritative physics state.
//! The prediction maths is a pure function ([`predicted_orbit`]) so it is
//! unit-testable without a renderer; the Bevy system only converts the
//! planet-centred points to the flight frame and draws them.

use crate::components::rocket::{
    GroundRest, PlannedManeuver, RocketMissionState, RocketPlanetBinding, RocketRenderState,
    TerrainCollisionState,
};
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::physics_orbital::apsis_endpoints_from_state;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::GroundContact;
use crate::domain::services::trajectory::{
    predict_patched_conics, predict_patched_conics_with_impulse, GravityBody, ManeuverImpulse,
    ManeuverPrediction,
};
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::rocket_presentation::interpolate_render_transform;
use crate::infrastructure::bevy_adapters::terrain_render::{recenter_render_origin, RenderOrigin};
use bevy::math::DVec3;
use bevy::prelude::*;

/// Predicted orbit: planet-centred sample points plus apoapsis/periapsis.
#[derive(Debug, Clone, PartialEq)]
pub struct OrbitPrediction {
    /// Planet-centred inertial sample positions (meters) along the trajectory.
    pub planet_frame_points: Vec<DVec3>,
    /// Seconds since the prediction start for each planet-frame sample. Kept
    /// alongside the line points so body-fixed presentation can account for
    /// planetary rotation without rerunning the predictor.
    pub planet_frame_times_s: Vec<f64>,
    /// Planet-centred apoapsis position, if a bound/apogee was found.
    pub apoapsis: Option<DVec3>,
    /// Planet-centred periapsis position, if a bound/perigee was found.
    pub periapsis: Option<DVec3>,
    /// The planned burn point, when it is reachable within this prediction.
    pub maneuver: Option<ManeuverPrediction>,
}

/// Cached presentation prediction shared by the flight gizmo and terrain map.
/// The cache is deliberately non-authoritative: simulation continues to own the
/// rocket state while presentation amortizes the relatively expensive propagator.
#[derive(Resource, Debug)]
pub struct OrbitPredictionCache {
    prediction: OrbitPrediction,
    prediction_start_sim_time_s: f64,
}

impl Default for OrbitPredictionCache {
    fn default() -> Self {
        Self {
            prediction: OrbitPrediction::empty(),
            prediction_start_sim_time_s: 0.0,
        }
    }
}

impl OrbitPredictionCache {
    pub fn prediction(&self) -> &OrbitPrediction {
        &self.prediction
    }

    pub fn prediction_start_sim_time_s(&self) -> f64 {
        self.prediction_start_sim_time_s
    }
}

/// Minimum radar altitude before a projected trajectory is meaningful flight
/// presentation. Near-surface ballistic arcs are not reliable orbit guidance.
pub const MIN_ORBIT_PREDICTION_ALTITUDE_M: f64 = 1_000.0;

/// Shared presentation policy for the flight-frame orbit line and terrain-map
/// prediction track. This reads contact/lifecycle state but never changes it.
pub fn orbit_prediction_allowed(
    mission: RocketMissionState,
    ground_contact: GroundContact,
    resting: bool,
    radar_altitude_m: f64,
) -> bool {
    !matches!(
        mission,
        RocketMissionState::PreLaunch | RocketMissionState::Landed | RocketMissionState::Crashed
    ) && ground_contact == GroundContact::None
        && !resting
        && radar_altitude_m.is_finite()
        && radar_altitude_m >= MIN_ORBIT_PREDICTION_ALTITUDE_M
}

impl OrbitPrediction {
    pub fn empty() -> Self {
        Self {
            planet_frame_points: Vec::new(),
            planet_frame_times_s: Vec::new(),
            apoapsis: None,
            periapsis: None,
            maneuver: None,
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
    surface_radius_m: f64,
) -> OrbitPrediction {
    predicted_orbit_with_maneuver(
        position_m,
        velocity_mps,
        planet_mass_kg,
        surface_radius_m,
        None,
    )
}

/// Like [`predicted_orbit`], with an optional presentation-only planned impulse.
pub fn predicted_orbit_with_maneuver(
    position_m: DVec3,
    velocity_mps: DVec3,
    planet_mass_kg: f64,
    surface_radius_m: f64,
    maneuver: Option<ManeuverImpulse>,
) -> OrbitPrediction {
    let speed = velocity_mps.length();
    if !position_m.is_finite()
        || !velocity_mps.is_finite()
        || !planet_mass_kg.is_finite()
        || planet_mass_kg <= 0.0
        || !surface_radius_m.is_finite()
        || surface_radius_m <= 0.0
        || !speed.is_finite()
        || speed < 1.0
        || position_m.length() <= surface_radius_m
        || maneuver.is_some_and(|maneuver| {
            !maneuver.execute_after_s.is_finite()
                || maneuver.execute_after_s < 0.0
                || !maneuver.delta_v_mps.is_finite()
        })
    {
        return OrbitPrediction::empty();
    }

    let mu = gravitational_parameter(planet_mass_kg);
    let r = position_m.length();
    if !mu.is_finite() || mu <= 0.0 || !r.is_finite() {
        return OrbitPrediction::empty();
    }
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
    .max(60.0);
    let step = (horizon / 160.0).max(1.0);
    if !horizon.is_finite() || !step.is_finite() {
        return OrbitPrediction::empty();
    }

    let body = GravityBody::new("central", DVec3::ZERO, planet_mass_kg);
    let pred = match maneuver {
        Some(maneuver) => match predict_patched_conics_with_impulse(
            &[body],
            position_m,
            velocity_mps,
            horizon,
            step,
            maneuver,
        ) {
            Ok(prediction) => prediction,
            Err(_) => return OrbitPrediction::empty(),
        },
        None => predict_patched_conics(&[body], position_m, velocity_mps, horizon, step),
    };

    let mut points = Vec::with_capacity(pred.points.len());
    let mut times_s = Vec::with_capacity(pred.points.len());
    points.push(position_m);
    times_s.push(0.0);
    let mut previous_position = position_m;
    let mut previous_time_s = 0.0;
    let mut intersects_surface = false;
    let mut impact_point_index = None;
    for (index, p) in pred.points.iter().enumerate().skip(1) {
        if !p.position_m.is_finite() || !p.time_s.is_finite() || p.time_s < previous_time_s {
            return OrbitPrediction::empty();
        }
        if let Some((impact_position, fraction)) =
            segment_surface_intersection(previous_position, p.position_m, surface_radius_m)
        {
            // Validate every rendered chord, not just sampled endpoints. A
            // coarse propagated arc can otherwise jump from one outside point
            // to another through the planet before its next sample.
            points.push(impact_position);
            times_s.push(previous_time_s + (p.time_s - previous_time_s) * fraction);
            intersects_surface = true;
            impact_point_index = Some(index);
            break;
        }
        points.push(p.position_m);
        times_s.push(p.time_s);
        previous_position = p.position_m;
        previous_time_s = p.time_s;
    }

    if points.len() < 2 {
        return OrbitPrediction::empty();
    }

    let apsis_state = pred
        .maneuver
        .map(|maneuver| (maneuver.position_m, maneuver.post_burn_velocity_mps))
        .unwrap_or((position_m, velocity_mps));
    let apsides = (!intersects_surface)
        .then(|| apsis_endpoints_from_state(apsis_state.0, apsis_state.1, mu))
        .flatten();
    let maneuver = pred.maneuver.filter(|maneuver| {
        impact_point_index
            .map(|impact_index| maneuver.pre_burn_point_index < impact_index)
            .unwrap_or(true)
    });

    OrbitPrediction {
        planet_frame_points: points,
        planet_frame_times_s: times_s,
        apoapsis: apsides.map(|apsides| apsides.apoapsis_position_m),
        periapsis: apsides.map(|apsides| apsides.periapsis_position_m),
        maneuver,
    }
}

/// Find the first intersection between an outside trajectory chord and the
/// planet's spherical visual surface. Both endpoints may be outside when a
/// coarse propagator would otherwise draw a chord through the surface.
fn segment_surface_intersection(start: DVec3, end: DVec3, radius_m: f64) -> Option<(DVec3, f64)> {
    let direction = end - start;
    let a = direction.length_squared();
    if a <= f64::EPSILON {
        return None;
    }
    let b = 2.0 * start.dot(direction);
    let c = start.length_squared() - radius_m * radius_m;
    let discriminant = b * b - 4.0 * a * c;
    if !discriminant.is_finite() || discriminant < 0.0 {
        return None;
    }
    let sqrt_discriminant = discriminant.sqrt();
    let near = (-b - sqrt_discriminant) / (2.0 * a);
    let far = (-b + sqrt_discriminant) / (2.0 * a);
    let fraction = [near, far]
        .into_iter()
        .find(|fraction| fraction.is_finite() && (0.0..=1.0).contains(fraction))?;
    Some((start.lerp(end, fraction), fraction))
}

fn planet_frame_to_flight(
    point_m: DVec3,
    render_origin: DVec3,
    physical_scale: &PhysicalScale,
) -> Vec3 {
    ((point_m - render_origin) * physical_scale.flight_display_units_per_meter as f64).as_vec3()
}

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct OrbitLineGizmos {}

/// Plugin that draws the always-on orbit prediction line in rocket mode.
pub struct RocketOrbitPlugin;

impl Plugin for RocketOrbitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitPredictionCache>()
            .init_gizmo_group::<OrbitLineGizmos>()
            .add_systems(
                Update,
                (update_orbit_prediction_cache, draw_orbit_prediction)
                    .chain()
                    .after(interpolate_render_transform)
                    .after(recenter_render_origin),
            );
    }
}

/// Refresh the shared prediction from the current interpolated flight state.
/// The terrain map consumes this same result, so this remains the only
/// presentation propagation path without allowing the flight line to lag behind
/// a fast-moving or time-warped vehicle.
#[allow(clippy::type_complexity)]
pub fn update_orbit_prediction_cache(
    planet_query: Query<&PlanetComponent>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketRenderState,
        &RocketMissionState,
        &TerrainCollisionState,
        &GroundRest,
        Option<&PlannedManeuver>,
    )>,
    fixed_time: Res<Time<Fixed>>,
    sim_time: Res<SimulationTime>,
    mut cache: ResMut<OrbitPredictionCache>,
) {
    let Some((binding, render, mission, collision, ground_rest, planned_maneuver)) =
        rocket_query.iter().next()
    else {
        return;
    };
    let Some(planet) = planet_query
        .iter()
        .find(|planet| planet.matches_body(&binding.planet_name))
    else {
        return;
    };

    let allowed = orbit_prediction_allowed(
        *mission,
        collision.ground_contact,
        ground_rest.active,
        collision.radar_altitude_m,
    );
    let planet_mass_kg = planet.domain_planet.mass_kg;
    let surface_radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let alpha = fixed_time.overstep_fraction() as f64;
    let position_m = render
        .prev
        .position_m
        .lerp(render.current.position_m, alpha);
    let velocity_mps = render
        .prev
        .velocity_mps
        .lerp(render.current.velocity_mps, alpha);
    let maneuver = planned_maneuver.and_then(|planned| {
        let execute_after_s = planned.execute_at_sim_time_s - sim_time.sim_time_s;
        (execute_after_s > 0.0 && execute_after_s.is_finite() && planned.delta_v_mps.is_finite())
            .then_some(ManeuverImpulse {
                execute_after_s,
                delta_v_mps: planned.delta_v_mps,
            })
    });

    cache.prediction = if allowed {
        predicted_orbit_with_maneuver(
            position_m,
            velocity_mps,
            planet_mass_kg,
            surface_radius_m,
            maneuver,
        )
    } else {
        OrbitPrediction::empty()
    };
    cache.prediction_start_sim_time_s = sim_time.sim_time_s;
}

/// Draw the predicted trajectory (and apoapsis/periapsis markers) in the
/// flight frame. The planet centre in flight units (meters) is at
/// `-render_origin.origin`, because the render origin tracks the rocket's
/// physics position in planet-centred inertial frame.
fn draw_orbit_prediction(
    render_origin: Res<RenderOrigin>,
    physical_scale: Res<PhysicalScale>,
    prediction_cache: Res<OrbitPredictionCache>,
    mut gizmos: Gizmos<OrbitLineGizmos>,
) {
    let pred = prediction_cache.prediction();
    if pred.planet_frame_points.len() < 2 {
        return;
    }

    let to_world = |p: DVec3| planet_frame_to_flight(p, render_origin.origin, &physical_scale);

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
    if let Some(maneuver) = pred.maneuver {
        gizmos.sphere(
            to_world(maneuver.position_m),
            10.0,
            Color::srgb(1.0, 0.8, 0.15),
        );
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
            EARTH_RADIUS_M,
        );
        assert!(pred.planet_frame_points.len() > 20);
        for p in &pred.planet_frame_points {
            assert!(
                (p.length() - r).abs() < r * 0.01,
                "predicted radius {} drifted from {r}",
                p.length()
            );
        }
        assert!(pred.apoapsis.is_none());
        assert!(pred.periapsis.is_none());
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
            EARTH_RADIUS_M,
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
            EARTH_RADIUS_M,
        );
        assert!(pred.planet_frame_points.is_empty());
        assert!(pred.apoapsis.is_none());
    }

    #[test]
    fn invalid_prediction_inputs_yield_an_empty_prediction() {
        let invalid_states = [
            (
                DVec3::new(f64::NAN, 0.0, 0.0),
                DVec3::X,
                EARTH_MASS_KG,
                EARTH_RADIUS_M,
            ),
            (
                DVec3::new(EARTH_RADIUS_M + 10_000.0, 0.0, 0.0),
                DVec3::NAN,
                EARTH_MASS_KG,
                EARTH_RADIUS_M,
            ),
            (
                DVec3::new(EARTH_RADIUS_M + 10_000.0, 0.0, 0.0),
                DVec3::X,
                0.0,
                EARTH_RADIUS_M,
            ),
            (
                DVec3::new(EARTH_RADIUS_M + 10_000.0, 0.0, 0.0),
                DVec3::X,
                EARTH_MASS_KG,
                0.0,
            ),
        ];

        for (position, velocity, mass, radius) in invalid_states {
            assert_eq!(
                predicted_orbit(position, velocity, mass, radius),
                OrbitPrediction::empty()
            );
        }
    }

    #[test]
    fn prediction_policy_hides_terminal_contact_and_low_altitude_states() {
        assert!(orbit_prediction_allowed(
            RocketMissionState::Ascent,
            GroundContact::None,
            false,
            MIN_ORBIT_PREDICTION_ALTITUDE_M,
        ));
        assert!(!orbit_prediction_allowed(
            RocketMissionState::Crashed,
            GroundContact::None,
            false,
            10_000.0,
        ));
        assert!(!orbit_prediction_allowed(
            RocketMissionState::Landed,
            GroundContact::None,
            false,
            10_000.0,
        ));
        assert!(!orbit_prediction_allowed(
            RocketMissionState::Landing,
            GroundContact::Landed,
            false,
            10_000.0,
        ));
        assert!(!orbit_prediction_allowed(
            RocketMissionState::Ascent,
            GroundContact::None,
            true,
            10_000.0,
        ));
        assert!(!orbit_prediction_allowed(
            RocketMissionState::Ascent,
            GroundContact::None,
            false,
            MIN_ORBIT_PREDICTION_ALTITUDE_M - 0.1,
        ));
    }

    #[test]
    fn prediction_is_deterministic() {
        let r = EARTH_RADIUS_M + 400_000.0;
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        let a = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
            EARTH_RADIUS_M,
        );
        let b = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
            EARTH_RADIUS_M,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn planned_prograde_impulse_marks_and_changes_the_prediction() {
        let r = EARTH_RADIUS_M + 400_000.0;
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        let baseline = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
            EARTH_RADIUS_M,
        );
        let with_maneuver = predicted_orbit_with_maneuver(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            EARTH_MASS_KG,
            EARTH_RADIUS_M,
            Some(ManeuverImpulse {
                execute_after_s: 120.0,
                delta_v_mps: DVec3::new(0.0, 0.0, 100.0),
            }),
        );

        assert!(with_maneuver.maneuver.is_some());
        assert_ne!(
            baseline.planet_frame_points,
            with_maneuver.planet_frame_points
        );
        assert!(
            with_maneuver.apoapsis.expect("post-burn apoapsis").length() > r,
            "a prograde impulse must raise apoapsis"
        );
    }

    #[test]
    fn suborbital_prediction_stops_at_the_surface() {
        let pred = predicted_orbit(
            DVec3::new(EARTH_RADIUS_M + 100_000.0, 0.0, 0.0),
            DVec3::new(-2_000.0, 0.0, 0.0),
            EARTH_MASS_KG,
            EARTH_RADIUS_M,
        );

        assert!(pred.planet_frame_points.len() > 1);
        assert!(pred
            .planet_frame_points
            .iter()
            .all(|point| point.length() >= EARTH_RADIUS_M - 1e-3));
        let impact = pred.planet_frame_points.last().unwrap();
        assert!((impact.length() - EARTH_RADIUS_M).abs() < 1e-3);
        assert!(pred.apoapsis.is_none());
        assert!(pred.periapsis.is_none());
    }

    #[test]
    fn surface_crossing_chord_stops_at_its_first_surface_intersection() {
        let start = DVec3::new(EARTH_RADIUS_M + 100.0, 0.0, 0.0);
        let end = DVec3::new(-EARTH_RADIUS_M - 100.0, 0.0, 0.0);
        let (impact, fraction) =
            segment_surface_intersection(start, end, EARTH_RADIUS_M).expect("surface crossing");

        assert!((impact.length() - EARTH_RADIUS_M).abs() < 1e-6);
        assert!(impact.x > 0.0, "must stop at the first, near-side crossing");
        assert!(fraction > 0.0 && fraction < 0.5);
    }

    #[test]
    fn flight_conversion_rebases_and_scales_prediction_points() {
        let scale = PhysicalScale {
            flight_display_units_per_meter: 0.5,
            flight_meters_per_display_unit: 2.0,
            ..PhysicalScale::default()
        };
        let point = planet_frame_to_flight(
            DVec3::new(6_371_100.0, 20.0, -10.0),
            DVec3::new(6_371_000.0, 0.0, 0.0),
            &scale,
        );
        assert_eq!(point, Vec3::new(50.0, 10.0, -5.0));
    }
}
