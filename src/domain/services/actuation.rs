//! Actuation: apply physical actuator limits before forces reach physics.
//!
//! Actuation is the last command layer of the flight loop (AGENTS.md section
//! 18): it converts the control layer's commands into bounded actuator outputs
//! (throttle rate limit, gimbal deflection clamp, RCS torque clamp). Physics
//! integrates only the bounded outputs.

use crate::domain::math::DVec3;

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
            // Representative physical cluster scale (a few kN of thrust at a
            // few metres), not the 5e7 control authority the old roll-slew
            // target required. See SC-04 in
            // docs/scientific_correctness_audit.md.
            max_rcs_torque_nm: 5.0e3,
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

/// Default mechanical deflection range of a grid fin, ±30° (spec).
pub const GRID_FIN_MAX_DEFLECTION_RAD: f64 = 30.0_f64.to_radians();

/// Grid-fin aerodynamic effectiveness as a function of Mach (spec "hypersonic
/// grid-fin effectiveness": reduced fin effectiveness and shock interactions).
/// Full in the subsonic regime, tapering to a 0.35 floor at hypersonic speed
/// (10 g/cm³-scale plasma seals the gaps and shock interactions dominate).
pub fn grid_fin_effectiveness(mach: f64) -> f64 {
    let taper = 1.0 / (1.0 + 0.1 * mach.max(0.0));
    (0.35 + 0.65 * taper).clamp(0.0, 1.0)
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

    #[test]
    fn grid_fin_effectiveness_tapers_monotonically_with_mach() {
        // Spec "hypersonic grid-fin effectiveness": reduced fin effectiveness
        // and shock interactions as Mach rises.
        assert!(grid_fin_effectiveness(5.0) < grid_fin_effectiveness(0.5));
        let mut last = grid_fin_effectiveness(0.0);
        for m in [0.5, 1.0, 2.0, 4.0, 8.0] {
            let e = grid_fin_effectiveness(m);
            assert!(e <= last);
            last = e;
        }
    }
}
