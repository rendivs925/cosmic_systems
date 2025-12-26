use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::SimulationParameters;
use bevy::math::Vec3;

pub fn calculate_precession_angle(precession_rate: f32, delta_time: f32) -> f32 {
    precession_rate * delta_time
}

pub fn calculate_thrust_magnitude(gyro: &Gyroscope, params: &SimulationParameters) -> f32 {
    params.thrust_scale * gyro.asymmetry * (gyro.spin_rate.powi(2)) * gyro.precession_rate
}

pub fn calculate_total_thrust(gyros: &[&Gyroscope], params: &SimulationParameters) -> Vec3 {
    let mut total_thrust = Vec3::ZERO;
    for gyro in gyros {
        let thrust_mag = calculate_thrust_magnitude(gyro, params);
        total_thrust += thrust_mag * gyro.angular_momentum.normalize_or_zero();
    }
    // Average thrust if multiple gyros
    if !gyros.is_empty() {
        total_thrust /= gyros.len() as f32;
    }
    total_thrust
}

pub fn calculate_arrow_scale(thrust: Vec3) -> f32 {
    thrust.length().clamp(0.1, 10.0)
}

// Orbital mechanics functions for solar system simulation

/// Calculate the position of a planet/moon in its orbit at a given time
/// Uses simplified circular orbit approximation (Kepler's first law)
pub fn calculate_planet_position(
    planet: &Planet,
    time_days: f32,
    solar_params: &SolarSystemParameters,
    parent_position: Vec3,
) -> Vec3 {
    if planet.name == "Sun" {
        // Sun is at the origin
        return Vec3::ZERO;
    }

    // Calculate angle based on orbital period
    let angle = 2.0 * std::f32::consts::PI * time_days / planet.orbital_period_days;

    // For moons, orbital_distance_au is relative to parent, not Sun
    // For planets, it's already in AU from Sun
    let distance = if planet.parent_entity.is_some() {
        // Moon orbiting a planet - convert astronomical distance to simulation units
        // orbital_distance_au represents actual AU distance from parent planet
        // Scale massively for clear separation while maintaining relative accuracy
        planet.orbital_distance_au * solar_params.scale_factor * 500.0
    } else {
        // Planet orbiting Sun
        solar_params.au_to_units(planet.orbital_distance_au)
    };

    // Position relative to parent
    let relative_pos = Vec3::new(distance * angle.cos(), 0.0, distance * angle.sin());

    // Add parent position to get absolute position
    parent_position + relative_pos
}

/// Calculate the rotation angle of a planet at a given time
pub fn calculate_planet_rotation(planet: &Planet, time_days: f32) -> f32 {
    // Convert days to hours and calculate rotation
    let time_hours = time_days * 24.0;
    2.0 * std::f32::consts::PI * time_hours / planet.rotation_period_hours
}

/// Calculate the visual radius for a planet/moon based on actual astronomical sizes with correct relative proportions
pub fn calculate_visual_radius(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    // Astronomical radii in kilometers (actual values)
    let astronomical_radii = [
        ("Sun", 696342.0),
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

    // Apply logarithmic scaling to make large objects manageable while preserving proportions
    // This ensures Mercury is visible but Sun doesn't overwhelm the scene
    let scaled_relative = if astronomical_radius_km > mercury_radius_km * 10.0 {
        // For large objects (planets and Sun), use logarithmic scaling
        (relative_to_mercury.ln() * 2.0).max(1.0)
    } else {
        // For smaller objects (moons), maintain more linear scaling
        relative_to_mercury.max(0.1)
    };

    // Apply final scaling for visibility in the simulation
    scaled_relative * solar_params.planet_scale
}

/// Calculate the visual radius for the Sun with appropriate scaling
pub fn calculate_sun_visual_radius(solar_params: &SolarSystemParameters) -> f32 {
    // Sun radius is ~696,000 km, Jupiter is ~71,000 km
    // Sun is about 9.7 times larger than Jupiter
    // For visualization, make it reasonably larger than Jupiter but not overwhelming
    let jupiter_visual_radius = calculate_visual_radius(&Planet::create_jupiter(), solar_params);
    jupiter_visual_radius * 2.5 // Sun appears 2.5x larger than Jupiter visually
}

/// Get the astronomical unit in simulation units
pub const AU_IN_KM: f32 = 149597870.7; // 1 AU in kilometers

/// Convert astronomical units to simulation distance units
pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}
