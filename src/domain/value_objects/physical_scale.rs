use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

/// One astronomical unit in meters (IAU definition).
pub const AU_IN_METERS: f64 = 149_597_870_700.0;

/// Central definition of the mapping between real physical units (meters)
/// and the visualization world's display units.
///
/// Flight dynamics run in real meters (f64). Rendering stays in the solar
/// system's display units (f32). Every rocket and terrain subsystem must map
/// between the two through this resource rather than hardcoding its own scale
/// factor (AGENTS.md sections 15 and 39).
///
/// Two scales are defined:
///
/// - `flight_*`: the scale at which a vehicle is rendered near a local origin
///   (the rocket-body / local-tangent presentation boundary).
/// - `solar_*`: the scale at which the solar system itself is rendered,
///   derived from [`SolarSystemParameters`] (1 AU = `scale_factor` display
///   units).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PhysicalScale {
    /// Display units per real meter at flight/vehicle scale.
    pub flight_display_units_per_meter: f32,
    /// Real meters per display unit at flight/vehicle scale.
    pub flight_meters_per_display_unit: f32,
    /// Display units per real meter at the solar/orbital rendering scale.
    pub solar_display_units_per_meter: f32,
    /// Real meters per display unit at the solar/orbital rendering scale.
    pub solar_meters_per_display_unit: f32,
    /// Solar-system visualization scale factor (display units per AU).
    pub solar_scale_factor: f32,
}

impl Default for PhysicalScale {
    fn default() -> Self {
        Self::from_solar_parameters(&SolarSystemParameters::for_visualization())
    }
}

impl PhysicalScale {
    /// Build the scale definitions from the authoritative solar-system
    /// parameters. `flight_display_units_per_meter` defaults to 1.0 (one
    /// display unit per real meter at the vehicle presentation boundary).
    pub fn from_solar_parameters(solar: &SolarSystemParameters) -> Self {
        let flight_display_units_per_meter = 1.0;
        let solar_display_units_per_meter = (solar.scale_factor as f64 / AU_IN_METERS) as f32;
        Self {
            flight_display_units_per_meter,
            flight_meters_per_display_unit: 1.0 / flight_display_units_per_meter,
            solar_display_units_per_meter,
            solar_meters_per_display_unit: 1.0 / solar_display_units_per_meter,
            solar_scale_factor: solar.scale_factor,
        }
    }

    /// Convert a distance in meters to display units at flight scale.
    pub fn flight_meters_to_units(&self, meters: f64) -> f64 {
        meters * self.flight_display_units_per_meter as f64
    }

    /// Convert a distance in display units to meters at flight scale.
    pub fn flight_units_to_meters(&self, units: f64) -> f64 {
        units * self.flight_meters_per_display_unit as f64
    }

    /// Convert a distance in meters to display units at solar/orbital scale.
    pub fn solar_meters_to_units(&self, meters: f64) -> f64 {
        meters * self.solar_display_units_per_meter as f64
    }

    /// Convert a distance in display units to meters at solar/orbital scale.
    pub fn solar_units_to_meters(&self, units: f64) -> f64 {
        units * self.solar_meters_per_display_unit as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flight_scale_round_trips() {
        let scale = PhysicalScale::default();
        let meters = 6_371_000.0;
        let units = scale.flight_meters_to_units(meters);
        let back = scale.flight_units_to_meters(units);
        assert!((meters - back).abs() < 1e-6, "round trip off by {back}");
    }

    #[test]
    fn solar_scale_round_trips() {
        let scale = PhysicalScale::default();
        let meters = 1.0;
        let units = scale.solar_meters_to_units(meters);
        let back = scale.solar_units_to_meters(units);
        // f32 scale storage limits precision to ~1e-7 relative.
        assert!(
            (meters - back).abs() < meters * 1e-6,
            "round trip off by {back}"
        );
    }

    #[test]
    fn solar_scale_matches_one_au() {
        let scale = PhysicalScale::default();
        // 1 AU in meters must map to the solar `scale_factor` display units.
        let units = scale.solar_meters_to_units(AU_IN_METERS);
        assert!(
            (units - scale.solar_scale_factor as f64).abs()
                < scale.solar_scale_factor as f64 * 1e-6,
            "1 AU mapped to {units} display units"
        );
    }

    #[test]
    fn flight_scale_is_unit_default() {
        let scale = PhysicalScale::default();
        assert_eq!(scale.flight_display_units_per_meter, 1.0);
        assert_eq!(scale.flight_meters_per_display_unit, 1.0);
    }

    #[test]
    fn derived_from_visualization_parameters() {
        let solar = SolarSystemParameters::for_visualization();
        let scale = PhysicalScale::from_solar_parameters(&solar);
        assert_eq!(scale.solar_scale_factor, solar.scale_factor);
        // 75_000 units / 1.496e11 m
        let expected = 75_000.0 / 149_597_870_700.0;
        assert!(
            (scale.solar_display_units_per_meter - expected).abs() < expected * 1e-6,
            "got {}",
            scale.solar_display_units_per_meter
        );
    }
}
