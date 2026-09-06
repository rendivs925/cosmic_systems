//! ECS access for the shared solar-system configuration.

use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::Resource;

impl Resource for SolarSystemParameters {}
