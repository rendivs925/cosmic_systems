use bevy::log::LogPlugin;
use bevy::prelude::*;

use std::env;

pub mod application;
pub mod components;
pub mod domain;
pub mod infrastructure;
pub mod presentation;
pub mod systems;

use application::modes::{parse_launch_options, Mode};
use application::rocket_config::{RocketCatalog, VehicleSelection};
use application::solar_system_startup::SolarCameraEnabled;
use infrastructure::plugins::{
    CraftModePlugin, GyroModePlugin, RocketModePlugin, SharedSimulationPlugin,
    SolarSystemModePlugin,
};

/// Validate the requested vehicle BEFORE any window/renderer exists. The
/// previous behavior validated inside `app.run()`'s Startup schedule, so an
/// unknown key booted the full GPU stack and then panicked mid-teardown —
/// surfacing as alternating SIGABRT/SIGSEGV exit codes instead of a clean
/// error (Phase 17).
fn validate_vehicle_selection(selection: &Option<String>) {
    let Some(requested) = selection else {
        return; // None = catalog default; always valid.
    };
    if let Ok(catalog) = RocketCatalog::from_dir() {
        if catalog.get(requested).is_none() {
            let mut available: Vec<&String> = catalog.keys().collect();
            available.sort();
            eprintln!(
                "Unknown vehicle '{requested}'. Available vehicles: {}",
                available
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            std::process::exit(2);
        }
        // Catalog IO/parse failures are still handled by the plugin's
        // fail-fast path (AGENTS.md section 65); do not duplicate them here.
    }
}

fn main() {
    let options = parse_launch_options(env::args());

    if matches!(options.mode, Mode::Rocket) {
        validate_vehicle_selection(&options.vehicle);
    }

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: options.mode.title().to_string(),
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

    match options.mode {
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
            app.insert_resource(VehicleSelection(options.vehicle));
            // Rocket Mode owns its own camera/HUD/input context. It does NOT
            // add SolarSystemModePlugin, whose solar camera controller (free
            // WASD flight), solar navigation input, and Explore/Orbits UI would
            // otherwise fight the rocket camera and leak into the launch view.
            app.add_plugins((SharedSimulationPlugin, RocketModePlugin));
        }
        Mode::Gyro => {
            app.add_plugins(GyroModePlugin);
        }
    }

    app.run();
}
