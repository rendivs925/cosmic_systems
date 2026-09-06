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

use crate::domain::math::DVec3;
use serde::Deserialize;

/// Validated degree-two Earth gravity-model parameters.
///
/// `j2` is dimensionless and `reference_radius_m` is the gravity model's
/// documented equatorial reference radius, not a terrain or render radius.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct EarthJ2GravityModel {
    pub model_id: String,
    pub reference_radius_m: f64,
    pub j2: f64,
}

impl EarthJ2GravityModel {
    pub fn is_valid(&self) -> bool {
        !self.model_id.trim().is_empty()
            && self.reference_radius_m.is_finite()
            && self.reference_radius_m > 0.0
            && self.j2.is_finite()
            && self.j2 > 0.0
    }
}

/// Individually declared gravity terms in a named force-model tier.
///
/// All terms operate on a vehicle state in planet-centered inertial meters.
/// The tier declaration is intentionally separate from the fixed-pipeline
/// adapter so future terms extend the existing gravity authority rather than
/// introducing parallel force calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForceModelTerm {
    BoundBodyPointMass,
    EarthJ2,
    LunarThirdBody,
    SolarThirdBody,
}

/// Named deterministic gravity-model selections for flight and propagation.
///
/// `PlanetSun` documents the existing powered-flight model and is the default
/// to preserve its point-mass plus solar-tide behavior. The remaining tiers
/// declare the terms implemented by the following fidelity tasks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ForceModelTier {
    TwoBody,
    EarthJ2,
    EarthMoonSun,
    #[default]
    PlanetSun,
}

impl ForceModelTier {
    /// Terms selected by this tier, in stable reporting order.
    pub const fn active_terms(self) -> &'static [ForceModelTerm] {
        match self {
            Self::TwoBody => &[ForceModelTerm::BoundBodyPointMass],
            Self::EarthJ2 => &[ForceModelTerm::BoundBodyPointMass, ForceModelTerm::EarthJ2],
            Self::EarthMoonSun => &[
                ForceModelTerm::BoundBodyPointMass,
                ForceModelTerm::LunarThirdBody,
                ForceModelTerm::SolarThirdBody,
            ],
            Self::PlanetSun => &[
                ForceModelTerm::BoundBodyPointMass,
                ForceModelTerm::SolarThirdBody,
            ],
        }
    }
}

/// Immutable domain configuration for the selected gravity model.
///
/// Systems consume this by shared reference. It owns no body states, frame
/// conversions, or mutable simulation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ForceModelConfig {
    tier: ForceModelTier,
}

impl ForceModelConfig {
    pub const fn new(tier: ForceModelTier) -> Self {
        Self { tier }
    }

    pub const fn tier(self) -> ForceModelTier {
        self.tier
    }

    pub const fn active_terms(self) -> &'static [ForceModelTerm] {
        self.tier.active_terms()
    }

    /// Stable value used by telemetry and scientific-validation consumers.
    pub const fn validation_report(self) -> ForceModelReport {
        ForceModelReport {
            tier: self.tier,
            active_terms: self.active_terms(),
        }
    }
}

/// Force-model metadata reported alongside authoritative telemetry and
/// scientific-validation results. It contains no dynamic state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForceModelReport {
    pub tier: ForceModelTier,
    pub active_terms: &'static [ForceModelTerm],
}

impl Default for ForceModelReport {
    fn default() -> Self {
        ForceModelConfig::default().validation_report()
    }
}

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
    gravitational_acceleration_from_mu(
        gravitational_parameter(body_mass_kg),
        position_m,
        body_position_m,
    )
}

/// Gravitational acceleration from a validated standard gravitational
/// parameter `mu_m3_s2`. All positions remain in one inertial meter frame.
pub fn gravitational_acceleration_from_mu(
    mu_m3_s2: f64,
    position_m: DVec3,
    body_position_m: DVec3,
) -> DVec3 {
    let r = position_m - body_position_m;
    let r_sq = r.length_squared();
    if !mu_m3_s2.is_finite() || mu_m3_s2 <= 0.0 || r_sq < 1.0 {
        return DVec3::ZERO;
    }
    -mu_m3_s2 / r_sq * r.normalize()
}

/// Degree-two zonal-harmonic acceleration for Earth, in m/s².
///
/// `position_m` and the unit `spin_axis_inertial` are in the same
/// planet-centered inertial frame. The model is axisymmetric, so its PCK
/// orientation dependency is the shared instantaneous pole direction rather
/// than a separately maintained prime-meridian rotation.
pub fn earth_j2_acceleration(
    mu_m3_s2: f64,
    position_m: DVec3,
    spin_axis_inertial: DVec3,
    model: &EarthJ2GravityModel,
) -> DVec3 {
    let r_sq = position_m.length_squared();
    let spin_axis_sq = spin_axis_inertial.length_squared();
    if !mu_m3_s2.is_finite()
        || mu_m3_s2 <= 0.0
        || r_sq < 1.0
        || !spin_axis_sq.is_finite()
        || spin_axis_sq < f64::EPSILON
        || !model.is_valid()
    {
        return DVec3::ZERO;
    }

    let spin_axis = spin_axis_inertial / spin_axis_sq.sqrt();
    let z_m = position_m.dot(spin_axis);
    let z_ratio_squared = z_m * z_m / r_sq;
    let r_fifth_m5 = r_sq * r_sq * r_sq.sqrt();
    let factor = 1.5 * model.j2 * mu_m3_s2 * model.reference_radius_m.powi(2) / r_fifth_m5;
    if !factor.is_finite() {
        return DVec3::ZERO;
    }

    factor * (position_m * (5.0 * z_ratio_squared - 1.0) - 2.0 * z_m * spin_axis)
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
    differential_gravitational_acceleration_from_mu(
        gravitational_parameter(perturbing_body_mass_kg),
        vehicle_position_m,
        reference_origin_position_m,
        perturbing_body_position_m,
    )
}

/// Differential gravitational acceleration from a validated standard
/// gravitational parameter. This preserves the existing planet-centered
/// inertial-frame tide formulation.
pub fn differential_gravitational_acceleration_from_mu(
    perturbing_mu_m3_s2: f64,
    vehicle_position_m: DVec3,
    reference_origin_position_m: DVec3,
    perturbing_body_position_m: DVec3,
) -> DVec3 {
    gravitational_acceleration_from_mu(
        perturbing_mu_m3_s2,
        vehicle_position_m,
        perturbing_body_position_m,
    ) - gravitational_acceleration_from_mu(
        perturbing_mu_m3_s2,
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

    fn earth_j2_model() -> EarthJ2GravityModel {
        EarthJ2GravityModel {
            model_id: "test".to_string(),
            reference_radius_m: 6_378_136.3,
            j2: 1.082_626_173_852_222_7e-3,
        }
    }

    #[test]
    fn earth_j2_is_inward_at_the_equator_and_outward_at_the_pole() {
        let model = earth_j2_model();
        let mu_m3_s2 = 3.986_004_355_070_227e14;
        let radius_m = 6_778_136.3;
        let equatorial = earth_j2_acceleration(mu_m3_s2, DVec3::X * radius_m, DVec3::Z, &model);
        let polar = earth_j2_acceleration(mu_m3_s2, DVec3::Z * radius_m, DVec3::Z, &model);

        assert!(equatorial.x < 0.0);
        assert!(equatorial.y.abs() < 1.0e-15 && equatorial.z.abs() < 1.0e-15);
        assert!(polar.z > 0.0);
        assert!(polar.x.abs() < 1.0e-15 && polar.y.abs() < 1.0e-15);
    }

    #[test]
    fn earth_j2_uses_the_supplied_inertial_pole_direction() {
        let model = earth_j2_model();
        let acceleration = earth_j2_acceleration(
            3.986_004_355_070_227e14,
            DVec3::new(6_778_136.3, 0.0, 0.0),
            DVec3::X,
            &model,
        );

        assert!(acceleration.x > 0.0);
        assert!(acceleration.y.abs() < 1.0e-15 && acceleration.z.abs() < 1.0e-15);
    }

    #[test]
    fn earth_j2_equatorial_acceleration_matches_the_closed_form_magnitude() {
        let model = earth_j2_model();
        let mu_m3_s2 = 3.986_004_355_070_227e14;
        let radius_m = 6_778_136.3;
        let acceleration = earth_j2_acceleration(mu_m3_s2, DVec3::X * radius_m, DVec3::Z, &model);
        let expected_magnitude_mps2 =
            1.5 * model.j2 * mu_m3_s2 * model.reference_radius_m.powi(2) / radius_m.powi(4);

        assert!(acceleration.x < 0.0);
        assert!(
            (acceleration.length() - expected_magnitude_mps2).abs()
                < expected_magnitude_mps2 * 1.0e-14
        );
    }

    #[test]
    fn lunar_and_solar_differential_accelerations_are_additive_at_one_origin() {
        let vehicle_position_m = DVec3::new(6_778_136.3, -125_000.0, 80_000.0);
        let reference_origin_position_m = DVec3::ZERO;
        let moon_position_m = DVec3::new(384_400_000.0, 10_000_000.0, -3_000_000.0);
        let sun_position_m = DVec3::new(-149_597_870_700.0, 2_000_000_000.0, 5_000_000.0);
        let lunar_mu_m3_s2 = 4.904_869_5e12;
        let solar_mu_m3_s2 = 1.327_124_400_18e20;
        let lunar = differential_gravitational_acceleration_from_mu(
            lunar_mu_m3_s2,
            vehicle_position_m,
            reference_origin_position_m,
            moon_position_m,
        );
        let solar = differential_gravitational_acceleration_from_mu(
            solar_mu_m3_s2,
            vehicle_position_m,
            reference_origin_position_m,
            sun_position_m,
        );

        assert!(lunar.is_finite() && solar.is_finite());
        assert!(lunar.length() > 0.0 && solar.length() > 0.0);
        let expected =
            gravitational_acceleration_from_mu(lunar_mu_m3_s2, vehicle_position_m, moon_position_m)
                - gravitational_acceleration_from_mu(
                    lunar_mu_m3_s2,
                    reference_origin_position_m,
                    moon_position_m,
                )
                + gravitational_acceleration_from_mu(
                    solar_mu_m3_s2,
                    vehicle_position_m,
                    sun_position_m,
                )
                - gravitational_acceleration_from_mu(
                    solar_mu_m3_s2,
                    reference_origin_position_m,
                    sun_position_m,
                );
        assert!((lunar + solar).distance(expected) < 1.0e-18);
        assert_eq!(
            differential_gravitational_acceleration_from_mu(
                lunar_mu_m3_s2,
                reference_origin_position_m,
                reference_origin_position_m,
                moon_position_m,
            ) + differential_gravitational_acceleration_from_mu(
                solar_mu_m3_s2,
                reference_origin_position_m,
                reference_origin_position_m,
                sun_position_m,
            ),
            DVec3::ZERO
        );
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

    #[test]
    fn force_model_tiers_declare_stable_term_sets() {
        assert_eq!(
            ForceModelTier::TwoBody.active_terms(),
            &[ForceModelTerm::BoundBodyPointMass]
        );
        assert_eq!(
            ForceModelTier::EarthJ2.active_terms(),
            &[ForceModelTerm::BoundBodyPointMass, ForceModelTerm::EarthJ2]
        );
        assert_eq!(
            ForceModelTier::EarthMoonSun.active_terms(),
            &[
                ForceModelTerm::BoundBodyPointMass,
                ForceModelTerm::LunarThirdBody,
                ForceModelTerm::SolarThirdBody,
            ]
        );
    }

    #[test]
    fn default_force_model_reports_current_planet_sun_terms() {
        let report = ForceModelConfig::default().validation_report();

        assert_eq!(report.tier, ForceModelTier::PlanetSun);
        assert_eq!(
            report.active_terms,
            &[
                ForceModelTerm::BoundBodyPointMass,
                ForceModelTerm::SolarThirdBody,
            ]
        );
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
