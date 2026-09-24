//! Booster boostback (return-to-launch-site) guidance.

use super::attitude::attitude_from_direction;
use crate::domain::math::{DQuat, DVec3};

/// Horizontal distance to the pad below which boostback hands off to the
/// landing leg (m).
pub const BOOSTBACK_COMPLETE_DISTANCE_M: f64 = 5_000.0;
/// Horizontal speed below which boostback hands off (m/s).
pub const BOOSTBACK_COMPLETE_SPEED_MPS: f64 = 50.0;
/// Proportional gain on horizontal position error [1/s²]: at 50 km error this
/// commands ~2.5 m/s² of horizontal acceleration.
pub const BOOSTBACK_POSITION_GAIN_INV_S2: f64 = 5e-5;
/// Damping gain on horizontal velocity error [1/s].
pub const BOOSTBACK_VELOCITY_GAIN_INV_S: f64 = 0.02;

/// Command output of the boostback skeleton: target attitude and throttle
/// only — actuation and physics remain downstream (AGENTS.md section 18).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoostbackCommand {
    pub attitude: DQuat,
    /// Throttle fraction in [0, 1]; zero = coast.
    pub throttle: f64,
    /// True when the pad is roughly below and the landing leg should take
    /// over ([`super::AutopilotMode::Landing`]).
    pub complete: bool,
}

/// Booster flyback (RTLS) boostback targeting: a PD law on the horizontal
/// state relative to the launch site drives downrange-to-site toward zero.
/// Vertical dynamics are deliberately ignored here — the burn shapes the
/// downrange; the existing suicide-burn/hover-slam leg handles touchdown.
/// Pure function; testable without Bevy.
pub fn boostback_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    launch_site_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
) -> BoostbackCommand {
    let up = position_m.normalize_or_zero();
    let rel_site = launch_site_position_m - position_m;
    let horizontal_error = rel_site - up * rel_site.dot(up);
    let horizontal_velocity = velocity_mps - up * velocity_mps.dot(up);

    let complete = horizontal_error.length() < BOOSTBACK_COMPLETE_DISTANCE_M
        && horizontal_velocity.length() < BOOSTBACK_COMPLETE_SPEED_MPS;

    // PD command on the horizontal state, saturated by available thrust.
    let accel_cmd = horizontal_error * BOOSTBACK_POSITION_GAIN_INV_S2
        - horizontal_velocity * BOOSTBACK_VELOCITY_GAIN_INV_S;

    let accel_mag = accel_cmd.length();
    if accel_mag < 1e-6 || !accel_mag.is_finite() {
        return BoostbackCommand {
            attitude: attitude_from_direction(up),
            throttle: 0.0,
            complete,
        };
    }

    let available_accel_mps2 = max_thrust_n / mass_kg.max(1e-6);
    let throttle = (accel_mag / available_accel_mps2).clamp(0.05, 1.0);
    BoostbackCommand {
        attitude: attitude_from_direction(accel_cmd / accel_mag),
        throttle,
        complete,
    }
}
