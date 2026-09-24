//! Shared target-attitude helpers used by every guidance mode.

use crate::domain::math::{DQuat, DVec3};

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

/// Hold the longitudinal axis along `direction` and roll the vehicle about
/// that same body +Y axis by a reentry-bank command. Banking is attitude
/// control, not a raw torque on an arbitrary body axis.
pub fn banked_attitude_from_direction(direction: DVec3, bank_angle_rad: f64) -> DQuat {
    attitude_from_direction(direction) * DQuat::from_rotation_y(bank_angle_rad)
}
