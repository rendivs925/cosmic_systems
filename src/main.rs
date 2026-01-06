use bevy::prelude::*;
use infrastructure::bevy_adapters::components::PerformanceStats;
use infrastructure::bevy_adapters::systems::log_performance_stats;

// Run condition for visual updates (every 3 frames)
fn every_n_frames(n: usize) -> impl FnMut(Local<usize>) -> bool {
    move |mut frame_count: Local<usize>| {
        *frame_count += 1;
        if *frame_count >= n {
            *frame_count = 0;
            true
        } else {
            false
        }
    }
}
use std::env;

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod presentation;

use application::startup::*;
use domain::value_objects::simulation_params::SimulationParameters;
use infrastructure::bevy_adapters::components::{
    CameraInputState, HoveredPlanet, NotificationQueue, ScreenshotState, SelectedPlanet, ZenMode,
    UiPointerState,
};
use infrastructure::bevy_adapters::systems::*;
use presentation::ui::*;

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
            resolution: (1280, 720).into(),
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
        app.insert_resource(PerformanceStats::default());
        app.insert_resource(UiPointerState::default());
        app.insert_resource(CameraInputState::default());
        app.insert_resource(ZenMode::default());
        app.insert_resource(infrastructure::bevy_adapters::systems::QualityAdaptationResource::default());
        app.add_systems(Startup, setup_space);
        app.add_systems(Startup, setup_ui);

        // Physics systems run on FixedUpdate for consistent simulation
        app.add_systems(FixedUpdate, update_planet_positions);
        app.add_systems(FixedUpdate, update_planet_rotations);
        app.add_systems(FixedUpdate, update_moon_orbit_positions);
        app.add_systems(Update, update_orbit_visuals);
        app.add_systems(Update, update_orbit_visibility);
        app.add_systems(Update, update_planet_reflections);
        app.add_systems(Update, apply_pending_material_textures);
        app.add_systems(Update, handle_solar_system_input);
        app.add_systems(Update, handle_planet_selection);
        app.add_systems(Update, handle_mouse_planet_selection);
        app.add_systems(Update, handle_nav_interactions);
         app.add_systems(Update, update_navbar);
        app.add_systems(
            Update,
            update_planet_selection_visuals.run_if(every_n_frames(2)),
        );
        app.add_systems(Update, update_performance_stats);
        app.add_systems(Update, log_performance_stats);
        app.add_systems(Update, adaptive_quality_system);
        #[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
        app.add_systems(Update, crate::infrastructure::bevy_adapters::systems::init_vulkan_solver);
        app.add_systems(Update, update_info_card);
        app.add_systems(Update, update_notifications_ui);
        app.add_systems(Update, update_ui_hover_state.before(update_camera_controller));
         app.add_systems(Update, take_pending_screenshot);
         app.add_systems(Update, toggle_video_recording);
         app.add_systems(Update, handle_video_recording);
         app.add_systems(Update, update_camera_controller);
         app.add_systems(Update, apply_camera_transform);
         app.add_systems(Update, auto_inspect_selected_planet);
         // Starfield update disabled for now
         // app.add_systems(Update, update_starfield_position);
    }

    app.run();
}
