use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use crate::infrastructure::bevy_adapters::components::PerformanceStats;
use wasm_bindgen::prelude::*;
use web_sys;

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

use crate::application::startup::*;
use crate::infrastructure::bevy_adapters::components::{
    HoveredPlanet, NotificationQueue, ScreenshotState, SelectedPlanet,
};
use crate::infrastructure::bevy_adapters::systems::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Info).unwrap();

    web_sys::console::log_1(&"🚀 Starting Cosmic Systems Simulator (WASM)".into());

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: "Cosmic Systems Simulator".to_string(),
            canvas: Some("#bevy".to_owned()),
            resolution: (1280.0, 720.0).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins.set(window_plugin);

    let mut app = App::new();
    app.add_plugins(plugins);
    app.add_plugins(EguiPlugin);

    // Solar system mode (no gyro mode for web)
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
    app.add_systems(Startup, setup_space);

    // Physics systems run on FixedUpdate for consistent simulation
    app.add_systems(FixedUpdate, update_planet_positions);
    app.add_systems(FixedUpdate, update_planet_rotations);
    app.add_systems(FixedUpdate, update_moon_orbit_positions);
    app.add_systems(Update, update_orbit_visuals);
    app.add_systems(Update, update_orbit_visibility);
    app.add_systems(Update, update_planet_reflections);
    app.add_systems(Update, handle_solar_system_input);
    app.add_systems(Update, handle_planet_selection);
    app.add_systems(Update, handle_mouse_planet_selection);
    app.add_systems(Update, display_navigation_bar);
    app.add_systems(
        Update,
        update_planet_selection_visuals.run_if(every_n_frames(2)),
    );
    app.add_systems(Update, update_performance_stats);
    app.add_systems(Update, display_hover_info);
    app.add_systems(Update, display_notifications);
    app.add_systems(Update, take_pending_screenshot);
    app.add_systems(Update, update_camera_controller);
    app.add_systems(Update, apply_camera_transform);
    app.add_systems(Update, auto_inspect_selected_planet);

    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        if let Ok(Some(loading)) = document.query_selector(".loading") {
            loading.remove();
        }
    }

    web_sys::console::log_1(&"✅ Cosmic Systems Simulator initialized successfully".into());
    app.run();
}
