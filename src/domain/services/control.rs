//! Attitude control: what commands should achieve the guidance target.
//!
//! Control is the middle layer of the flight loop (AGENTS.md section 18): it
//! converts the guidance target attitude and the current attitude/rate into a
//! commanded torque using a PID with anti-windup. It never writes the vehicle's
//! motion; the actuation layer converts the torque into bounded actuator
//! commands (gimbal deflection, RCS).
//!
//! The controller works on the body-frame attitude error (rotation vector) and
//! damps angular rate. The gains are **normalized**: they express desired
//! angular acceleration (rad/s² per rad of error, etc.), and the commanded
//! torque is that acceleration times the body-frame inertia,
//! `τ = I ∘ (kp·e + ki·∫e·dt − kd·ω)`. This keeps the loop gain
//! vehicle-independent: the per-step damping factor is `kd·dt`, stable at any
//! vehicle scale, whereas absolute-torque gains tuned on a heavy vehicle are
//! numerically unstable (bang-bang against actuator clamps) on light ones.

use bevy::math::{DQuat, DVec3};

/// PID gains for the attitude controller, plus anti-windup and output clamps.
/// Gains are normalized (acceleration-space); see the module docs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PidGains {
    /// Proportional gain, rad/s² per rad of attitude error.
    pub kp: f64,
    /// Integral gain, rad/s² per rad·s of accumulated error.
    pub ki: f64,
    /// Derivative (rate damping) gain, 1/s. Stability of the discrete loop
    /// requires `kd < 2/dt`; at the 64 Hz fixed step that means kd < 128.
    pub kd: f64,
    /// Integral clamp magnitude (anti-windup), rad·s.
    pub integral_clamp: f64,
    /// Output torque clamp magnitude, N·m (applied after inertia weighting).
    pub output_clamp: f64,
}

impl Default for PidGains {
    fn default() -> Self {
        Self {
            // Effective accelerations previously produced by the absolute
            // gains on the Falcon-9-class reference vehicle (~2e8 N·m on
            // ~1.9e7 kg·m² transverse inertia); kept so existing tuned
            // behavior is preserved.
            kp: 10.0,
            ki: 0.25,
            kd: 8.0,
            integral_clamp: 5.0,
            output_clamp: 1.0e8,
        }
    }
}

/// The attitude error as a body-frame rotation vector (axis × angle): the
/// rotation the vehicle must perform about its own body axes to reach the
/// target attitude.
pub fn attitude_error_body(target: DQuat, current: DQuat) -> DVec3 {
    let q_err = target * current.conjugate();
    let axis = q_err.xyz();
    let len = axis.length();
    if len < 1e-12 {
        return DVec3::ZERO;
    }
    // Rotation vector in the world frame, then rotated into the body frame.
    let angle = 2.0 * len.atan2(q_err.w);
    let rotation_world = axis / len * angle;
    current.conjugate() * rotation_world
}

/// The attitude error magnitude in radians.
pub fn attitude_error_angle(target: DQuat, current: DQuat) -> f64 {
    let q_err = target * current.conjugate();
    2.0 * q_err.xyz().length().atan2(q_err.w).abs()
}

/// Update the integral term with anti-windup (clamp before accumulation).
pub fn integral_with_anti_windup(
    integral: DVec3,
    error_body: DVec3,
    dt: f64,
    integral_clamp: f64,
) -> DVec3 {
    let updated = integral + error_body * dt;
    if updated.length() > integral_clamp {
        updated.normalize() * integral_clamp
    } else {
        updated
    }
}

/// Clamp a commanded torque to a maximum magnitude.
pub fn clamp_torque(torque: DVec3, max_torque_nm: f64) -> DVec3 {
    let len = torque.length();
    if len > max_torque_nm && max_torque_nm > 0.0 {
        torque / len * max_torque_nm
    } else {
        torque
    }
}

/// Compute the commanded body-frame torque from the attitude error, angular
/// velocity, the body-frame inertia diagonal (the dynamics model is a
/// diagonal tensor), PID gains, and integral state. The gains are normalized
/// (acceleration-space), so the same values are stable for any vehicle mass:
/// `τ = I ∘ (kp·e + ki·∫e·dt − kd·ω)`. Returns the torque and the updated
/// integral (with anti-windup).
pub fn control_torque_body(
    target: DQuat,
    current: DQuat,
    angular_velocity_body: DVec3,
    inertia_diag: DVec3,
    gains: &PidGains,
    integral: &mut DVec3,
    dt: f64,
) -> DVec3 {
    let error = attitude_error_body(target, current);
    *integral = integral_with_anti_windup(*integral, error, dt, gains.integral_clamp);
    let angular_accel = gains.kp * error + gains.ki * *integral - gains.kd * angular_velocity_body;
    clamp_torque(inertia_diag * angular_accel, gains.output_clamp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;

    /// Falcon-9-class reference vehicle.
    const HEAVY_INERTIA: DVec3 = DVec3::splat(5.8e7);
    /// Electron-class small launcher: ~1.3e5 transverse, ~2.3e3 roll.
    const LIGHT_INERTIA: DVec3 = DVec3::new(1.34e5, 2.26e3, 1.34e5);

    fn state() -> RocketDynamicsState {
        RocketDynamicsState::new(
            DVec3::new(6_371_000.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            142_200.0,
            bevy::math::DMat3::from_diagonal(HEAVY_INERTIA),
            DVec3::ZERO,
        )
    }

    #[test]
    fn error_is_zero_for_aligned_attitudes() {
        let e = attitude_error_body(DQuat::IDENTITY, DQuat::IDENTITY);
        assert_eq!(e, DVec3::ZERO);
    }

    #[test]
    fn error_points_along_rotation_axis() {
        // Current is rotated 90° about +Y from the target.
        let target = DQuat::IDENTITY;
        let current = DQuat::from_rotation_y(90.0_f64.to_radians());
        let error = attitude_error_body(target, current);
        assert!(error.y < 0.0, "must rotate back about -Y");
        assert!((attitude_error_angle(target, current) - 90.0_f64.to_radians()).abs() < 1e-9);
    }

    #[test]
    fn pid_attitude_converges_with_bounded_overshoot() {
        let gains = PidGains::default();
        let target = DQuat::from_rotation_z(30.0_f64.to_radians());
        let mut dyn_state = state();
        let mut integral = DVec3::ZERO;
        let dt = 0.02;
        let steps = 2_000; // 40 s

        for _ in 0..steps {
            let torque = control_torque_body(
                target,
                dyn_state.orientation,
                dyn_state.angular_velocity_radps,
                HEAVY_INERTIA,
                &gains,
                &mut integral,
                dt,
            );
            dyn_state.integrate_rotation(torque, dt);
        }

        let error = attitude_error_angle(target, dyn_state.orientation);
        assert!(
            error < 0.05,
            "attitude did not converge: {error} rad after 40 s"
        );
        assert!(
            dyn_state.angular_velocity_radps.length() < 0.05,
            "rate not damped: {}",
            dyn_state.angular_velocity_radps.length()
        );
        // Bounded overshoot: never exceed ~1.5× the initial error during the
        // transient on the dominant axis.
        let mut peak: f64 = 0.0;
        let mut s = state();
        let mut i = DVec3::ZERO;
        for _ in 0..steps {
            let t = control_torque_body(
                target,
                s.orientation,
                s.angular_velocity_radps,
                HEAVY_INERTIA,
                &gains,
                &mut i,
                dt,
            );
            s.integrate_rotation(t, dt);
            peak = peak.max(attitude_error_angle(target, s.orientation));
        }
        let initial_error = 30.0_f64.to_radians();
        assert!(
            peak < initial_error * 1.5,
            "overshoot too large: {peak} vs initial {initial_error}"
        );
    }

    /// Regression (Phase 12 electron tower-tip): the same normalized gains
    /// must hold a LIGHT vehicle stable at the 64 Hz fixed step. Absolute-
    /// torque gains tuned on a heavy vehicle bang-banged the light vehicle's
    /// roll axis against the actuator clamp and tumbled it.
    #[test]
    fn normalized_gains_stable_on_light_vehicle_at_64hz() {
        let gains = PidGains::default();
        let target = DQuat::from_rotation_z(10.0_f64.to_radians());
        let mut orientation = DQuat::IDENTITY;
        let mut angular_velocity = DVec3::ZERO;
        let mut integral = DVec3::ZERO;
        let dt = 1.0 / 64.0;

        for _ in 0..640 {
            let torque = control_torque_body(
                target,
                orientation,
                angular_velocity,
                LIGHT_INERTIA,
                &gains,
                &mut integral,
                dt,
            );
            // Integrate with the light vehicle's diagonal inertia.
            let alpha = DVec3::new(
                torque.x / LIGHT_INERTIA.x,
                torque.y / LIGHT_INERTIA.y,
                torque.z / LIGHT_INERTIA.z,
            );
            angular_velocity += alpha * dt;
            orientation =
                (orientation * DQuat::from_scaled_axis(angular_velocity * dt)).normalize();
        }

        let error = attitude_error_angle(target, orientation);
        assert!(error < 0.05, "light vehicle did not converge: {error} rad");
        assert!(
            angular_velocity.length() < 0.5,
            "light vehicle rate not damped: {} rad/s",
            angular_velocity.length()
        );
    }

    #[test]
    fn integral_is_clamped_by_anti_windup() {
        let gains = PidGains::default();
        let mut integral = DVec3::ZERO;
        // Persistent large error must not grow the integral past the clamp.
        for _ in 0..100 {
            integral = integral_with_anti_windup(
                integral,
                DVec3::new(1.0, 0.0, 0.0),
                1.0,
                gains.integral_clamp,
            );
        }
        assert!(
            integral.length() <= gains.integral_clamp * (1.0 + 1e-12),
            "integral escaped anti-windup: {}",
            integral.length()
        );
    }

    #[test]
    fn output_is_clamped() {
        let torque = DVec3::new(1.0e10, 0.0, 0.0);
        let clamped = clamp_torque(torque, 2.0e8);
        assert!((clamped.length() - 2.0e8).abs() < 1e-6);
        assert!(clamped.x > 0.0);
        // Under-limit torque is unchanged.
        let small = DVec3::new(1.0, 2.0, 3.0);
        assert_eq!(clamp_torque(small, 2.0e8), small);
    }
}
