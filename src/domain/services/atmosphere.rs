//! Per-planet atmosphere models.
//!
//! The single authoritative atmosphere implementation (AGENTS.md sections 19
//! and 50): every subsystem that needs temperature, pressure, density, or
//! speed of sound by altitude consumes this module instead of defining its own
//! formulas. Each planet carries an [`AtmosphereSource`]; planets without an
//! atmosphere use [`VacuumAtmosphere`].
//!
//! ## Earth model
//!
//! An ISA-style reference (troposphere with a linear lapse rate, isothermal
//! stratosphere) valid for 0–~80 km with acceptable engineering accuracy:
//!
//! - 0–11 km (troposphere): `T = T0 − L·h`, pressure via the barometric
//!   formula `P = P0·(T/T0)^(g0/(R·L))`, density `ρ = P/(R·T)`.
//! - Above 11 km: isothermal at 216.65 K with exponential pressure decay
//!   (stratosphere and above simplified to a single isothermal layer).
//!
//! Units are SI (kelvin, pascals, kg/m³, m/s). The model is an approximation;
//! a real ISA implementation or measured data can replace the formulas behind
//! the same trait.

use crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2;
use std::fmt::Debug;
use std::sync::Arc;

/// Standard atmosphere sea-level density, kg/m³.
pub const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
/// Standard atmosphere sea-level temperature, kelvin.
pub const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;
/// Standard atmosphere sea-level pressure, pascals.
pub const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;
/// Troposphere lapse rate, K/m.
pub const TROPOSPHERE_LAPSE_RATE_K_PER_M: f64 = 0.0065;
/// Tropopause altitude, meters.
pub const TROPOPAUSE_M: f64 = 11_000.0;
/// Isothermal stratosphere temperature, kelvin.
pub const STRATOSPHERE_TEMPERATURE_K: f64 = 216.65;
/// Specific gas constant of air, J/(kg·K).
pub const SPECIFIC_GAS_CONSTANT_AIR: f64 = 287.05;
/// Ratio of specific heats of air.
pub const HEAT_CAPACITY_RATIO_AIR: f64 = 1.4;

/// Atmospheric state at an altitude, all SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereProperties {
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_mps: f64,
}

/// A planet's atmosphere model: returns the state at a given geometric
/// altitude above mean radius.
pub trait AtmosphereSource: Send + Sync + Debug {
    fn properties(&self, altitude_m: f64) -> AtmosphereProperties;
}

/// ISA-style Earth atmosphere (see module docs).
#[derive(Debug, Default)]
pub struct EarthAtmosphere;

impl AtmosphereSource for EarthAtmosphere {
    fn properties(&self, altitude_m: f64) -> AtmosphereProperties {
        let h = altitude_m.max(0.0);
        let (temperature_k, pressure_pa) = if h <= TROPOPAUSE_M {
            let t = SEA_LEVEL_TEMPERATURE_K - TROPOSPHERE_LAPSE_RATE_K_PER_M * h;
            let exponent = STANDARD_GRAVITY_MPS2
                / (SPECIFIC_GAS_CONSTANT_AIR * TROPOSPHERE_LAPSE_RATE_K_PER_M);
            let p = SEA_LEVEL_PRESSURE_PA * (t / SEA_LEVEL_TEMPERATURE_K).powf(exponent);
            (t, p)
        } else {
            let t = STRATOSPHERE_TEMPERATURE_K;
            let p_tropopause = SEA_LEVEL_PRESSURE_PA
                * (STRATOSPHERE_TEMPERATURE_K / SEA_LEVEL_TEMPERATURE_K).powf(
                    STANDARD_GRAVITY_MPS2
                        / (SPECIFIC_GAS_CONSTANT_AIR * TROPOSPHERE_LAPSE_RATE_K_PER_M),
                );
            let p = p_tropopause
                * (-(STANDARD_GRAVITY_MPS2 * (h - TROPOPAUSE_M)) / (SPECIFIC_GAS_CONSTANT_AIR * t))
                    .exp();
            (t, p)
        };
        let density_kg_m3 = pressure_pa / (SPECIFIC_GAS_CONSTANT_AIR * temperature_k);
        let speed_of_sound_mps =
            (HEAT_CAPACITY_RATIO_AIR * SPECIFIC_GAS_CONSTANT_AIR * temperature_k).sqrt();
        AtmosphereProperties {
            temperature_k,
            pressure_pa,
            density_kg_m3,
            speed_of_sound_mps,
        }
    }
}

/// Bodies with no atmosphere: zero pressure, temperature, density, and speed
/// of sound at every altitude.
#[derive(Debug, Default)]
pub struct VacuumAtmosphere;

impl AtmosphereSource for VacuumAtmosphere {
    fn properties(&self, _altitude_m: f64) -> AtmosphereProperties {
        AtmosphereProperties {
            temperature_k: 0.0,
            pressure_pa: 0.0,
            density_kg_m3: 0.0,
            speed_of_sound_mps: 0.0,
        }
    }
}

/// The atmosphere source for a planet by name. Earth gets the real ISA-style
/// model; all other bodies are treated as vacuum for now. Mars/Venus models
/// are a documented future extension behind the same trait.
pub fn atmosphere_for(name: &str) -> Arc<dyn AtmosphereSource> {
    match name {
        "Earth" => Arc::new(EarthAtmosphere),
        _ => Arc::new(VacuumAtmosphere),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_sea_level_matches_isa_reference() {
        let earth = EarthAtmosphere;
        let props = earth.properties(0.0);
        assert!((props.temperature_k - 288.15).abs() < 0.01);
        assert!((props.pressure_pa - 101_325.0).abs() < 101_325.0 * 0.01);
        assert!((props.density_kg_m3 - 1.225).abs() < 1.225 * 0.01);
        // a = sqrt(gamma * R * T) ≈ 340.3 m/s at sea level.
        assert!((props.speed_of_sound_mps - 340.3).abs() < 1.0);
    }

    #[test]
    fn earth_tropopause_temperature() {
        let props = EarthAtmosphere.properties(11_000.0);
        assert!((props.temperature_k - 216.65).abs() < 0.01);
        // Standard pressure at 11 km ≈ 22 632 Pa.
        assert!((props.pressure_pa - 22_632.0).abs() < 22_632.0 * 0.02);
    }

    #[test]
    fn density_decreases_with_altitude() {
        let earth = EarthAtmosphere;
        let sea = earth.properties(0.0);
        let high = earth.properties(10_000.0);
        assert!(high.density_kg_m3 < sea.density_kg_m3);
        assert!(high.pressure_pa < sea.pressure_pa);
        assert!(high.speed_of_sound_mps < sea.speed_of_sound_mps);
        assert!(high.temperature_k < sea.temperature_k);
    }

    #[test]
    fn stratosphere_is_isothermal() {
        let earth = EarthAtmosphere;
        let a = earth.properties(15_000.0);
        let b = earth.properties(20_000.0);
        assert!((a.temperature_k - b.temperature_k).abs() < 1e-9);
    }

    #[test]
    fn planets_have_different_atmospheres() {
        let earth = atmosphere_for("Earth");
        let moon = atmosphere_for("Moon");
        let earth_props = earth.properties(0.0);
        let moon_props = moon.properties(0.0);
        assert!(earth_props.density_kg_m3 > 0.0);
        assert_eq!(moon_props.density_kg_m3, 0.0);
        assert_eq!(moon_props.pressure_pa, 0.0);
    }

    #[test]
    fn vacuum_is_empty_everywhere() {
        let props = VacuumAtmosphere.properties(10_000.0);
        assert_eq!(props.temperature_k, 0.0);
        assert_eq!(props.pressure_pa, 0.0);
        assert_eq!(props.density_kg_m3, 0.0);
        assert_eq!(props.speed_of_sound_mps, 0.0);
    }
}
