//! Mission guidance: where the vehicle should go.
//!
//! Guidance is the first layer of the flight loop (AGENTS.md section 18):
//! mission → guidance → control → actuation → physics → state → guidance.
//! Guidance computes a target attitude (and phase transitions) from the
//! mission and the current state; it never commands actuators or writes the
//! vehicle's motion. All target generation is a pure function so it is
//! testable without Bevy.
//!
//! ## Ascent guidance
//!
//! A gravity-turn pitch-over: the vehicle holds the local vertical on the pad,
//! then pitches over toward the downrange direction by an angle that grows with
//! altitude, reaching [`AscentGuidanceProfile::max_turn_angle_rad`] by
//! [`AscentGuidanceProfile::turn_end_altitude_m`]. Orbit insertion aligns the
//! vehicle prograde (with the velocity vector).

use crate::domain::entities::rocket::RocketMissionState;
use bevy::math::{DQuat, DVec3};

/// Gravity-turn ascent profile.
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
}

impl Default for AscentGuidanceProfile {
    fn default() -> Self {
        Self {
            ascent_start_altitude_m: 5_000.0,
            turn_start_altitude_m: 2_000.0,
            turn_end_altitude_m: 80_000.0,
            max_turn_angle_rad: 80.0_f64.to_radians(),
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
        }
    }
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

/// The horizontal pitch axis for a pitch-over toward an azimuth: the
/// normalized cross of the local vertical and the reference direction. The
/// chosen reference fixes the ascent plane; the result is horizontal
/// (perpendicular to `up_dir`).
pub fn pitch_axis_from_reference(up_dir: DVec3, reference: DVec3) -> Option<DVec3> {
    let axis = up_dir.cross(reference);
    if axis.length_squared() < 1e-12 {
        None
    } else {
        Some(axis.normalize())
    }
}

/// Attitude (body→world) whose +Y body axis points along `direction`, with
/// minimal rotation (no roll).
pub fn attitude_from_direction(direction: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::Y, direction)
}

/// Prograde (velocity-aligned) target attitude. Falls back to identity for a
/// stationary vehicle.
pub fn prograde_attitude(velocity_mps: DVec3) -> DQuat {
    let speed = velocity_mps.length();
    if speed < 1e-6 {
        DQuat::IDENTITY
    } else {
        DQuat::from_rotation_arc(DVec3::Y, velocity_mps / speed)
    }
}

/// The guidance target attitude for a mission phase:
/// - PreLaunch / Launch: local vertical.
/// - Ascent: gravity turn toward the downrange plane.
/// - Orbit: prograde.
/// - Descent phases: hold the local vertical (a safe default; real descent
///   guidance is a later phase).
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
        _ => attitude_from_direction(up_dir),
    }
}

/// Advance the ascent mission phase from the current state:
/// - Launch → Ascent once above `ascent_start_altitude_m`.
/// - Ascent → Orbit once the speed reaches `circular_speed_mps` (within
///   `orbit_speed_fraction`).
pub fn advance_ascent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    speed_mps: f64,
    circular_speed_mps: f64,
    ascent_start_altitude_m: f64,
    orbit_speed_fraction: f64,
) -> RocketMissionState {
    match phase {
        RocketMissionState::Launch if altitude_m >= ascent_start_altitude_m => {
            RocketMissionState::Ascent
        }
        RocketMissionState::Ascent if speed_mps >= circular_speed_mps * orbit_speed_fraction => {
            RocketMissionState::Orbit
        }
        _ => phase,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::circular_orbit_speed_mps;
    use bevy::math::DVec3;

    fn profile() -> AscentGuidanceProfile {
        AscentGuidanceProfile::new(0.0, 0.0, 80_000.0, 80.0_f64.to_radians())
    }

    fn up_dir() -> DVec3 {
        DVec3::new(0.0, 1.0, 0.0)
    }

    #[test]
    fn gravity_turn_ramps_pitch_with_altitude() {
        let p = profile();
        assert_eq!(gravity_turn_pitch_angle(&p, 0.0), 0.0);
        assert_eq!(
            gravity_turn_pitch_angle(&p, 40_000.0),
            40.0_f64.to_radians()
        );
        assert!((gravity_turn_pitch_angle(&p, 80_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
        // Clamps beyond the profile.
        assert!((gravity_turn_pitch_angle(&p, 200_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
    }

    #[test]
    fn gravity_turn_direction_pitches_toward_azimuth() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let dir = gravity_turn_direction(&p, up_dir(), axis, 80_000.0);
        // Rotated 80° from vertical toward the horizontal plane.
        assert!((dir.dot(up_dir()) - 80.0_f64.to_radians().cos()).abs() < 1e-9);
        // Horizontal component grows with altitude.
        let vertical = gravity_turn_direction(&p, up_dir(), axis, 0.0);
        assert!((vertical - up_dir()).length() < 1e-9);
    }

    #[test]
    fn pitch_axis_is_horizontal_and_normalized() {
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        assert!((axis.length() - 1.0).abs() < 1e-12);
        assert!(axis.dot(up_dir()).abs() < 1e-12);
        assert!(pitch_axis_from_reference(DVec3::Z, DVec3::Z).is_none());
    }

    #[test]
    fn attitude_points_body_y_along_direction() {
        let dir = DVec3::new(0.0, 1.0, 0.0);
        let q = attitude_from_direction(dir);
        let body_y = q * DVec3::Y;
        assert!((body_y - dir).length() < 1e-9);
    }

    #[test]
    fn prograde_aligns_with_velocity() {
        let vel = DVec3::new(7_600.0, 0.0, 0.0);
        let q = prograde_attitude(vel);
        assert!((q * DVec3::Y - DVec3::X).length() < 1e-9);
        assert_eq!(prograde_attitude(DVec3::ZERO), DQuat::IDENTITY);
    }

    #[test]
    fn phase_selects_distinct_targets() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let vel = DVec3::new(7_600.0, 0.0, 0.0);

        let launch =
            target_attitude_for_phase(RocketMissionState::Launch, &p, up_dir(), axis, 0.0, vel);
        let ascent = target_attitude_for_phase(
            RocketMissionState::Ascent,
            &p,
            up_dir(),
            axis,
            80_000.0,
            vel,
        );
        let orbit =
            target_attitude_for_phase(RocketMissionState::Orbit, &p, up_dir(), axis, 80_000.0, vel);

        assert!((launch * DVec3::Y - up_dir()).length() < 1e-9);
        // Ascent tilts from vertical.
        assert!((ascent * DVec3::Y).dot(up_dir()) < 1.0 - 1e-3);
        // Orbit aligns with the velocity.
        assert!((orbit * DVec3::Y - DVec3::X).length() < 1e-9);
    }

    #[test]
    fn phase_advances_launch_ascent_orbit() {
        let circular = circular_orbit_speed_mps(5.97237e24, 6_571_000.0);
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Launch,
                1_000.0,
                0.0,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Launch
        );
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Launch,
                6_000.0,
                100.0,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Ascent
        );
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Ascent,
                200_000.0,
                circular,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Orbit
        );
        // Below orbital speed stays in ascent.
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Ascent,
                200_000.0,
                circular * 0.5,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Ascent
        );
    }
}
