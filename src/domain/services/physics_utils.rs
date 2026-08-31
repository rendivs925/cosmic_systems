use crate::domain::entities::planet::Planet;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

/// Calculate the rotation angle for a planet at a given time
pub fn calculate_planet_rotation(planet: &Planet, time_days: f32) -> f32 {
    // Convert days to hours and calculate rotation
    let time_hours = time_days * 24.0;
    2.0 * std::f32::consts::PI * time_hours / planet.rotation_period_hours
}

/// High-precision counterpart for physical reference-frame consumers. Solar
/// presentation retains the f32 API above; flight epochs must not lose phase
/// precision when simulation time reaches long mission durations.
pub fn calculate_planet_rotation_f64(planet: &Planet, time_days: f64) -> f64 {
    std::f64::consts::TAU * time_days * 24.0 / planet.rotation_period_hours as f64
}

/// Calculate a body's solar-map radius in the same AU-derived units as its orbit.
pub fn calculate_visual_radius(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    planet.radius_km / AU_IN_KM * solar_params.scale_factor * solar_params.planet_scale
}

/// Calculate the visual radius for the Sun with appropriate scaling
pub fn calculate_sun_visual_radius(solar_params: &SolarSystemParameters) -> f32 {
    // Sun radius is ~696,340 km
    // Use the same TRUE mathematical proportions as planets
    // Sun will be ~285x larger than Mercury, ~109x larger than Earth
    // This preserves the real-world dominance of the Sun in the solar system
    let sun = PlanetFactory::create_by_name("Sun").unwrap();
    calculate_visual_radius(&sun, solar_params)
}

/// Get the astronomical unit in simulation units
pub const AU_IN_KM: f32 = 149597870.7; // 1 AU in kilometers

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::planet_factory::PlanetFactory;

    #[test]
    fn visual_radius_uses_the_orbital_distance_scale() {
        let solar = SolarSystemParameters::for_visualization();
        let earth = PlanetFactory::create_by_name("Earth").unwrap();

        let expected_units = earth.radius_km / AU_IN_KM * solar.scale_factor;
        assert!((calculate_visual_radius(&earth, &solar) - expected_units).abs() < 1e-6);
    }
}
