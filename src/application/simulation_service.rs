use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::services::physics;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use bevy::math::Vec3;

pub struct SimulationService;

impl SimulationService {
    pub fn update_gyroscope(gyro: &mut Gyroscope, params: &SimulationParameters, spin_axis: Vec3) {
        gyro.update_params(params.rpm, params.precession_hz, params.asymmetry);
        gyro.update_angular_momentum(spin_axis);
    }

    pub fn calculate_thrust(gyros: &[&Gyroscope], params: &SimulationParameters) -> Vec3 {
        physics::calculate_total_thrust(gyros, params)
    }

    pub fn get_precession_angle(gyro: &Gyroscope, delta_time: f32) -> f32 {
        physics::calculate_precession_angle(gyro.precession_rate, delta_time)
    }
}
