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
}
