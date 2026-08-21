//! Actuation: apply physical actuator limits before forces reach physics.
//!
//! Actuation is the last command layer of the flight loop (AGENTS.md section
//! 18): it converts the control layer's commands into bounded actuator outputs
//! (throttle rate limit, gimbal deflection clamp, RCS torque clamp). Physics
//! integrates only the bounded outputs.

use bevy::math::DVec3;

/// Physical limits of the vehicle's actuators.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActuationLimits {
    /// Maximum throttle rate of change, per second (0..1 range).
    pub max_throttle_slew_per_s: f32,
    /// Maximum RCS torque magnitude per body axis, N·m.
    pub max_rcs_torque_nm: f64,
    /// Maximum gimbal deflection, radians (matched to the engine range).
    pub max_gimbal_deflection_rad: f32,
}

impl Default for ActuationLimits {
    fn default() -> Self {
        Self {
            max_throttle_slew_per_s: 2.0,
            max_rcs_torque_nm: 5.0e7,
            max_gimbal_deflection_rad: 5.0_f32.to_radians(),
        }
    }
}

/// Limit a throttle command's rate of change to the actuator slew rate.
pub fn limit_throttle_slew(current: f32, desired: f32, max_slew_per_s: f32, dt: f32) -> f32 {
    let delta = desired - current;
    let max_delta = max_slew_per_s * dt.max(0.0);
    let bounded = delta.clamp(-max_delta, max_delta);
    (current + bounded).clamp(0.0, 1.0)
}

/// Clamp a commanded deflection to the actuator's mechanical range.
pub fn clamp_deflection(deflection_rad: f32, max_deflection_rad: f32) -> f32 {
    let max_abs = max_deflection_rad.abs();
    deflection_rad.clamp(-max_abs, max_abs)
}

/// Clamp an RCS torque command per axis to the maximum torque.
pub fn clamp_rcs_torque(torque: DVec3, max_torque_nm: f64) -> DVec3 {
    DVec3::new(
        torque.x.clamp(-max_torque_nm, max_torque_nm),
        torque.y.clamp(-max_torque_nm, max_torque_nm),
        torque.z.clamp(-max_torque_nm, max_torque_nm),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn throttle_slew_limits_rate_of_change() {
        // Instant command of 1.0 limited to a 0.4/s slew over 0.1 s.
        let limited = limit_throttle_slew(0.0, 1.0, 2.0, 0.1);
        assert!((limited - 0.2).abs() < 1e-6);
        // Over time the throttle catches up to the command.
        let mut throttle = 0.0;
        for _ in 0..20 {
            throttle = limit_throttle_slew(throttle, 1.0, 2.0, 0.1);
        }
        assert!((throttle - 1.0).abs() < 1e-6);
        // Cannot go negative or above 1.
        assert_eq!(limit_throttle_slew(0.0, -5.0, 2.0, 0.1), 0.0);
        assert!(limit_throttle_slew(0.99, 5.0, 2.0, 0.1) <= 1.0);
    }

    #[test]
    fn gimbal_deflection_is_clamped_to_range() {
        assert_eq!(
            clamp_deflection(1.0, 5.0_f32.to_radians()),
            5.0_f32.to_radians()
        );
        assert_eq!(
            clamp_deflection(-1.0, 5.0_f32.to_radians()),
            -5.0_f32.to_radians()
        );
        assert!((clamp_deflection(0.02, 5.0_f32.to_radians()) - 0.02).abs() < 1e-9);
    }

    #[test]
    fn rcs_torque_is_clamped_per_axis() {
        let clamped = clamp_rcs_torque(DVec3::new(1.0e9, -2.0e9, 1.0e7), 5.0e7);
        assert_eq!(clamped, DVec3::new(5.0e7, -5.0e7, 1.0e7));
    }
}
