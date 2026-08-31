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
//! The Earth model implements the 1976 U.S. Standard Atmosphere layer table
//! through 84.852 km geopotential altitude. Geometric flight altitude is
//! converted to geopotential altitude before sampling, as required by the
//! standard. Above that range, the final state continues isothermally so the
//! simulation has no force discontinuity while explicitly remaining outside
//! the model's validated range.
//!
//! Units are SI (kelvin, pascals, kg/m³, m/s). The model is an approximation;
//! a real ISA implementation or measured data can replace the formulas behind
//! the same trait.

use crate::domain::services::aerodynamics::{dynamic_pressure_q, mach_number};
use crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2;
use bevy::math::DVec3;
use std::fmt::Debug;
use std::sync::Arc;

/// Standard atmosphere sea-level density, kg/m³.
pub const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
/// Standard atmosphere sea-level temperature, kelvin.
pub const SEA_LEVEL_TEMPERATURE_K: f64 = 288.15;
/// Standard atmosphere sea-level pressure, pascals.
pub const SEA_LEVEL_PRESSURE_PA: f64 = 101_325.0;
/// ISA-1976 specific gas constant of dry air, J/(kg·K).
pub const SPECIFIC_GAS_CONSTANT_AIR: f64 = 287.05287;
/// Ratio of specific heats of air.
pub const HEAT_CAPACITY_RATIO_AIR: f64 = 1.4;
/// Earth radius used by the U.S. Standard Atmosphere geopotential conversion,
/// meters. This is a model constant, not the planetary shape authority.
const STANDARD_ATMOSPHERE_EARTH_RADIUS_M: f64 = 6_356_766.0;

#[derive(Clone, Copy)]
struct StandardAtmosphereLayer {
    base_geopotential_altitude_m: f64,
    base_temperature_k: f64,
    base_pressure_pa: f64,
    lapse_rate_k_per_m: f64,
}

/// 1976 U.S. Standard Atmosphere base states. The final isothermal layer is a
/// continuous extrapolation beyond the 84.852 km published layer boundary.
const EARTH_STANDARD_ATMOSPHERE_LAYERS: [StandardAtmosphereLayer; 8] = [
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 0.0,
        base_temperature_k: 288.15,
        base_pressure_pa: 101_325.0,
        lapse_rate_k_per_m: -0.0065,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 11_000.0,
        base_temperature_k: 216.65,
        base_pressure_pa: 22_632.06,
        lapse_rate_k_per_m: 0.0,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 20_000.0,
        base_temperature_k: 216.65,
        base_pressure_pa: 5_474.889,
        lapse_rate_k_per_m: 0.001,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 32_000.0,
        base_temperature_k: 228.65,
        base_pressure_pa: 868.0187,
        lapse_rate_k_per_m: 0.0028,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 47_000.0,
        base_temperature_k: 270.65,
        base_pressure_pa: 110.9063,
        lapse_rate_k_per_m: 0.0,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 51_000.0,
        base_temperature_k: 270.65,
        base_pressure_pa: 66.93887,
        lapse_rate_k_per_m: -0.0028,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 71_000.0,
        base_temperature_k: 214.65,
        base_pressure_pa: 3.956420,
        lapse_rate_k_per_m: -0.002,
    },
    StandardAtmosphereLayer {
        base_geopotential_altitude_m: 84_852.0,
        base_temperature_k: 186.946,
        base_pressure_pa: 0.3734,
        lapse_rate_k_per_m: 0.0,
    },
];

fn geopotential_altitude_m(geometric_altitude_m: f64) -> f64 {
    let geometric_altitude_m = geometric_altitude_m.max(0.0);
    STANDARD_ATMOSPHERE_EARTH_RADIUS_M * geometric_altitude_m
        / (STANDARD_ATMOSPHERE_EARTH_RADIUS_M + geometric_altitude_m)
}

#[cfg(test)]
fn geometric_altitude_from_geopotential_m(geopotential_altitude_m: f64) -> f64 {
    STANDARD_ATMOSPHERE_EARTH_RADIUS_M * geopotential_altitude_m
        / (STANDARD_ATMOSPHERE_EARTH_RADIUS_M - geopotential_altitude_m)
}

/// Atmospheric state at an altitude, all SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AtmosphereProperties {
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_mps: f64,
}

/// One vehicle's atmosphere sample and air-relative motion for a fixed tick.
///
/// The inertial dynamics state remains authoritative. This value captures the
/// one atmosphere sample that every flight consumer must share for that tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightConditions {
    pub altitude_m: f64,
    pub temperature_k: f64,
    pub ambient_pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_mps: f64,
    pub atmosphere_relative_velocity_mps: DVec3,
    pub airspeed_mps: f64,
    pub mach_number: f64,
    pub dynamic_pressure_pa: f64,
}

impl FlightConditions {
    pub fn from_atmosphere(
        altitude_m: f64,
        atmosphere: AtmosphereProperties,
        atmosphere_relative_velocity_mps: DVec3,
    ) -> Self {
        let airspeed_mps = atmosphere_relative_velocity_mps.length();
        Self {
            altitude_m,
            temperature_k: atmosphere.temperature_k,
            ambient_pressure_pa: atmosphere.pressure_pa,
            density_kg_m3: atmosphere.density_kg_m3,
            speed_of_sound_mps: atmosphere.speed_of_sound_mps,
            atmosphere_relative_velocity_mps,
            airspeed_mps,
            mach_number: mach_number(airspeed_mps, atmosphere.speed_of_sound_mps),
            dynamic_pressure_pa: dynamic_pressure_q(atmosphere.density_kg_m3, airspeed_mps),
        }
    }
}

impl Default for FlightConditions {
    fn default() -> Self {
        Self::from_atmosphere(0.0, VacuumAtmosphere.properties(0.0), DVec3::ZERO)
    }
}

/// A planet's atmosphere model: returns the state at a given geometric
/// altitude above mean radius.
pub trait AtmosphereSource: Send + Sync + Debug {
    fn properties(&self, altitude_m: f64) -> AtmosphereProperties;
}

/// 1976 U.S. Standard Atmosphere Earth model (see module docs).
#[derive(Debug, Default)]
pub struct EarthAtmosphere;

impl AtmosphereSource for EarthAtmosphere {
    fn properties(&self, altitude_m: f64) -> AtmosphereProperties {
        let h = geopotential_altitude_m(altitude_m);
        let layer = EARTH_STANDARD_ATMOSPHERE_LAYERS
            .iter()
            .rev()
            .find(|layer| h >= layer.base_geopotential_altitude_m)
            .unwrap_or(&EARTH_STANDARD_ATMOSPHERE_LAYERS[0]);
        let height_above_base_m = h - layer.base_geopotential_altitude_m;
        let (temperature_k, pressure_pa) = if layer.lapse_rate_k_per_m.abs() <= f64::EPSILON {
            let t = layer.base_temperature_k;
            let p = layer.base_pressure_pa
                * (-(STANDARD_GRAVITY_MPS2 * height_above_base_m)
                    / (SPECIFIC_GAS_CONSTANT_AIR * t))
                    .exp();
            (t, p)
        } else {
            let t = layer.base_temperature_k + layer.lapse_rate_k_per_m * height_above_base_m;
            let p = layer.base_pressure_pa
                * (t / layer.base_temperature_k).powf(
                    -STANDARD_GRAVITY_MPS2 / (SPECIFIC_GAS_CONSTANT_AIR * layer.lapse_rate_k_per_m),
                );
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
        let props = EarthAtmosphere.properties(geometric_altitude_from_geopotential_m(11_000.0));
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
    fn earth_standard_atmosphere_matches_published_layer_checkpoints() {
        let earth = EarthAtmosphere;
        for (geopotential_altitude_m, temperature_k, pressure_pa) in [
            (20_000.0, 216.65, 5_474.889),
            (47_000.0, 270.65, 110.9063),
            (71_000.0, 214.65, 3.956420),
            (84_852.0, 186.946, 0.3734),
        ] {
            let properties = earth.properties(geometric_altitude_from_geopotential_m(
                geopotential_altitude_m,
            ));
            assert!((properties.temperature_k - temperature_k).abs() < 0.01);
            assert!((properties.pressure_pa - pressure_pa).abs() < pressure_pa * 0.01);
        }
    }

    #[test]
    fn earth_standard_atmosphere_is_continuous_at_layer_boundaries() {
        let earth = EarthAtmosphere;
        for layer in EARTH_STANDARD_ATMOSPHERE_LAYERS.iter().skip(1) {
            let boundary_m =
                geometric_altitude_from_geopotential_m(layer.base_geopotential_altitude_m);
            let below = earth.properties(boundary_m - 0.01);
            let above = earth.properties(boundary_m + 0.01);
            assert!(
                (above.temperature_k - below.temperature_k).abs() < 0.01,
                "temperature discontinuity at {} m",
                layer.base_geopotential_altitude_m
            );
            assert!(
                // The published base pressures are rounded; retain continuity
                // within that tabulation precision rather than asserting a
                // false bit-level identity between adjacent equations.
                (above.pressure_pa - below.pressure_pa).abs() < layer.base_pressure_pa * 1e-4,
                "pressure discontinuity at {} m",
                layer.base_geopotential_altitude_m
            );
        }
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

    #[test]
    fn flight_conditions_share_one_atmosphere_sample() {
        let velocity = DVec3::new(100.0, 0.0, 0.0);
        let conditions = FlightConditions::from_atmosphere(
            1_000.0,
            AtmosphereProperties {
                temperature_k: 280.0,
                pressure_pa: 90_000.0,
                density_kg_m3: 1.0,
                speed_of_sound_mps: 350.0,
            },
            velocity,
        );
        assert_eq!(conditions.ambient_pressure_pa, 90_000.0);
        assert_eq!(conditions.airspeed_mps, 100.0);
        assert!((conditions.mach_number - 100.0 / 350.0).abs() < 1e-12);
        assert_eq!(conditions.dynamic_pressure_pa, 5_000.0);
    }
}
