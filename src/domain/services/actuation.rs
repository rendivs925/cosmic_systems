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

/// Default mechanical deflection range of a grid fin, ±30° (spec).
pub const GRID_FIN_MAX_DEFLECTION_RAD: f64 = 30.0_f64.to_radians();

/// X-configuration grid-fin mixing signs: rows are the four fins (at 45°,
/// 135°, 225°, 315° azimuth around the body +Y roll axis), columns are
/// (pitch X, roll Y, yaw Z). A `+1` means the fin deflects to contribute
/// positive torque about that body axis. This is the classic balanced X-mixer
/// used on control-fin vehicles.
const GRID_FIN_MIX: [[f64; 3]; 4] = [
    [1.0, 1.0, 1.0],
    [1.0, -1.0, -1.0],
    [-1.0, -1.0, 1.0],
    [-1.0, 1.0, -1.0],
];

/// Grid-fin aerodynamic effectiveness as a function of Mach (spec "hypersonic
/// grid-fin effectiveness": reduced fin effectiveness and shock interactions).
/// Full in the subsonic regime, tapering to a 0.35 floor at hypersonic speed
/// (10 g/cm³-scale plasma seals the gaps and shock interactions dominate).
pub fn grid_fin_effectiveness(mach: f64) -> f64 {
    let taper = 1.0 / (1.0 + 0.1 * mach.max(0.0));
    (0.35 + 0.65 * taper).clamp(0.0, 1.0)
}

/// Mix a normalized body-torque command (`[-1, 1]` per axis) into four grid-fin
/// deflections (radians), scaled by aerodynamic effectiveness and clamped to
/// the ±30° mechanical range (spec "grid-fin deflection limits"). Returns the
/// fin deflections in fin order `[45°, 135°, 225°, 315°]`.
pub fn grid_fin_mixer(desired_torque_body: DVec3, max_deflection_rad: f64, mach: f64) -> [f64; 4] {
    let effectiveness = grid_fin_effectiveness(mach);
    let range = max_deflection_rad.abs();
    let mut deflections = [0.0_f64; 4];
    for (i, mix_row) in GRID_FIN_MIX.iter().enumerate() {
        // Scale so a unit torque command maps to (at most) full deflection.
        let raw = mix_row[0] * desired_torque_body.x
            + mix_row[1] * desired_torque_body.y
            + mix_row[2] * desired_torque_body.z;
        deflections[i] = (raw * effectiveness * range).clamp(-range, range);
    }
    deflections
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
    fn grid_fin_deflections_are_clamped_to_mechanical_range() {
        // Spec "grid-fin deflection limits": ±30° typical, never exceeded even
        // for a saturated command on every axis.
        let saturate = grid_fin_mixer(DVec3::new(3.0, 3.0, 3.0), GRID_FIN_MAX_DEFLECTION_RAD, 0.1);
        for d in saturate {
            assert!(d.abs() <= GRID_FIN_MAX_DEFLECTION_RAD + 1e-12);
        }
    }

    #[test]
    fn hypersonic_flight_reduces_fin_deflection() {
        // Spec "hypersonic grid-fin effectiveness": the same command deflects
        // the fins far less at Mach 5 than at Mach 0.5.
        let cmd = DVec3::new(1.0, 0.0, 0.0);
        let subsonic = grid_fin_mixer(cmd, GRID_FIN_MAX_DEFLECTION_RAD, 0.5);
        let hypersonic = grid_fin_mixer(cmd, GRID_FIN_MAX_DEFLECTION_RAD, 5.0);
        for (a, b) in subsonic.iter().zip(hypersonic.iter()) {
            assert!(
                b.abs() < a.abs(),
                "Mach 5 must deflect the fin less ({b} vs {a})"
            );
        }
        assert!(grid_fin_effectiveness(5.0) < grid_fin_effectiveness(0.5));
        // Monotonically non-increasing in Mach.
        let mut last = grid_fin_effectiveness(0.0);
        for m in [0.5, 1.0, 2.0, 4.0, 8.0] {
            let e = grid_fin_effectiveness(m);
            assert!(e <= last);
            last = e;
        }
    }

    #[test]
    fn pure_pitch_command_symmetrically_deflects_fins() {
        // A pure pitch command mixes into all four fins with the X-pattern
        // magnitudes (the fins are the authority for pitch/yaw/roll).
        let d = grid_fin_mixer(DVec3::new(1.0, 0.0, 0.0), GRID_FIN_MAX_DEFLECTION_RAD, 0.0);
        assert!(
            d.iter().all(|x| x.abs() > 0.0),
            "all fins must act on a pitch command"
        );
        // Symmetric about the pitch axis.
        assert!(
            (d[0] + d[3]).abs() < 1e-12,
            "fin 0 and fin 3 are mirror inputs"
        );
    }
}
