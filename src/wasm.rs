use bevy::asset::{AssetMetaCheck, AssetPlugin};
use bevy::prelude::*;
use bevy::render::view::Msaa;
use bevy::time::Fixed;
use crate::infrastructure::bevy_adapters::components::{ChromeOptimizations, PerformanceStats};
use crate::infrastructure::web_workers::physics_worker::{adapt_worker_pool, PhysicsWorkerPool};
use js_sys::Reflect;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
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

use crate::application::solar_system_startup::spawn_bodies_progressively;
use crate::application::startup::*;
use crate::infrastructure::bevy_adapters::components::{
    CameraInputState, HoveredPlanet, NotificationQueue, ScreenshotState, SelectedPlanet,
    UiPointerState,
};
use crate::infrastructure::bevy_adapters::systems::*;
use crate::presentation::ui::*;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    prepare_dom_for_wasm();
    web_sys::console::log_1(&"🚀 Starting Cosmic Systems Simulator (WASM)".into());

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: "Cosmic Systems Simulator".to_string(),
            canvas: Some("#bevy".to_owned()),
            fit_canvas_to_parent: true,
            resolution: (1280.0, 720.0).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins
        .set(window_plugin)
        .set(AssetPlugin {
            file_path: "assets".to_string(),
            watch_for_changes_override: Some(false),
            meta_check: AssetMetaCheck::Never,
            ..default()
        });

    let mut app = App::new();
    app.add_plugins(plugins);
    app.insert_resource(Msaa::Off);

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
    let (is_chrome, webgpu_supported) = detect_chrome_and_webgpu();
    let mut perf_stats = PerformanceStats::default();
    perf_stats.target_fps = if is_chrome { 60.0 } else { 45.0 };
    app.insert_resource(perf_stats);
    let worker_pool = PhysicsWorkerPool::new_dynamic();
    let worker_target = worker_pool.worker_count();
    app.insert_resource(ChromeOptimizations {
        is_chrome,
        webgpu_supported,
        webgpu_enabled: is_chrome && webgpu_supported,
        worker_target,
    });
    app.insert_non_send_resource(worker_pool);
    app.insert_resource(UiPointerState::default());
    app.insert_resource(CameraInputState::default());
    app.insert_resource(Time::<Fixed>::from_hz(30.0));
    app.add_systems(Startup, setup_space);
    app.add_systems(Startup, setup_ui);

    // Physics systems run on FixedUpdate for consistent simulation
    app.add_systems(FixedUpdate, update_planet_positions);
    app.add_systems(FixedUpdate, update_planet_rotations);
    app.add_systems(FixedUpdate, update_moon_orbit_positions);
    app.add_systems(Update, spawn_bodies_progressively);
    app.add_systems(Update, update_orbit_visuals);
    app.add_systems(Update, update_orbit_visibility);
    app.add_systems(Update, update_planet_reflections);
    app.add_systems(
        Update,
        queue_pending_material_textures.before(apply_pending_material_textures),
    );
    app.add_systems(Update, apply_pending_material_textures);
    app.add_systems(Update, handle_solar_system_input);
    app.add_systems(Update, handle_planet_selection);
    app.add_systems(Update, handle_mouse_planet_selection);
    app.add_systems(Update, handle_nav_interactions);
    app.add_systems(Update, update_navbar);
    app.add_systems(
        Update,
        update_planet_selection_visuals.run_if(every_n_frames(4)),
    );
    app.add_systems(Update, update_performance_stats);
    app.add_systems(Update, adapt_worker_pool);
    app.add_systems(Update, cap_fixed_overstep);
    app.add_systems(Update, update_info_card);
    app.add_systems(Update, update_notifications_ui);
    app.add_systems(Update, update_ui_hover_state.before(update_camera_controller));
    app.add_systems(Update, take_pending_screenshot);
    app.add_systems(Update, update_camera_controller);
    app.add_systems(Update, apply_camera_transform);
    app.add_systems(Update, auto_inspect_selected_planet);

    prepare_dom_for_wasm();

    log_webgpu_status(is_chrome, webgpu_supported);
    if worker_target == 0 {
        web_sys::console::log_1(&"Worker pool unavailable; running on main thread".into());
    } else {
        web_sys::console::log_1(
            &format!("Worker pool initialized with {worker_target} workers").into(),
        );
    }
    web_sys::console::log_1(&"✅ Cosmic Systems Simulator initialized successfully".into());
    app.run();
}

fn prepare_dom_for_wasm() {
    if let Some(window) = web_sys::window() {
        if let Some(document) = window.document() {
            if let Some(root) = document.document_element() {
                if let Ok(root) = root.dyn_into::<web_sys::HtmlElement>() {
                    let _ = root.style().set_property("width", "100%");
                    let _ = root.style().set_property("height", "100%");
                    let _ = root.style().set_property("margin", "0");
                    let _ = root.style().set_property("padding", "0");
                    let _ = root.style().set_property("overflow", "hidden");
                }
            }

            if let Some(body) = document.body() {
                let _ = body.style().set_property("width", "100%");
                let _ = body.style().set_property("height", "100%");
                let _ = body.style().set_property("margin", "0");
                let _ = body.style().set_property("padding", "0");
                let _ = body.style().set_property("overflow", "hidden");
            }

            if let Ok(Some(canvas)) = document.query_selector("#bevy") {
                if let Ok(canvas) = canvas.dyn_into::<web_sys::HtmlElement>() {
                    let _ = canvas.style().set_property("width", "100%");
                    let _ = canvas.style().set_property("height", "100%");
                    let _ = canvas.style().set_property("display", "block");
                }
            }

            if let Ok(Some(loading)) = document.query_selector(".loading") {
                loading.remove();
            }
        }
    }
}

fn detect_chrome_and_webgpu() -> (bool, bool) {
    let user_agent = web_sys::window()
        .and_then(|window| window.navigator().user_agent().ok())
        .unwrap_or_default();
    let is_chrome = user_agent.contains("Chrome") && !user_agent.contains("Edg");

    let webgpu_supported = web_sys::window()
        .map(|window| JsValue::from(window.navigator()))
        .and_then(|navigator| Reflect::has(&navigator, &JsValue::from_str("gpu")).ok())
        .unwrap_or(false);

    (is_chrome, webgpu_supported)
}

fn log_webgpu_status(is_chrome: bool, webgpu_supported: bool) {
    if is_chrome && webgpu_supported {
        web_sys::console::log_1(
            &"WebGPU enabled by default for Chrome (high-performance backend preferred)".into(),
        );
    } else if webgpu_supported {
        web_sys::console::log_1(&"WebGPU supported, using CPU fallback until enabled".into());
    } else {
        web_sys::console::log_1(&"WebGPU unavailable, using SIMD + workers fallback".into());
    }
}
