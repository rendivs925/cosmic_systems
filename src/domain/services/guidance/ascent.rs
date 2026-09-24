//! Ascent guidance: gravity-turn pitch-over, launch heading, and the ascent
//! phase transition.

use super::attitude::{attitude_from_direction, prograde_attitude};
use crate::domain::entities::rocket::RocketMissionState;
use crate::domain::math::{DQuat, DVec3};
use crate::domain::services::reference_frames::{
    planet_inertial_enu_basis, PlanetInertialEnuError,
};

/// Gravity-turn ascent profile with time-based pitch schedule.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscentGuidanceProfile {
    /// Altitude (m) at which Launch transitions to Ascent.
    pub ascent_start_altitude_m: f64,
    /// Altitude (m) at which the pitch-over begins.
    pub turn_start_altitude_m: f64,
    /// Altitude (m) at which the pitch-over reaches its maximum.
    pub turn_end_altitude_m: f64,
    /// Maximum pitch angle from the local vertical, radians.
    pub max_turn_angle_rad: f64,
    /// Time (s) after liftoff when pitch-over begins (for time-based schedule).
    pub turn_start_time_s: f64,
    /// Time (s) after liftoff when pitch-over ends.
    pub turn_end_time_s: f64,
    /// Target orbital inclination (radians) - determines launch azimuth.
    pub target_inclination_rad: f64,
    /// Pitch-gate: minimum altitude (m) the vehicle must reach before any
    /// pitch-over begins, so low-thrust vehicles clear the pad/tower first
    /// regardless of what the time schedule says.
    pub pitch_gate_min_altitude_m: f64,
    /// Pitch-gate: minimum vertical speed (m/s) required before pitch-over
    /// begins. Together with [`Self::pitch_gate_min_altitude_m`] this keeps
    /// the ascent vertical until the vehicle is genuinely flying.
    pub pitch_gate_min_vertical_speed_mps: f64,
}

impl Default for AscentGuidanceProfile {
    fn default() -> Self {
        Self {
            ascent_start_altitude_m: 5_000.0,
            turn_start_altitude_m: 2_000.0,
            turn_end_altitude_m: 80_000.0,
            max_turn_angle_rad: 80.0_f64.to_radians(),
            turn_start_time_s: 10.0,
            turn_end_time_s: 160.0,
            target_inclination_rad: 28.5_f64.to_radians(), // KSC latitude
            // Tower-clearance gates: ~8 vehicle heights up and climbing
            // decisively before the gravity turn may start.
            pitch_gate_min_altitude_m: 150.0,
            pitch_gate_min_vertical_speed_mps: 30.0,
        }
    }
}

impl AscentGuidanceProfile {
    pub fn new(
        ascent_start_altitude_m: f64,
        turn_start_altitude_m: f64,
        turn_end_altitude_m: f64,
        max_turn_angle_rad: f64,
    ) -> Self {
        Self {
            ascent_start_altitude_m,
            turn_start_altitude_m,
            turn_end_altitude_m,
            max_turn_angle_rad,
            turn_start_time_s: 10.0,
            turn_end_time_s: 160.0,
            target_inclination_rad: 28.5_f64.to_radians(),
            pitch_gate_min_altitude_m: 150.0,
            pitch_gate_min_vertical_speed_mps: 30.0,
        }
    }
}

/// A prograde, ascending-node horizontal launch solution in the local
/// planet-inertial ENU-like frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscendingNodeLaunchHeading {
    /// Unit horizontal heading in planet-centered inertial coordinates.
    pub direction_pci: DVec3,
    /// Azimuth east of north in radians.
    pub azimuth_east_of_north_rad: f64,
}

/// Reasons an inclination target cannot produce a safe local launch heading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AscendingNodeLaunchHeadingError {
    InvalidTargetInclination,
    InvalidLocalFrame(PlanetInertialEnuError),
    UnreachableInclination,
}

/// Compute the deterministic prograde ascending-node heading for an orbit
/// inclination measured from `spin_axis`.
///
/// `position_m` and `spin_axis` are planet-centered inertial vectors in meters
/// and unitless direction respectively. The local frame supplies east, north,
/// and up. The selected ascending solution has a non-negative north component
/// and satisfies `cos(i) = cos(latitude) * sin(azimuth)`, where azimuth is
/// measured east of north. The other mathematical solution is southbound and
/// would descend through the equator, so it is deliberately not selected.
///
/// An orbit is reachable only when `abs(cos(i)) <= cos(latitude)`. Polar sites
/// have no defined azimuth. Both cases return an error instead of substituting
/// a generic world axis that could steer into the wrong orbital plane.
pub fn prograde_ascending_node_launch_heading(
    position_m: DVec3,
    spin_axis: DVec3,
    target_inclination_rad: f64,
) -> Result<AscendingNodeLaunchHeading, AscendingNodeLaunchHeadingError> {
    if !target_inclination_rad.is_finite()
        || !(0.0..=std::f64::consts::PI).contains(&target_inclination_rad)
    {
        return Err(AscendingNodeLaunchHeadingError::InvalidTargetInclination);
    }

    let basis = planet_inertial_enu_basis(position_m, spin_axis)
        .map_err(AscendingNodeLaunchHeadingError::InvalidLocalFrame)?;
    let normalized_spin_axis = spin_axis.normalize();
    let cos_latitude = (1.0 - normalized_spin_axis.dot(basis.up).powi(2))
        .max(0.0)
        .sqrt();
    let sin_azimuth = target_inclination_rad.cos() / cos_latitude;
    if !sin_azimuth.is_finite() || sin_azimuth.abs() > 1.0 + 1.0e-12 {
        return Err(AscendingNodeLaunchHeadingError::UnreachableInclination);
    }

    let azimuth_east_of_north_rad = sin_azimuth.clamp(-1.0, 1.0).asin();
    let direction_pci = (basis.north * azimuth_east_of_north_rad.cos()
        + basis.east * azimuth_east_of_north_rad.sin())
    .normalize();
    Ok(AscendingNodeLaunchHeading {
        direction_pci,
        azimuth_east_of_north_rad,
    })
}

/// Pitch angle (radians from the local vertical) for the gravity turn at an
/// altitude, ramping from 0 at [`AscentGuidanceProfile::turn_start_altitude_m`]
/// to [`AscentGuidanceProfile::max_turn_angle_rad`] at
/// [`AscentGuidanceProfile::turn_end_altitude_m`].
pub fn gravity_turn_pitch_angle(profile: &AscentGuidanceProfile, altitude_m: f64) -> f64 {
    let t = ((altitude_m - profile.turn_start_altitude_m)
        / (profile.turn_end_altitude_m - profile.turn_start_altitude_m))
        .clamp(0.0, 1.0);
    profile.max_turn_angle_rad * t
}

/// Pitch angle for gravity turn using time-based schedule (more realistic).
/// Ramps from 0 at turn_start_time_s to max_turn_angle_rad at turn_end_time_s.
pub fn gravity_turn_pitch_angle_time(
    profile: &AscentGuidanceProfile,
    time_since_liftoff_s: f64,
) -> f64 {
    let t = ((time_since_liftoff_s - profile.turn_start_time_s)
        / (profile.turn_end_time_s - profile.turn_start_time_s))
        .clamp(0.0, 1.0);
    profile.max_turn_angle_rad * t
}

/// Combined pitch angle using both altitude and time (whichever is more advanced).
pub fn gravity_turn_pitch_angle_combined(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    time_since_liftoff_s: f64,
) -> f64 {
    let altitude_angle = gravity_turn_pitch_angle(profile, altitude_m);
    let time_angle = gravity_turn_pitch_angle_time(profile, time_since_liftoff_s);
    altitude_angle.max(time_angle)
}

/// True when the vehicle has cleared the pad/tower enough for the gravity
/// turn to begin: at or beyond the gate altitude AND vertical speed. Both
/// conditions use inclusive thresholds so a gate exactly met engages the turn.
pub fn ascent_pitch_gate_clear(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    vertical_speed_mps: f64,
) -> bool {
    altitude_m >= profile.pitch_gate_min_altitude_m
        && vertical_speed_mps >= profile.pitch_gate_min_vertical_speed_mps
}

/// Pitch angle of the gated ascent schedule: strictly vertical until the
/// tower-clearance gate passes, then the combined altitude/time schedule.
pub fn gravity_turn_pitch_angle_gated(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    time_since_liftoff_s: f64,
    vertical_speed_mps: f64,
) -> f64 {
    if !ascent_pitch_gate_clear(profile, altitude_m, vertical_speed_mps) {
        return 0.0;
    }
    gravity_turn_pitch_angle_combined(profile, altitude_m, time_since_liftoff_s)
}

/// Desired body-axis direction for the gravity turn: the local vertical
/// rotated about the pitch axis (horizontal, perpendicular to the ascent
/// plane) by the turn angle at the current altitude.
pub fn gravity_turn_direction(
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
) -> DVec3 {
    let angle = gravity_turn_pitch_angle(profile, altitude_m);
    (DQuat::from_axis_angle(pitch_axis, angle) * up_dir).normalize()
}

/// Desired body-axis direction for the gated ascent schedule: local vertical
/// until the tower-clearance gate passes (`altitude_m` and
/// `vertical_speed_mps` at or beyond the profile's gate), then the combined
/// altitude/time pitch schedule.
pub fn gravity_turn_direction_gated(
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
    time_since_liftoff_s: f64,
    vertical_speed_mps: f64,
) -> DVec3 {
    let angle = gravity_turn_pitch_angle_gated(
        profile,
        altitude_m,
        time_since_liftoff_s,
        vertical_speed_mps,
    );
    (DQuat::from_axis_angle(pitch_axis, angle) * up_dir).normalize()
}

/// Advance the ascent mission phase from the current state:
/// - Launch → Ascent once above `ascent_start_altitude_m`.
/// - Ascent → Orbit only after the authoritative state satisfies the configured
///   target-orbit predicate. Raw speed alone misclassifies boosted trajectories
///   with an Earth-intersecting periapsis.
pub fn advance_ascent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    ascent_start_altitude_m: f64,
    target_orbit_reached: bool,
) -> RocketMissionState {
    match phase {
        RocketMissionState::Launch if altitude_m >= ascent_start_altitude_m => {
            RocketMissionState::Ascent
        }
        RocketMissionState::Ascent if target_orbit_reached => RocketMissionState::Orbit,
        _ => phase,
    }
}

/// The guidance target attitude for a mission phase:
/// - PreLaunch / Launch: local vertical.
/// - Ascent: gravity turn toward the downrange plane.
/// - Orbit: prograde.
/// - DeorbitBurn: retrograde.
/// - ReentryCorridor: bank angle modulated (handled by control system).
/// - PoweredDescent: thrust-aligned.
/// - UnpoweredDescent: vertical (handled by parafoil control).
/// - Landing: local vertical.
pub fn target_attitude_for_phase(
    phase: RocketMissionState,
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
    velocity_mps: DVec3,
) -> DQuat {
    match phase {
        RocketMissionState::PreLaunch | RocketMissionState::Launch => {
            attitude_from_direction(up_dir)
        }
        RocketMissionState::Ascent => attitude_from_direction(gravity_turn_direction(
            profile, up_dir, pitch_axis, altitude_m,
        )),
        RocketMissionState::Orbit => prograde_attitude(velocity_mps),
        RocketMissionState::DeorbitBurn => {
            // Retrograde attitude.
            let speed = velocity_mps.length();
            if speed > 1e-6 {
                attitude_from_direction(-velocity_mps / speed)
            } else {
                attitude_from_direction(up_dir)
            }
        }
        RocketMissionState::ReentryCorridor => {
            // Bank angle is managed by control system; hold angle of attack.
            attitude_from_direction(up_dir)
        }
        RocketMissionState::PoweredDescent => {
            // Thrust-aligned attitude (computed by powered_descent_guidance).
            attitude_from_direction(up_dir)
        }
        RocketMissionState::UnpoweredDescent => {
            // Vertical descent; parafoil handles lateral.
            attitude_from_direction(up_dir)
        }
        RocketMissionState::Landing => attitude_from_direction(up_dir),
        _ => attitude_from_direction(up_dir),
    }
}
