use bevy::app::{TaskPoolOptions, TaskPoolPlugin, TaskPoolThreadAssignmentPolicy};
use bevy::asset::AssetPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;

use std::env;

use cosmic_systems_wasm::application::modes::{parse_launch_options, Mode};
use cosmic_systems_wasm::application::plugins::{
    CraftModePlugin, RocketModePlugin, SharedSimulationPlugin, SolarSystemModePlugin,
};
use cosmic_systems_wasm::application::rocket_config::{RocketCatalog, VehicleSelection};
use cosmic_systems_wasm::application::solar_system_startup::SolarCameraEnabled;

/// Reject an unknown requested vehicle before creating the window or renderer.
fn validate_vehicle_selection(selection: &VehicleSelection) {
    let Some(requested) = selection.requested() else {
        return;
    };
    let Ok(catalog) = RocketCatalog::from_dir() else {
        // Plugin startup reports catalog load failures.
        return;
    };
    if catalog.resolve(selection).is_some() {
        return;
    }

    let available = catalog.keys().collect::<Vec<_>>().join(", ");
    eprintln!(
        "Unknown vehicle '{}'. Available vehicles: {}",
        requested.as_str(),
        available
    );
    std::process::exit(2);
}

fn rocket_task_pool_options() -> TaskPoolOptions {
    // Reserve CPU capacity for rendering, IO, and fixed simulation.
    TaskPoolOptions {
        async_compute: TaskPoolThreadAssignmentPolicy {
            min_threads: 1,
            max_threads: 6,
            percent: 0.25,
            on_thread_spawn: None,
            on_thread_destroy: None,
        },
        ..Default::default()
    }
}

fn main() {
    let options = parse_launch_options(env::args().skip(1))
        .unwrap_or_else(|error| panic!("Invalid launch options: {error}"));
    let mode = options.mode;
    let vehicle_selection = if matches!(mode, Mode::Rocket) {
        Some(VehicleSelection::from(options.vehicle))
    } else {
        None
    };

    if let Some(selection) = &vehicle_selection {
        validate_vehicle_selection(selection);
    }

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: mode.title().to_string(),
            resolution: (1280, 720).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins
        .set(window_plugin)
        // Native runs load the repository's checked-in assets.
        .set(AssetPlugin {
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").to_string(),
            ..default()
        })
        .set(LogPlugin {
            filter: "info,bevy_render::view::window=error,wgpu_hal::vulkan::instance=error"
                .to_string(),
            ..default()
        });
    let plugins = if matches!(mode, Mode::Rocket) {
        plugins.set(TaskPoolPlugin {
            task_pool_options: rocket_task_pool_options(),
        })
    } else {
        plugins
    };

    let mut app = App::new();
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
            app.insert_resource(vehicle_selection.expect("rocket mode has a vehicle selection"));
            // Rocket mode owns its camera, HUD, and input context.
            app.add_plugins((SharedSimulationPlugin, RocketModePlugin));
        }
    }

    app.run();
}

#[cfg(test)]
mod tests {
    use super::rocket_task_pool_options;

    #[test]
    fn rocket_mode_reserves_cpu_capacity_for_presentation_during_terrain_bakes() {
        let options = rocket_task_pool_options();
        assert_eq!(options.async_compute.max_threads, 6);
        assert_eq!(options.async_compute.percent, 0.25);
    }
}
