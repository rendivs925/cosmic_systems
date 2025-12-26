use bevy::prelude::*;

#[derive(Resource, Clone, Debug)]
pub struct SimulationParameters {
    pub rpm: f32,
    pub precession_hz: f32,
    pub asymmetry: f32,
    pub thrust_scale: f32,
}

impl Default for SimulationParameters {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationParameters {
    pub fn new() -> Self {
        Self {
            rpm: 30000.0,
            precession_hz: 100.0,
            asymmetry: 0.5,
            thrust_scale: 0.001,
        }
    }
}