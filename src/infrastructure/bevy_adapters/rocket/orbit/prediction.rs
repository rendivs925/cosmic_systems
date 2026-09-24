//! Pure orbit-prediction types and the patched-conics sampling API.

use super::super::components::RocketMissionState;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::gravity::{ForceModelConfig, ForceModelTier};
use crate::domain::services::long_arc_propagation::{
    LongArcIntegrationSettings, LongArcPropagationRequest, LongArcState, TwoBodyAccelerationModel,
};
use crate::domain::services::physics_orbital::apsis_endpoints_from_state;
use crate::domain::services::terrain_collision::GroundContact;
use crate::domain::services::trajectory::{
    predict_patched_conics, predict_patched_conics_until_radius,
    predict_patched_conics_with_impulse, GravityBody, ManeuverImpulse, ManeuverPrediction,
};
use bevy::math::DVec3;

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

/// Minimum radar altitude before a projected trajectory is meaningful flight
/// presentation. Near-surface ballistic arcs are not reliable orbit guidance.
pub const MIN_ORBIT_PREDICTION_ALTITUDE_M: f64 = 1_000.0;
const IMPACT_PREDICTION_HORIZON_S: f64 = 1_800.0;
pub(super) const IMPACT_PREDICTION_MAX_STEP_S: f64 = 0.5;

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
pub(super) fn segment_surface_intersection(
    start: DVec3,
    end: DVec3,
    radius_m: f64,
) -> Option<(DVec3, f64)> {
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
