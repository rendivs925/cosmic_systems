//! 6-DOF rigid-body dynamics for the rocket.
//!
//! Physics is the authoritative source of rocket motion: the rendered
//! [`Transform`](bevy::transform::components::Transform) is derived from this
//! state, never faked. Translation and rotation are integrated with semi-
//! implicit (symplectic) Euler in f64; higher-order integrators are a
//! documented future option once profiling evidence justifies them (AGENTS.md
//! section 41).
//!
//! Conventions:
//! - Position/velocity are in the planet-centered inertial meter frame.
//! - Orientation is a body→world quaternion; angular velocity and torque are
//!   in the body frame.
//! - The body +Y axis is the rocket's longitudinal (roll) axis.

use bevy::math::{DMat3, DQuat, DVec3};

/// Cohesive 6-DOF physical state of the rocket.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RocketDynamicsState {
    /// Planet-centered inertial position, meters.
    pub position_m: DVec3,
    /// Velocity, m/s.
    pub velocity_mps: DVec3,
    /// Body→world orientation.
    pub orientation: DQuat,
    /// Angular velocity in the body frame, rad/s.
    pub angular_velocity_radps: DVec3,
    /// Angular acceleration in the body frame, rad/s².
    pub angular_acceleration_radps2: DVec3,
    /// Total mass, kg.
    pub mass_kg: f64,
    /// Inertia tensor in the body frame, kg·m².
    pub inertia_body: DMat3,
    /// Center of mass in the body frame, meters.
    pub center_of_mass_m: DVec3,
}

impl RocketDynamicsState {
    pub fn new(
        position_m: DVec3,
        velocity_mps: DVec3,
        orientation: DQuat,
        mass_kg: f64,
        inertia_body: DMat3,
        center_of_mass_m: DVec3,
    ) -> Self {
        Self {
            position_m,
            velocity_mps,
            orientation,
            angular_velocity_radps: DVec3::ZERO,
            angular_acceleration_radps2: DVec3::ZERO,
            mass_kg,
            inertia_body,
            center_of_mass_m,
        }
    }

    /// Integrate translation with semi-implicit Euler: `v += (F/m)·dt`,
    /// then `p += v·dt`.
    pub fn integrate_translation(&mut self, net_force_n: DVec3, dt: f64) {
        if self.mass_kg > 0.0 {
            self.velocity_mps += net_force_n / self.mass_kg * dt;
        }
        self.position_m += self.velocity_mps * dt;
    }

    /// Integrate rotation with Euler for angular velocity and a normalized
    /// quaternion step: `α = I⁻¹(τ − ω × (Iω))`, `ω += α·dt`,
    /// `q = normalize(q · from_scaled_axis(ω·dt))`.
    pub fn integrate_rotation(&mut self, net_torque_nm: DVec3, dt: f64) {
        let angular_momentum = self.inertia_body * self.angular_velocity_radps;
        let gyroscopic = self.angular_velocity_radps.cross(angular_momentum);
        let alpha = self.inertia_body.inverse() * (net_torque_nm - gyroscopic);
        self.angular_acceleration_radps2 = alpha;
        self.angular_velocity_radps += alpha * dt;
        let delta = DQuat::from_scaled_axis(self.angular_velocity_radps * dt);
        self.orientation = (self.orientation * delta).normalize();
    }
}

/// Inertia tensor of a solid cylinder about its center, returned as
/// `(transverse_pitch_yaw, longitudinal_roll)` where the longitudinal axis is
/// +Y.
pub fn cylinder_inertia(mass_kg: f64, radius_m: f64, height_m: f64) -> (f64, f64) {
    let transverse = (1.0 / 12.0) * mass_kg * (3.0 * radius_m.powi(2) + height_m.powi(2));
    let longitudinal = 0.5 * mass_kg * radius_m.powi(2);
    (transverse, longitudinal)
}

/// Compute the body-frame inertia tensor and center of mass for a rocket
/// modeled as a dry cylindrical structure (full height, centered) plus a fuel
/// mass occupying the lower half of the envelope. Parallel-axis theorem shifts
/// each part to the combined center of mass, so the result updates as fuel is
/// consumed.
///
/// This is a documented geometric approximation; refined mass-distribution
/// models can replace it without changing the integration.
pub fn rocket_inertia_tensor(
    dry_mass_kg: f64,
    fuel_mass_kg: f64,
    radius_m: f64,
    height_m: f64,
) -> (DMat3, DVec3) {
    let fuel_height = height_m * 0.5;
    let fuel_center_y = -height_m * 0.25;
    let dry_center_y = 0.0;
    let total = (dry_mass_kg + fuel_mass_kg).max(1e-9);
    let com_y = (dry_mass_kg * dry_center_y + fuel_mass_kg * fuel_center_y) / total;

    let (dry_t, dry_l) = cylinder_inertia(dry_mass_kg, radius_m, height_m);
    let (fuel_t, fuel_l) = cylinder_inertia(fuel_mass_kg, radius_m, fuel_height);

    // Parallel-axis theorem for the transverse (pitch/yaw) axes; the
    // longitudinal (roll) axis is unaffected by a shift along the body Y.
    let transverse = dry_t
        + dry_mass_kg * (dry_center_y - com_y).powi(2)
        + fuel_t
        + fuel_mass_kg * (fuel_center_y - com_y).powi(2);
    let longitudinal = dry_l + fuel_l;

    (
        DMat3::from_diagonal(DVec3::new(transverse, longitudinal, transverse)),
        DVec3::new(0.0, com_y, 0.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const DRY: f64 = 22_200.0;
    const FUEL: f64 = 120_000.0;
    const RADIUS: f64 = 1.85;
    const HEIGHT: f64 = 70.0;

    fn test_state() -> RocketDynamicsState {
        let (inertia, com) = rocket_inertia_tensor(DRY, FUEL, RADIUS, HEIGHT);
        RocketDynamicsState::new(
            DVec3::new(6_371_000.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            DRY + FUEL,
            inertia,
            com,
        )
    }

    #[test]
    fn translation_integrates_under_force() {
        let mut state = test_state();
        let force = DVec3::new(100_000.0, 0.0, 0.0);
        state.integrate_translation(force, 1.0);
        // a = F/m = 100000/142200 ≈ 0.703 m/s², then position += v*dt.
        let expected_a = force / state.mass_kg;
        assert!((state.velocity_mps - expected_a).length() < 1e-9);
        assert!((state.position_m.x - (6_371_000.0 + expected_a.x)).abs() < 1e-9);
    }

    #[test]
    fn translation_reflects_mass_change() {
        let mut heavy = test_state();
        let mut light = test_state();
        light.mass_kg = DRY; // fuel exhausted
        let force = DVec3::new(100_000.0, 0.0, 0.0);
        heavy.integrate_translation(force, 1.0);
        light.integrate_translation(force, 1.0);
        assert!(
            light.velocity_mps.length() > heavy.velocity_mps.length(),
            "same force must accelerate a lighter rocket more"
        );
    }

    #[test]
    fn gravity_pulls_rocket_toward_body() {
        let mut state = test_state();
        let earth_mass = 5.97237e24;
        let mu = crate::domain::services::gravity::gravitational_parameter(earth_mass);
        let g = mu / state.position_m.length_squared();
        // Radial inward force, F = m*g (planet-centered frame).
        let force = -DVec3::new(g * state.mass_kg, 0.0, 0.0);
        state.integrate_translation(force, 0.01);
        assert!(state.velocity_mps.x < 0.0, "should fall toward the body");
        assert!(state.position_m.x < 6_371_000.0);
    }

    #[test]
    fn zero_torque_rotation_is_stable() {
        let mut state = test_state();
        state.angular_velocity_radps = DVec3::new(0.0, 0.2, 0.0);
        let initial_omega = state.angular_velocity_radps;
        for _ in 0..10_000 {
            state.integrate_rotation(DVec3::ZERO, 0.001);
        }
        // Angular velocity is constant without torque.
        assert!((state.angular_velocity_radps - initial_omega).length() < 1e-12);
        // Orientation stayed a unit quaternion.
        assert!((state.orientation.length() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn quaternion_stays_normalized_under_torque() {
        let mut state = test_state();
        state.angular_velocity_radps = DVec3::new(0.3, 0.1, -0.2);
        for _ in 0..5_000 {
            state.integrate_rotation(DVec3::new(50.0, 10.0, -20.0), 0.001);
        }
        assert!((state.orientation.length() - 1.0).abs() < 1e-9);
        // A full 2π rotation about Y returns near identity.
        let mut spin = test_state();
        spin.angular_velocity_radps = DVec3::new(0.0, 1.0, 0.0);
        let steps = 2_000;
        let dt = 2.0 * std::f64::consts::PI / steps as f64;
        for _ in 0..steps {
            spin.integrate_rotation(DVec3::ZERO, dt);
        }
        // Identity and -identity represent the same 2π rotation.
        let closeness = spin.orientation.dot(DQuat::IDENTITY).abs();
        assert!(
            closeness > 1.0 - 1e-9,
            "expected identity after 2π, dot = {closeness}"
        );
    }

    #[test]
    fn torque_about_principal_axis() {
        let mut state = test_state();
        // Diagonal inertia; torque about the longitudinal (Y) axis.
        let torque = DVec3::new(0.0, 5_000.0, 0.0);
        let i_yy = state.inertia_body.y_axis.y;
        let expected_alpha = torque.y / i_yy;
        state.integrate_rotation(torque, 1.0);
        assert!(
            (state.angular_velocity_radps.y - expected_alpha).abs() < 1e-6,
            "alpha should be tau/I, got {} vs {}",
            state.angular_velocity_radps.y,
            expected_alpha
        );
    }

    #[test]
    fn inertia_and_com_update_with_fuel() {
        let (full_i, full_com) = rocket_inertia_tensor(DRY, FUEL, RADIUS, HEIGHT);
        let (empty_i, empty_com) = rocket_inertia_tensor(DRY, 0.0, RADIUS, HEIGHT);
        // Full rocket is heavier in the lower half → COM below body center.
        assert!(full_com.y < 0.0);
        // Empty rocket is symmetric → COM at body center.
        assert!(empty_com.y.abs() < 1e-9);
        // Burn reduces transverse inertia (mass moved toward center).
        let full_t = full_i.x_axis.x;
        let empty_t = empty_i.x_axis.x;
        assert!(empty_t < full_t);
        // Rolling inertia scales with mass.
        assert!(empty_i.y_axis.y < full_i.y_axis.y);
    }
}
