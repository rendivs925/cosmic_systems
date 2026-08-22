use bevy::log::LogPlugin;
use bevy::prelude::*;

use std::env;

pub mod application;
pub mod components;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
pub mod systems;

use application::modes::Mode;
use application::solar_system_startup::SolarCameraEnabled;
use infrastructure::plugins::{
    CraftModePlugin, GyroModePlugin, RocketModePlugin, SharedSimulationPlugin,
    SolarSystemModePlugin,
};

fn main() {
    let mode = Mode::from_args(env::args());

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: mode.title().to_string(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins.set(window_plugin).set(LogPlugin {
        filter: "info,bevy_render::view::window=error,wgpu_hal::vulkan::instance=error".to_string(),
        ..default()
    });

    let mut app = App::new();
    // GizmoPlugin ships inside DefaultPlugins (bevy_gizmos feature); no
    // explicit registration needed here.
    app.add_plugins(plugins);

    match mode {
        Mode::Solar => {
            app.add_plugins((SharedSimulationPlugin, SolarSystemModePlugin));
        }
        Mode::Craft => {
            app.insert_resource(SolarCameraEnabled(false));
            app.add_plugins((
                SharedSimulationPlugin,
                SolarSystemModePlugin,
                CraftModePlugin,
            ));
        }
        Mode::Rocket => {
            app.add_plugins((
                SharedSimulationPlugin,
                SolarSystemModePlugin,
                RocketModePlugin,
            ));
        }
        Mode::Gyro => {
            app.add_plugins(GyroModePlugin);
        }
    }

    app.run();
}
