//! Authoritative Newtonian gravity for vehicles.
//!
//! This is the single gravity implementation in the codebase (AGENTS.md
//! sections 16 and 50). Every vehicle or terrain consumer that needs
//! gravitational acceleration MUST use this module rather than defining its
//! own gravity calculation.
//!
//! Gravity is computed in the shared physical meter frame (the reference-frame
//! module's planet-centered inertial or local-tangent frames) and consumes the
//! real `Planet.mass_kg` values.

use bevy::math::DVec3;

/// Newtonian gravitational constant, m³·kg⁻¹·s⁻² (CODATA 2018).
///
/// This is the single authoritative definition; other modules reference it
/// instead of duplicating the value (AGENTS.md section 40).
pub const GRAVITATIONAL_CONSTANT: f64 = 6.67430e-11;

/// Standard gravitational parameter μ = G·M for a body, in m³·s⁻².
pub fn gravitational_parameter(body_mass_kg: f64) -> f64 {
    GRAVITATIONAL_CONSTANT * body_mass_kg
}

/// Circular orbital speed at radius `radius_m` around a body, m/s.
pub fn circular_orbit_speed_mps(body_mass_kg: f64, radius_m: f64) -> f64 {
    (gravitational_parameter(body_mass_kg) / radius_m).sqrt()
}

/// Gravitational acceleration (m/s²) at `position_m` due to a body of mass
/// `body_mass_kg` located at `body_position_m`, both in the same physical
/// meter frame (typically planet-centered inertial).
///
/// Uses Newton's law `a = -μ / |r|² · r̂`, pointing toward the body. Returns
/// zero inside a 1 m sphere around the body center to avoid the singularity.
pub fn gravitational_acceleration(
    body_mass_kg: f64,
    position_m: DVec3,
    body_position_m: DVec3,
) -> DVec3 {
    let r = position_m - body_position_m;
    let r_sq = r.length_squared();
    if r_sq < 1.0 {
        return DVec3::ZERO;
    }
    let mu = gravitational_parameter(body_mass_kg);
    -mu / r_sq * r.normalize()
}

/// Gravitational acceleration relative to an accelerating inertial-frame
/// origin. All positions are meters in one inertial frame. This preserves a
/// planet-centered inertial vehicle state while adding a perturbing body's
/// tidal acceleration, rather than incorrectly applying that body's full
/// heliocentric acceleration to the vehicle alone.
pub fn differential_gravitational_acceleration(
    perturbing_body_mass_kg: f64,
    vehicle_position_m: DVec3,
    reference_origin_position_m: DVec3,
    perturbing_body_position_m: DVec3,
) -> DVec3 {
    gravitational_acceleration(
        perturbing_body_mass_kg,
        vehicle_position_m,
        perturbing_body_position_m,
    ) - gravitational_acceleration(
        perturbing_body_mass_kg,
        reference_origin_position_m,
        perturbing_body_position_m,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const EARTH_MASS_KG: f64 = 5.97237e24;
    const EARTH_RADIUS_M: f64 = 6_371_000.0;

    #[test]
    fn earth_surface_acceleration_is_about_9_8() {
        let a = gravitational_acceleration(
            EARTH_MASS_KG,
            DVec3::new(EARTH_RADIUS_M, 0.0, 0.0),
            DVec3::ZERO,
        );
        let magnitude = a.length();
        let expected = gravitational_parameter(EARTH_MASS_KG) / (EARTH_RADIUS_M * EARTH_RADIUS_M);
        assert!(
            (magnitude - expected).abs() < expected * 1e-9,
            "does not match G*M/r²: {magnitude} vs {expected}"
        );
        assert!(
            (magnitude - 9.8).abs() < 0.1,
            "Earth surface g should be ≈ 9.8 m/s², got {magnitude}"
        );
        // Acceleration points toward the body center.
        assert!(a.x < 0.0);
        assert!(a.y.abs() < 1e-9 && a.z.abs() < 1e-9);
    }

    #[test]
    fn acceleration_scales_inverse_square_with_distance() {
        let r = EARTH_RADIUS_M;
        let a1 = gravitational_acceleration(EARTH_MASS_KG, DVec3::new(r, 0.0, 0.0), DVec3::ZERO)
            .length();
        let a2 =
            gravitational_acceleration(EARTH_MASS_KG, DVec3::new(2.0 * r, 0.0, 0.0), DVec3::ZERO)
                .length();
        assert!(
            (a2 - a1 / 4.0).abs() < a1 * 1e-6,
            "doubling distance should quarter acceleration: {a1} -> {a2}"
        );
    }

    #[test]
    fn circular_orbit_period_matches_kepler_under_gravity() {
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let r0 = 6_778_000.0; // 400 km circular LEO
        let v0 = (mu / r0).sqrt();
        let kepler_period = 2.0 * PI * (r0 * r0 * r0 / mu).sqrt();

        // Integrate one full orbit with the authoritative gravity function.
        let mut pos = DVec3::new(r0, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, v0);
        let dt = 1.0;
        let steps = (kepler_period / dt).ceil() as u32;
        for _ in 0..steps {
            vel += gravitational_acceleration(EARTH_MASS_KG, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
        }

        // After one period a stable circular orbit returns near its radius and
        // speed (semi-implicit Euler preserves the circular orbit).
        assert!(
            (pos.length() - r0).abs() < r0 * 1e-3,
            "orbit radius drifted from {r0} to {}",
            pos.length()
        );
        assert!(
            (vel.length() - v0).abs() < v0 * 1e-3,
            "orbital speed drifted from {v0} to {}",
            vel.length()
        );
    }

    #[test]
    fn gravity_points_toward_body_from_any_octant() {
        let body_pos = DVec3::new(1.0e6, 2.0e6, 3.0e6);
        let r = DVec3::new(6.4e6, 6.5e6, 6.6e6);
        let a = gravitational_acceleration(EARTH_MASS_KG, r, body_pos);
        let to_body = body_pos - r;
        assert!(a.dot(to_body) > 0.0, "acceleration must point at the body");
    }

    #[test]
    fn differential_gravity_is_zero_at_the_accelerating_origin() {
        let perturbing_body_position_m = DVec3::new(1.5e11, -2.0e9, 5.0e8);
        let reference_origin_position_m = DVec3::new(1.0e6, -2.0e6, 3.0e6);
        let acceleration = differential_gravitational_acceleration(
            1.9885e30,
            reference_origin_position_m,
            reference_origin_position_m,
            perturbing_body_position_m,
        );

        assert_eq!(acceleration, DVec3::ZERO);
    }

    #[test]
    fn differential_gravity_matches_the_difference_of_two_absolute_samples() {
        let sun_mass_kg = 1.9885e30;
        let earth_position_m = DVec3::new(-2.65e10, 0.0, 1.47e11);
        let vehicle_position_m = earth_position_m + DVec3::new(6.771e6, 0.0, 0.0);
        let expected = gravitational_acceleration(sun_mass_kg, vehicle_position_m, DVec3::ZERO)
            - gravitational_acceleration(sun_mass_kg, earth_position_m, DVec3::ZERO);
        let actual = differential_gravitational_acceleration(
            sun_mass_kg,
            vehicle_position_m,
            earth_position_m,
            DVec3::ZERO,
        );

        assert_eq!(actual, expected);
        assert!(actual.length() > 1.0e-7 && actual.length() < 1.0e-5);
    }

    #[test]
    fn singularity_returns_zero() {
        let a = gravitational_acceleration(EARTH_MASS_KG, DVec3::ZERO, DVec3::ZERO);
        assert_eq!(a, DVec3::ZERO);
    }

    #[test]
    fn circular_orbit_speed_is_sqrt_mu_over_r() {
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let r = 6_778_000.0;
        let v = circular_orbit_speed_mps(EARTH_MASS_KG, r);
        assert!((v - (mu / r).sqrt()).abs() < 1e-6);
        // 400 km LEO ≈ 7.67 km/s.
        assert!((v - 7_670.0).abs() < 60.0);
    }

    /// Scenario `circular_orbit_drift` (Phase 17): a circular orbit integrated
    /// with the authoritative gravity must stay circular. Semi-implicit Euler
    /// bounds the radial error rather than accumulating it, so the repo-wide
    /// 1e-3 relative bound applies over multiple revolutions (dt = 1 s).
    #[test]
    fn circular_orbit_holds_radius_and_energy_over_three_revolutions() {
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let r0 = 6_778_000.0;
        let v0 = (mu / r0).sqrt();
        let period = 2.0 * PI * (r0 * r0 * r0 / mu).sqrt();
        let dt = 1.0;
        let steps = (3.0 * period / dt) as u32;

        let mut pos = DVec3::new(r0, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, v0);
        let specific_energy =
            |pos: DVec3, vel: DVec3| vel.length_squared() / 2.0 - mu / pos.length();
        let e0 = specific_energy(pos, vel);

        let mut worst_radius_err = 0.0f64;
        for _ in 0..steps {
            vel += gravitational_acceleration(EARTH_MASS_KG, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
            worst_radius_err = worst_radius_err.max((pos.length() - r0).abs());
        }

        assert!(
            worst_radius_err < r0 * 1e-3,
            "circular orbit drifted radially by {} m",
            worst_radius_err
        );
        let e_final = specific_energy(pos, vel);
        assert!(
            ((e_final - e0) / e0.abs()).abs() < 1e-4,
            "specific energy drifted from {e0} to {e_final}"
        );
    }

    /// Scenario `escape_velocity` (Phase 17): departing with exactly the
    /// analytic escape speed `v_esc = √(2μ/r)` must be marginally unbound
    /// (specific energy ≈ 0), slightly more must genuinely escape, and
    /// distinctly less must fall back inward.
    #[test]
    fn escape_velocity_boundary_behaves_analytically() {
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let r0 = 6_778_000.0;
        let v_esc = (2.0 * mu / r0).sqrt();

        // Exact boundary: ε = 0 up to floating-point roundoff.
        let eps = v_esc * v_esc / 2.0 - mu / r0;
        assert!(eps.abs() < 1e-6, "escape-speed energy residual {eps}");

        // Slightly hyperbolic: must depart and conserve its positive energy.
        let delta: f64 = 0.01; // (v/v_esc)² − 1
        let v_start = v_esc * (1.0 + delta).sqrt();
        let e_expected = v_start * v_start / 2.0 - mu / r0;

        let dt = 1.0;
        let steps = 20_000; // 20_000 s
        let mut pos = DVec3::new(r0, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, v_start);
        for _ in 0..steps {
            vel += gravitational_acceleration(EARTH_MASS_KG, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
        }
        assert!(
            pos.length() > 5.0 * r0,
            "hyperbolic trajectory failed to escape (r = {})",
            pos.length()
        );
        let energy_final = vel.length_squared() / 2.0 - mu / pos.length();
        assert!(
            ((energy_final - e_expected) / e_expected).abs() < 5e-4,
            "escape energy drifted from {e_expected} to {energy_final}"
        );

        // Sub-escape: half the escape speed cannot depart; the vehicle falls
        // inward (bound ellipse with apoapsis at the start point).
        let v_sub = 0.5 * v_esc;
        let mut pos = DVec3::new(r0, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, v_sub);
        let mut min_r = r0;
        for _ in 0..steps {
            vel += gravitational_acceleration(EARTH_MASS_KG, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
            min_r = min_r.min(pos.length());
        }
        assert!(
            pos.length() < r0 * 0.95 || min_r < r0 * 0.95,
            "sub-escape trajectory should fall inward (r_end = {}, r_min = {})",
            pos.length(),
            min_r
        );
    }
}
