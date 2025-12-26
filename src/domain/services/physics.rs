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
    // Base radius in km, scaled to simulation units
    // Use logarithmic scaling to make planets visible
    let base_radius = planet.radius_km * 0.001; // Convert km to simulation units roughly

    // Apply additional scaling for visibility
    base_radius * solar_params.planet_scale
}

/// Calculate the visual radius for the Sun with appropriate scaling
pub fn calculate_sun_visual_radius(solar_params: &SolarSystemParameters) -> f32 {
    // Sun is huge, so we need significant scaling down
    let sun_radius_units = solar_params.sun_radius_km * 0.001; // Rough conversion
    // Scale down dramatically to be viewable
    sun_radius_units * solar_params.planet_scale * 0.1
}

/// Get the astronomical unit in simulation units
pub const AU_IN_KM: f32 = 149597870.7; // 1 AU in kilometers

/// Convert astronomical units to simulation distance units
pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}