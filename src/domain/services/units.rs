use crate::domain::value_objects::solar_system_params::SolarSystemParameters;

pub fn au_to_simulation_units(au: f32, solar_params: &SolarSystemParameters) -> f32 {
    au * solar_params.scale_factor
}
