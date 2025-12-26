use bevy::prelude::*;
use std::env;

pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod presentation;

use application::startup::*;
use infrastructure::bevy_adapters::systems::*;
use domain::value_objects::simulation_params::SimulationParameters;




fn main() {
    let args: Vec<String> = env::args().collect();
    let is_gyro_mode = args.contains(&"gyro".to_string());

    let title = if is_gyro_mode {
        "Cosmic Frontier Simulator - Gyro Propulsion"
    } else {
        "Cosmic Frontier Simulator"
    };

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: title.to_string(),
            resolution: (1280.0, 720.0).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins.set(window_plugin);

    let mut app = App::new();
    app.add_plugins(plugins);

    if is_gyro_mode {
        app.insert_resource(SimulationParameters::new());
        app.add_systems(Startup, setup_gyro);
        app.add_systems(Update, update_gyroscopes);
        app.add_systems(Update, update_thrust);
        app.add_systems(Update, handle_input);
    } else {
        app.add_systems(Startup, setup_space);
        app.add_systems(Update, update_planet_positions);
        app.add_systems(Update, update_planet_rotations);
        app.add_systems(Update, handle_solar_system_input);
    }

    app.run();
}




