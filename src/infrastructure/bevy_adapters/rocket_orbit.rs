//! Rocket orbit prediction (feature: orbit prediction).
//!
//! Samples the patched-conics propagator ([`predict_patched_conics`], Phase 21)
//! over one orbital period. The prediction is presentation data for the HUD and
//! terrain map, not a rendered flight-path claim. The pure
//! [`predicted_orbit`] function remains independently testable.

use crate::components::rocket::{
    GroundRest, RocketMissionState, RocketPhysicsState, RocketPlanetBinding, TerrainCollisionState,
};
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::gravity::{ForceModelConfig, ForceModelTier};
use crate::domain::services::long_arc_propagation::{
    LongArcIntegrationSettings, LongArcPropagationRequest, LongArcState, TwoBodyAccelerationModel,
};
use crate::domain::services::physics_orbital::apsis_endpoints_from_state;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::GroundContact;
use crate::domain::services::trajectory::{
    predict_patched_conics, predict_patched_conics_until_radius,
    predict_patched_conics_with_impulse, GravityBody, ManeuverImpulse, ManeuverPrediction,
};
#[cfg(test)]
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
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
    revision: u64,
    key: Option<OrbitPredictionKey>,
}

/// Exact invalidation input for the presentation-only propagated path. It uses
/// authoritative fixed-step state, rather than interpolated render state, so a
/// static render frame cannot cause a fresh allocation or propagation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct OrbitPredictionKey {
    position_m_bits: [u64; 3],
    velocity_mps_bits: [u64; 3],
    planet_mu_m3_s2_bits: u64,
    surface_radius_m_bits: u64,
    allowed: bool,
}

impl Default for OrbitPredictionCache {
    fn default() -> Self {
        Self {
            prediction: OrbitPrediction::empty(),
            prediction_start_sim_time_s: 0.0,
            revision: 0,
            key: None,
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

    fn clear(&mut self) {
        if self.prediction.planet_frame_points.is_empty() && self.key.is_none() {
            return;
        }
        self.prediction = OrbitPrediction::empty();
        self.key = None;
        self.revision = self.revision.wrapping_add(1);
    }
}

/// Minimum radar altitude before a projected trajectory is meaningful flight
/// presentation. Near-surface ballistic arcs are not reliable orbit guidance.
pub const MIN_ORBIT_PREDICTION_ALTITUDE_M: f64 = 1_000.0;
const IMPACT_PREDICTION_HORIZON_S: f64 = 1_800.0;
const IMPACT_PREDICTION_MAX_STEP_S: f64 = 0.5;

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
    planet_mu_m3_s2: f64,
    surface_radius_m: f64,
) -> OrbitPrediction {
    predicted_orbit_with_maneuver(
        position_m,
        velocity_mps,
        planet_mu_m3_s2,
        surface_radius_m,
        None,
    )
}

/// Like [`predicted_orbit`], with an optional presentation-only planned impulse.
pub fn predicted_orbit_with_maneuver(
    position_m: DVec3,
    velocity_mps: DVec3,
    planet_mu_m3_s2: f64,
    surface_radius_m: f64,
    maneuver: Option<ManeuverImpulse>,
) -> OrbitPrediction {
    let speed = velocity_mps.length();
    if !position_m.is_finite()
        || !velocity_mps.is_finite()
        || !planet_mu_m3_s2.is_finite()
        || planet_mu_m3_s2 <= 0.0
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

    let mu = planet_mu_m3_s2;
    let r = position_m.length();
    if !mu.is_finite() || mu <= 0.0 || !r.is_finite() {
        return OrbitPrediction::empty();
    }
    let inv_a = 2.0 / r - speed * speed / mu;
    let semi_major = if inv_a > 1e-12 { 1.0 / inv_a } else { f64::NAN };
    let is_bound = semi_major.is_finite() && semi_major > 0.0;

    let reaches_surface =
        trajectory_reaches_surface(position_m, velocity_mps, mu, surface_radius_m);
    let horizon = if reaches_surface {
        IMPACT_PREDICTION_HORIZON_S
    } else if is_bound {
        // One orbital period plus a small margin so the loop closes.
        2.0 * std::f64::consts::PI * (semi_major.powi(3) / mu).sqrt() * 1.05
    } else {
        // Sub-orbital / hyperbolic: a few-hour arc so the view is informative
        // but bounded.
        4.0 * 3600.0
    }
    .max(60.0);
    // Keep short impact-path chords so near-surface ballistic arcs do not
    // collapse into a single straight segment before terrain interception.
    let sample_count = if is_bound { 512.0 } else { 720.0 };
    let step = if reaches_surface {
        (horizon / sample_count).min(IMPACT_PREDICTION_MAX_STEP_S)
    } else {
        (horizon / sample_count).max(0.25)
    };
    if !horizon.is_finite() || !step.is_finite() {
        return OrbitPrediction::empty();
    }

    let body = GravityBody::from_gravitational_parameter("central", DVec3::ZERO, mu);
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
        None if reaches_surface => predict_patched_conics_until_radius(
            &[body],
            position_m,
            velocity_mps,
            horizon,
            step,
            Some(surface_radius_m),
        ),
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

/// Scientific two-body coast prediction for a kernel-mapped bound body.
///
/// This is presentation-only. It receives an owned copy of the current f64
/// state and reports an explicit `TwoBody` provenance through the long-arc
/// request; it cannot mutate rocket ECS state or replace the fixed flight
/// pipeline. Planned impulses and possible surface impacts deliberately retain
/// the existing patched-conics path until their respective long-arc force and
/// event contracts are implemented.
pub fn predicted_two_body_long_arc(
    position_m: DVec3,
    velocity_mps: DVec3,
    planet_mu_m3_s2: f64,
    surface_radius_m: f64,
    central_body: NaifBodyId,
) -> Option<OrbitPrediction> {
    let speed = velocity_mps.length();
    if !position_m.is_finite()
        || !velocity_mps.is_finite()
        || !planet_mu_m3_s2.is_finite()
        || planet_mu_m3_s2 <= 0.0
        || !surface_radius_m.is_finite()
        || surface_radius_m <= 0.0
        || !speed.is_finite()
        || speed < 1.0
        || position_m.length() <= surface_radius_m
        || trajectory_reaches_surface(position_m, velocity_mps, planet_mu_m3_s2, surface_radius_m)
    {
        return None;
    }

    let radius_m = position_m.length();
    let inverse_semi_major_axis = 2.0 / radius_m - speed * speed / planet_mu_m3_s2;
    let semi_major_axis_m =
        (inverse_semi_major_axis > 1.0e-12).then(|| 1.0 / inverse_semi_major_axis);
    let horizon_s = semi_major_axis_m
        .filter(|semi_major_axis_m| semi_major_axis_m.is_finite() && *semi_major_axis_m > 0.0)
        .map(|semi_major_axis_m| {
            std::f64::consts::TAU * (semi_major_axis_m.powi(3) / planet_mu_m3_s2).sqrt() * 1.05
        })
        .unwrap_or(4.0 * 3_600.0)
        .max(60.0);
    if !horizon_s.is_finite() {
        return None;
    }

    const SAMPLE_COUNT: usize = 512;
    let checkpoint_offsets_s = (1..=SAMPLE_COUNT)
        .map(|index| horizon_s * index as f64 / SAMPLE_COUNT as f64)
        .collect();
    let request = LongArcPropagationRequest::new(
        LongArcState::new(position_m, velocity_mps),
        crate::domain::services::ephemeris::TdbEpoch::j2000(),
        central_body,
        ForceModelConfig::new(ForceModelTier::TwoBody),
        LongArcIntegrationSettings::default(),
        horizon_s,
        checkpoint_offsets_s,
    )
    .ok()?;
    let acceleration_model = TwoBodyAccelerationModel::new(planet_mu_m3_s2).ok()?;
    let result = request.propagate_with(&acceleration_model).ok()?;

    let mut planet_frame_points = Vec::with_capacity(result.checkpoints.len() + 1);
    let mut planet_frame_times_s = Vec::with_capacity(result.checkpoints.len() + 1);
    planet_frame_points.push(position_m);
    planet_frame_times_s.push(0.0);
    for checkpoint in result.checkpoints {
        planet_frame_points.push(checkpoint.state.position_m);
        planet_frame_times_s.push(checkpoint.offset_s);
    }
    let apsides = apsis_endpoints_from_state(position_m, velocity_mps, planet_mu_m3_s2);
    Some(OrbitPrediction {
        planet_frame_points,
        planet_frame_times_s,
        apoapsis: apsides.map(|apsides| apsides.apoapsis_position_m),
        periapsis: apsides.map(|apsides| apsides.periapsis_position_m),
        maneuver: None,
    })
}

fn trajectory_reaches_surface(
    position_m: DVec3,
    velocity_mps: DVec3,
    mu: f64,
    surface_radius_m: f64,
) -> bool {
    let radius_m = position_m.length();
    let angular_momentum = position_m.cross(velocity_mps);
    if angular_momentum.length_squared() <= f64::EPSILON {
        let escape_speed_mps = (2.0 * mu / radius_m).sqrt();
        return velocity_mps.length() < escape_speed_mps;
    }

    apsis_endpoints_from_state(position_m, velocity_mps, mu)
        .is_some_and(|apsides| apsides.periapsis_position_m.length() <= surface_radius_m)
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

#[cfg(test)]
fn planet_frame_to_flight(
    point_m: DVec3,
    render_origin: DVec3,
    physical_scale: &PhysicalScale,
) -> Vec3 {
    ((point_m - render_origin) * physical_scale.flight_display_units_per_meter as f64).as_vec3()
}

/// Plugin that maintains the shared prediction consumed by flight telemetry and
/// the terrain map. The 3D line is intentionally disabled pending a validated
/// visual trajectory design.
pub struct RocketOrbitPlugin;

impl Plugin for RocketOrbitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitPredictionCache>()
            .add_systems(Update, update_orbit_prediction_cache);
    }
}

/// Refresh the shared prediction only when its authoritative input changes.
/// The terrain map consumes this same result, so this remains the only
/// presentation propagation path without allowing render FPS to determine
/// propagation work.
#[allow(clippy::type_complexity)]
pub fn update_orbit_prediction_cache(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketMissionState,
        &TerrainCollisionState,
        &GroundRest,
    )>,
    sim_time: Res<SimulationTime>,
    mut cache: ResMut<OrbitPredictionCache>,
) {
    let Some((binding, rocket, mission, collision, ground_rest)) = rocket_query.iter().next()
    else {
        cache.clear();
        return;
    };
    let Some(planet) = planet_query
        .iter()
        .find(|planet| planet.matches_body(&binding.planet_name))
    else {
        cache.clear();
        return;
    };

    let allowed = orbit_prediction_allowed(
        *mission,
        collision.ground_contact,
        ground_rest.active,
        collision.radar_altitude_m,
    );
    let Some(planet_mu_m3_s2) =
        ephemeris_snapshot.gravitational_parameter_for_catalog_body(&planet.domain_planet.name)
    else {
        cache.clear();
        return;
    };
    let surface_radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let position_m = rocket.dynamics.position_m;
    let velocity_mps = rocket.dynamics.velocity_mps;
    let key = OrbitPredictionKey {
        position_m_bits: dvec3_bits(position_m),
        velocity_mps_bits: dvec3_bits(velocity_mps),
        planet_mu_m3_s2_bits: planet_mu_m3_s2.to_bits(),
        surface_radius_m_bits: surface_radius_m.to_bits(),
        allowed,
    };
    if cache.key.as_ref() == Some(&key) {
        return;
    }

    cache.prediction = if allowed {
        let central_body = NaifBodyId::for_catalog_name(&planet.domain_planet.name);
        central_body
            .and_then(|central_body| {
                predicted_two_body_long_arc(
                    position_m,
                    velocity_mps,
                    planet_mu_m3_s2,
                    surface_radius_m,
                    central_body,
                )
            })
            .unwrap_or_else(|| {
                predicted_orbit_with_maneuver(
                    position_m,
                    velocity_mps,
                    planet_mu_m3_s2,
                    surface_radius_m,
                    None,
                )
            })
    } else {
        OrbitPrediction::empty()
    };
    cache.prediction_start_sim_time_s = sim_time.sim_time_s;
    cache.key = Some(key);
    cache.revision = cache.revision.wrapping_add(1);
}

fn dvec3_bits(value: DVec3) -> [u64; 3] {
    [value.x.to_bits(), value.y.to_bits(), value.z.to_bits()]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::gravitational_parameter;

    const EARTH_MASS_KG: f64 = 5.97237e24;
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    fn earth_mu_m3_s2() -> f64 {
        gravitational_parameter(EARTH_MASS_KG)
    }

    #[test]
    fn circular_orbit_prediction_stays_near_radius() {
        // Circular LEO: the predicted one-period polyline stays at the orbit
        // radius and apoapsis ≈ periapsis ≈ radius.
        let r = EARTH_RADIUS_M + 200_000.0;
        let v = (earth_mu_m3_s2() / r).sqrt();
        let pred = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
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
        let v = (earth_mu_m3_s2() / r).sqrt() * 1.10;
        let pred = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
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
            earth_mu_m3_s2(),
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
                earth_mu_m3_s2(),
                EARTH_RADIUS_M,
            ),
            (
                DVec3::new(EARTH_RADIUS_M + 10_000.0, 0.0, 0.0),
                DVec3::NAN,
                earth_mu_m3_s2(),
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
                earth_mu_m3_s2(),
                0.0,
            ),
        ];

        for (position, velocity, mu, radius) in invalid_states {
            assert_eq!(
                predicted_orbit(position, velocity, mu, radius),
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
        let v = (earth_mu_m3_s2() / r).sqrt();
        let a = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
        );
        let b = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
        );
        assert_eq!(a, b);
    }

    #[test]
    fn long_arc_coast_prediction_is_deterministic_and_keeps_the_fixed_state_read_only() {
        let radius_m = EARTH_RADIUS_M + 400_000.0;
        let position_m = DVec3::X * radius_m;
        let velocity_mps = DVec3::Y * (earth_mu_m3_s2() / radius_m).sqrt();

        let first = predicted_two_body_long_arc(
            position_m,
            velocity_mps,
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
            NaifBodyId::EARTH,
        )
        .expect("valid orbital coast has a long-arc prediction");
        let second = predicted_two_body_long_arc(
            position_m,
            velocity_mps,
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
            NaifBodyId::EARTH,
        )
        .expect("identical request remains valid");

        assert_eq!(first, second);
        assert_eq!(position_m, DVec3::X * radius_m);
        assert_eq!(
            velocity_mps,
            DVec3::Y * (earth_mu_m3_s2() / radius_m).sqrt()
        );
        assert_eq!(first.planet_frame_points.len(), 513);
        assert_eq!(first.planet_frame_times_s.len(), 513);
        assert!(first
            .planet_frame_points
            .iter()
            .all(|point| (point.length() - radius_m).abs() < 1.0e-2));
    }

    #[test]
    fn planned_prograde_impulse_marks_and_changes_the_prediction() {
        let r = EARTH_RADIUS_M + 400_000.0;
        let v = (earth_mu_m3_s2() / r).sqrt();
        let baseline = predicted_orbit(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
        );
        let with_maneuver = predicted_orbit_with_maneuver(
            DVec3::new(r, 0.0, 0.0),
            DVec3::new(0.0, 0.0, v),
            earth_mu_m3_s2(),
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
            earth_mu_m3_s2(),
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
    fn impact_prediction_retains_intermediate_ballistic_samples() {
        let pred = predicted_orbit(
            DVec3::new(EARTH_RADIUS_M + 2_000.0, 0.0, 0.0),
            DVec3::new(-100.0, 0.0, 0.0),
            earth_mu_m3_s2(),
            EARTH_RADIUS_M,
        );

        assert!(
            pred.planet_frame_points.len() > 10,
            "short impact paths must not collapse to one chord"
        );
        assert!(
            pred.planet_frame_times_s.windows(2).all(|times| {
                (times[1] - times[0]) <= IMPACT_PREDICTION_MAX_STEP_S + f64::EPSILON
            }),
            "impact samples must use the bounded propagation step"
        );
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

    #[test]
    fn prediction_cache_key_changes_with_authoritative_state() {
        let base = OrbitPredictionKey {
            position_m_bits: dvec3_bits(DVec3::new(1.0, 2.0, 3.0)),
            velocity_mps_bits: dvec3_bits(DVec3::new(4.0, 5.0, 6.0)),
            planet_mu_m3_s2_bits: earth_mu_m3_s2().to_bits(),
            surface_radius_m_bits: EARTH_RADIUS_M.to_bits(),
            allowed: true,
        };
        assert_eq!(base, base.clone());

        let mut after_fixed_step = base.clone();
        after_fixed_step.velocity_mps_bits = dvec3_bits(DVec3::new(4.0, 5.1, 6.0));
        assert_ne!(base, after_fixed_step);
    }
}
