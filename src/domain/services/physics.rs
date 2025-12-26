use bevy::math::Vec3;
use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::SimulationParameters;

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

/// Calculate the position of a planet in its orbit at a given time
/// Uses simplified circular orbit approximation (Kepler's first law)
pub fn calculate_planet_position(planet: &Planet, time_days: f32, solar_params: &SolarSystemParameters) -> Vec3 {
    if planet.name == "Sun" {
        // Sun is at the origin
        return Vec3::ZERO;
    }

    // Calculate angle based on orbital period
    let angle = 2.0 * std::f32::consts::PI * time_days / planet.orbital_period_days;

    // Calculate distance in simulation units
    let distance = solar_params.au_to_units(planet.orbital_distance_au);

    // Position in orbital plane (XY plane)
    Vec3::new(distance * angle.cos(), 0.0, distance * angle.sin())
}

/// Calculate the rotation angle of a planet at a given time
pub fn calculate_planet_rotation(planet: &Planet, time_days: f32) -> f32 {
    // Convert days to hours and calculate rotation
    let time_hours = time_days * 24.0;
    2.0 * std::f32::consts::PI * time_hours / planet.rotation_period_hours
}

/// Calculate the visual radius for a planet based on its actual size and scaling
pub fn calculate_visual_radius(planet: &Planet, solar_params: &SolarSystemParameters) -> f32 {
    // For planets, use a combination of actual size and minimum visibility
    // Jupiter is our reference at ~143,000 km radius
    let jupiter_radius_km = 71492.0; // Jupiter's radius in km

    // Calculate relative size, but ensure minimum visibility
    let relative_size = (planet.radius_km / jupiter_radius_km).max(0.02); // Min 2% of Jupiter's size

    // Apply logarithmic scaling for better visibility of small planets
    let log_scaled = (relative_size * 10.0).ln().max(0.1);

    // Convert to simulation units and apply final scaling
    log_scaled * solar_params.planet_scale * 0.5
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