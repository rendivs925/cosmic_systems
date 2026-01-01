use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use std::env;

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

use application::startup::*;
use domain::value_objects::simulation_params::SimulationParameters;
use infrastructure::bevy_adapters::components::{
    HoveredPlanet, NotificationQueue, ScreenshotState, SelectedPlanet,
};
use infrastructure::bevy_adapters::systems::*;

fn main() {
    let args: Vec<String> = env::args().collect();
    let is_gyro_mode = args.contains(&"gyro".to_string());

    let title = if is_gyro_mode {
        "Cosmic Systems - Gyro Propulsion"
    } else {
        "Cosmic Systems Simulator"
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
    app.add_plugins(EguiPlugin);

    if is_gyro_mode {
        app.insert_resource(SimulationParameters::new());
        app.add_systems(Startup, setup_gyro);
        app.add_systems(Update, update_gyroscopes);
        app.add_systems(Update, update_thrust);
        app.add_systems(Update, handle_input);
    } else {
        app.insert_resource(SelectedPlanet {
            entity: None,
            name: None,
        });
        app.insert_resource(HoveredPlanet {
            name: None,
            info: None,
        });
        app.insert_resource(NotificationQueue {
            notifications: Vec::new(),
            hide_for_screenshot: false,
        });
        app.insert_resource(ScreenshotState { pending: false });
        app.add_systems(Startup, setup_space);
        app.add_systems(Update, update_planet_positions);
        app.add_systems(Update, update_planet_rotations);
        app.add_systems(Update, update_moon_orbit_positions);
        app.add_systems(Update, update_orbit_visuals);
        app.add_systems(Update, update_orbit_visibility);
        app.add_systems(Update, update_planet_reflections);
        app.add_systems(Update, handle_solar_system_input);
        app.add_systems(Update, handle_planet_selection);
        app.add_systems(Update, handle_mouse_planet_selection);
        app.add_systems(Update, display_navigation_bar);
        app.add_systems(Update, update_planet_selection_visuals);
        app.add_systems(Update, display_hover_info);
        app.add_systems(Update, display_notifications);
        app.add_systems(Update, take_pending_screenshot);
        app.add_systems(Update, update_camera_controller);
        app.add_systems(Update, apply_camera_transform);
        app.add_systems(Update, auto_inspect_selected_planet);
    }

    app.run();
}
