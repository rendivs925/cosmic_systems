//! Rendering-boundary conversions for the authoritative domain reference frames.

use crate::domain::math::DVec3;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use bevy::math::Vec3;

/// Convert a solar-map display position to planet-centered inertial meters.
pub fn solar_to_planet_inertial(
    position_solar_units: Vec3,
    planet_solar_units: Vec3,
    scale: &PhysicalScale,
) -> DVec3 {
    let delta = position_solar_units.as_dvec3() - planet_solar_units.as_dvec3();
    DVec3::new(
        scale.solar_units_to_meters(delta.x),
        scale.solar_units_to_meters(delta.y),
        scale.solar_units_to_meters(delta.z),
    )
}

/// Convert planet-centered inertial meters to a solar-map display position.
pub fn planet_inertial_to_solar(
    position_pci_m: DVec3,
    planet_solar_units: Vec3,
    scale: &PhysicalScale,
) -> Vec3 {
    let units = DVec3::new(
        scale.solar_meters_to_units(position_pci_m.x),
        scale.solar_meters_to_units(position_pci_m.y),
        scale.solar_meters_to_units(position_pci_m.z),
    );
    (planet_solar_units.as_dvec3() + units).as_vec3()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solar_map_round_trip_has_only_expected_f32_loss() {
        let scale = PhysicalScale::default();
        let position_m = DVec3::new(6_371_000.0, -200.0, 300.0);
        let planet_units = Vec3::new(75_000.0, 0.0, 0.0);
        let recovered = solar_to_planet_inertial(
            planet_inertial_to_solar(position_m, planet_units, &scale),
            planet_units,
            &scale,
        );
        assert!((recovered - position_m).length() < 20_000.0);
    }
}
