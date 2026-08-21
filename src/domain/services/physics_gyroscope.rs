use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::value_objects::simulation_params::SimulationParameters;
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
