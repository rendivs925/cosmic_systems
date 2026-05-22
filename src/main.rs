use bevy::log::LogPlugin;
use bevy::prelude::*;
use infrastructure::bevy_adapters::components::PerformanceStats;
use infrastructure::bevy_adapters::education_systems::register_education_systems;
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

use application::craft_startup::spawn_craft_model;
use application::startup::*;
use domain::value_objects::simulation_params::SimulationParameters;
use infrastructure::bevy_adapters::components::{
    CameraInputState, HoveredPlanet, NotificationQueue, ScreenshotState, SelectedPlanet,
    UiPointerState, ZenMode,
};
use infrastructure::bevy_adapters::craft_components::{
    CraftCameraState, CraftControlState, CraftEffectsEnabled, CraftTravelTarget,
};
use infrastructure::bevy_adapters::craft_effects::update_craft_visuals;
use infrastructure::bevy_adapters::craft_systems::{
    handle_craft_input, update_craft_camera, update_craft_physics,
};
use infrastructure::bevy_adapters::craft_ui::update_craft_ui;
use infrastructure::bevy_adapters::systems::*;
use presentation::ui::*;

fn setup_gyro_mode(app: &mut App) {
    app.insert_resource(SimulationParameters::new());
    app.add_systems(Startup, setup_gyro);
    app.add_systems(Update, update_gyroscopes);
    app.add_systems(Update, update_thrust);
    app.add_systems(Update, handle_input);
}

fn setup_craft_systems(app: &mut App) {
    app.insert_resource(CraftControlState::default());
    app.insert_resource(CraftCameraState::default());
    app.insert_resource(CraftTravelTarget::default());
    app.insert_resource(CraftEffectsEnabled(false));
    app.add_systems(Startup, spawn_craft);
    app.add_systems(Startup, spawn_craft_ui);
    app.add_systems(Update, (
        update_craft_physics,
        handle_craft_input,
        update_craft_camera,
        spawn_craft_model,
        update_craft_visuals,
        update_craft_ui,
    ).chain());
}

fn setup_solar_system_mode(app: &mut App) {
    // Insert resources
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
    app.insert_resource(UiIdleState::default());
    app.insert_resource(
        infrastructure::bevy_adapters::systems::QualityAdaptationResource::default(),
    );

    // Startup systems
    app.add_systems(Startup, setup_space);
    app.add_systems(Startup, setup_ui);
    app.add_systems(Startup, spawn_orbital_planes);
    app.add_systems(Startup, spawn_eccentricity_markers);

    // Physics systems run on Update for display-synced updates (eliminates FixedUpdate jitter)
    app.add_systems(Update, (
        update_planet_positions,
        update_planet_rotations,
        update_moon_orbit_positions,
    ).chain());

    // Visual and interaction systems
    app.add_systems(Update, update_orbit_visuals);
    app.add_systems(Update, update_orbit_thickness);
    app.add_systems(Update, update_orbit_quality);
    app.add_systems(Update, update_orbit_visibility);
    app.add_systems(Update, spawn_position_trackers);
    app.add_systems(Update, update_position_trackers);
    app.add_systems(Update, update_planet_reflections);
    app.add_systems(Update, update_orbital_planes);
    app.add_systems(Update, update_eccentricity_markers);
    app.add_systems(Update, apply_pending_material_textures);

    // Input handling
    app.add_systems(Update, handle_solar_system_input);
    app.add_systems(Update, handle_planet_selection);
    app.add_systems(Update, handle_mouse_planet_selection);
    app.add_systems(Update, handle_nav_interactions);

    // UI systems
    app.add_systems(Update, update_ui_idle);
    app.add_systems(Update, update_navbar);
    app.add_systems(Update, update_info_card);
    app.add_systems(Update, update_notifications_ui);
    app.add_systems(
        Update,
        update_ui_hover_state.before(update_camera_controller),
    );
    app.add_systems(Update, update_cursor_icon);

    // Camera and controls
    app.add_systems(Update, update_camera_controller);
    app.add_systems(Update, apply_camera_transform);
    app.add_systems(Update, auto_inspect_selected_planet);

    // Performance and quality systems
    app.add_systems(
        Update,
        update_planet_selection_visuals.run_if(every_n_frames(2)),
    );
    app.add_systems(Update, update_performance_stats);
    app.add_systems(Update, log_performance_stats);
    app.add_systems(Update, adaptive_quality_system);

    // Vulkan compute (native only)
    #[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::systems::init_vulkan_solver,
    );

    // Screenshot and recording
    app.add_systems(Update, take_pending_screenshot);
    app.add_systems(Update, toggle_video_recording);
    app.add_systems(Update, handle_video_recording);

    // Terrain systems
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::terrain_systems::update_terrain_visibility,
    );
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::terrain_systems::generate_terrain_mesh,
    );
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::terrain_systems::initialize_terrain_lod,
    );
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::terrain_systems::update_terrain_lod,
    );
    // Terrain orbital synchronization - high priority
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::terrain_systems::update_terrain_synchronization,
    );

    // Rocket systems
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::rocket_systems::update_rocket_physics,
    );
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::rocket_systems::update_rocket_controls,
    );
    app.add_systems(
        Update,
        crate::infrastructure::bevy_adapters::rocket_systems::update_rocket_terrain_interaction,
    );

    app.add_systems(Update, update_starfield_position);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let is_gyro_mode = args.contains(&"gyro".to_string());
    let is_craft_mode = args.contains(&"craft".to_string());

    let title = if is_gyro_mode {
        "Cosmic Systems - Gyro Propulsion"
    } else if is_craft_mode {
        "Cosmic Systems - ZPE Craft"
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

    let plugins = DefaultPlugins
        .set(window_plugin)
        .set(LogPlugin {
            filter: "info,bevy_render::view::window=error,wgpu_hal::vulkan::instance=error".to_string(),
            ..default()
        });

    let mut app = App::new();
    app.add_plugins(plugins);

    if is_gyro_mode {
        setup_gyro_mode(&mut app);
    } else if is_craft_mode {
        app.insert_resource(SolarCameraEnabled(false));
        setup_solar_system_mode(&mut app);
        setup_craft_systems(&mut app);
        register_education_systems(&mut app);
    } else {
        setup_solar_system_mode(&mut app);
    }

    app.run();
}
