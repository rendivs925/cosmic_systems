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

/// Attitude (body→world) whose +Y body axis points along `direction` by the
/// shortest arc from world +Y. This leaves roll arbitrary; use
/// [`roll_preserving_attitude_from_direction`] when the current roll matters.
pub fn attitude_from_direction(direction: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::Y, direction)
}

/// Attitude whose body +Y points along `direction` while preserving the
/// roll of `current` as closely as possible: rotate the current attitude by
/// the shortest arc from its current longitudinal axis to `direction`.
///
/// A launch vehicle does not have a roll program unless one is commanded, and
/// `attitude_from_direction` picks an arbitrary roll. Using this helper for the
/// vertical hold and the gravity turn keeps the pad roll instead of slewing
/// the vehicle about its longitudinal axis at liftoff.
pub fn roll_preserving_attitude_from_direction(direction: DVec3, current: DQuat) -> DQuat {
    let direction = direction.try_normalize().unwrap_or(DVec3::Y);
    let current_y = (current * DVec3::Y).try_normalize().unwrap_or(DVec3::Y);
    (DQuat::from_rotation_arc(current_y, direction) * current).normalize()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roll_preserving_target_keeps_the_longitudinal_axis_and_roll() {
        // A pad-like attitude with a non-zero roll about the longitudinal axis.
        let current = DQuat::from_rotation_y(0.7) * DQuat::from_rotation_z(0.3);
        let current_y = current * DVec3::Y;

        // Same direction: the target is the current attitude, bit-for-bit.
        let same = roll_preserving_attitude_from_direction(current_y, current);
        assert!((same * DVec3::Y - current_y).length() < 1e-12);
        assert!(same.angle_between(current) < 1e-9);

        // New direction: body +Y follows it and the relative rotation has no
        // roll component about the current longitudinal axis.
        let direction = (current_y + DVec3::X * 0.2).normalize();
        let target = roll_preserving_attitude_from_direction(direction, current);
        assert!((target * DVec3::Y - direction).length() < 1e-12);
        let relative = target * current.conjugate();
        let axis = DVec3::new(relative.x, relative.y, relative.z);
        assert!(
            axis.dot(current_y).abs() < 1e-12,
            "the relative rotation must not roll about the longitudinal axis"
        );
    }
}
