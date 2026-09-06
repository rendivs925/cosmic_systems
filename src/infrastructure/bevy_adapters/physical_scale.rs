use crate::domain::units::AU_IN_METERS;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::Resource;
use glam::DVec3;

/// Central presentation mapping between real meters and display units.
///
/// Authoritative simulation remains in meters. This resource owns the lossy
/// display-unit boundary shared by solar-map and local-flight rendering.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct PhysicalScale {
    pub flight_display_units_per_meter: f32,
    pub flight_meters_per_display_unit: f32,
    pub solar_display_units_per_meter: f64,
    pub solar_meters_per_display_unit: f64,
    pub solar_scale_factor: f32,
}

impl Default for PhysicalScale {
    fn default() -> Self {
        Self::from_solar_parameters(&SolarSystemParameters::for_visualization())
    }
}

impl PhysicalScale {
    pub fn from_solar_parameters(solar: &SolarSystemParameters) -> Self {
        let flight_display_units_per_meter = 1.0;
        let solar_display_units_per_meter = solar.scale_factor as f64 / AU_IN_METERS;
        Self {
            flight_display_units_per_meter,
            flight_meters_per_display_unit: 1.0 / flight_display_units_per_meter,
            solar_display_units_per_meter,
            solar_meters_per_display_unit: 1.0 / solar_display_units_per_meter,
            solar_scale_factor: solar.scale_factor,
        }
    }

    pub fn flight_meters_to_units(&self, meters: f64) -> f64 {
        meters * self.flight_display_units_per_meter as f64
    }

    pub fn flight_units_to_meters(&self, units: f64) -> f64 {
        units * self.flight_meters_per_display_unit as f64
    }

    pub fn solar_meters_to_units(&self, meters: f64) -> f64 {
        meters * self.solar_display_units_per_meter
    }

    pub fn solar_meters_to_units_vec3(&self, position_m: DVec3) -> DVec3 {
        position_m * self.solar_display_units_per_meter
    }

    pub fn solar_units_to_meters(&self, units: f64) -> f64 {
        units * self.solar_meters_per_display_unit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_scale_round_trips() {
        let scale = PhysicalScale::default();
        let meters = AU_IN_METERS;
        assert!(
            (scale.solar_units_to_meters(scale.solar_meters_to_units(meters)) - meters).abs() < 1.0
        );
    }

    #[test]
    fn solar_vector_conversion_preserves_f64_precision() {
        let scale = PhysicalScale::default();
        assert_eq!(
            scale.solar_meters_to_units_vec3(DVec3::new(
                AU_IN_METERS,
                -2.0 * AU_IN_METERS,
                0.5 * AU_IN_METERS
            )),
            DVec3::new(75_000.0, -150_000.0, 37_500.0)
        );
    }
}
