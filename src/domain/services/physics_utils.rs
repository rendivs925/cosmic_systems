use crate::domain::entities::planet::Planet;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

/// Calculate the rotation angle for a planet at a given time
pub fn calculate_planet_rotation(planet: &Planet, time_days: f32) -> f32 {
    // Convert days to hours and calculate rotation
    let time_hours = time_days * 24.0;
    2.0 * std::f32::consts::PI * time_hours / planet.rotation_period_hours
}

/// Calculate the visual radius for a planet/moon based on actual astronomical sizes with correct relative proportions
pub fn calculate_visual_radius(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    // Astronomical radii in kilometers (actual values)
    let astronomical_radii = [
        ("Sun", 696340.0),
        ("Jupiter", 69911.0),
        ("Saturn", 58232.0),
        ("Uranus", 25362.0),
        ("Neptune", 24622.0),
        ("Earth", 6371.0),
        ("Venus", 6051.8),
        ("Mars", 3389.5),
        ("Mercury", 2439.7),
        // Major moons
        ("Moon", 1737.4),
        ("Ganymede", 2634.1),
        ("Callisto", 2410.3),
        ("Io", 1821.6),
        ("Europa", 1560.8),
        ("Titan", 2574.7),
        ("Rhea", 763.8),
        ("Iapetus", 734.5),
        ("Dione", 561.4),
        ("Tethys", 531.1),
        ("Enceladus", 252.1),
        ("Mimas", 198.2),
        ("Hyperion", 135.0),
        ("Titania", 788.4),
        ("Oberon", 761.4),
        ("Umbriel", 584.7),
        ("Ariel", 578.9),
        ("Miranda", 235.8),
        ("Triton", 1353.4),
        ("Phobos", 11.1),
        ("Deimos", 6.2),
    ];

    // Find the actual astronomical radius for this planet
    let astronomical_radius_km = astronomical_radii
        .iter()
        .find(|(name, _)| *name == planet.name)
        .map(|(_, radius)| *radius)
        .unwrap_or(planet.radius_km); // Fallback to stored value

    // Calculate size relative to Mercury (smallest planet) to establish baseline
    let mercury_radius_km = 2439.7;
    let relative_to_mercury = astronomical_radius_km / mercury_radius_km;

    // Use TRUE mathematical proportions - no logarithmic scaling
    // This preserves real-world size relationships:
    // - Jupiter will be ~28.6x larger than Earth
    // - Sun will be ~109x larger than Earth
    // - All sizes are mathematically accurate to real-world data

    // Apply final scaling for visibility in the simulation
    relative_to_mercury * solar_params.planet_scale
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

/// Convert astronomical units to simulation distance units
pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}