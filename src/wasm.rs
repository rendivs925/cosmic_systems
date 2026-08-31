use crate::infrastructure::bevy_adapters::material_systems::{
    apply_pending_material_textures, apply_texture_worker_results, queue_pending_material_textures,
};
use crate::infrastructure::bevy_adapters::performance_components::PerformanceStats;
use crate::infrastructure::plugins::{SharedSimulationPlugin, SolarSystemModePlugin};
use crate::infrastructure::web_workers::texture_worker::TextureDecodeWorker;
use bevy::asset::{AssetMetaCheck, AssetPlugin};
use bevy::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys;

#[wasm_bindgen(start)]
pub fn main() {
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);

    prepare_dom_for_wasm();
    web_sys::console::log_1(&"Starting Cosmic Systems Simulator (WASM)".into());

    let window_plugin = WindowPlugin {
        primary_window: Some(Window {
            title: "Cosmic Systems Simulator".to_string(),
            canvas: Some("#bevy".to_owned()),
            fit_canvas_to_parent: true,
            resolution: (1280u32, 720u32).into(),
            ..default()
        }),
        ..default()
    };

    let plugins = DefaultPlugins.set(window_plugin).set(AssetPlugin {
        file_path: "assets".to_string(),
        watch_for_changes_override: Some(false),
        meta_check: AssetMetaCheck::Never,
        ..default()
    });

    let mut app = App::new();
    app.add_plugins(plugins);
    // Reuse the native solar composition so camera, render-origin, orbit, and
    // ephemeris presentation systems cannot diverge by platform.
    app.add_plugins((SharedSimulationPlugin, SolarSystemModePlugin));

    let perf_stats = PerformanceStats::default();
    app.insert_resource(perf_stats);
    app.insert_non_send_resource(TextureDecodeWorker::new());
    app.add_systems(
        Update,
        queue_pending_material_textures.before(apply_pending_material_textures),
    );
    app.add_systems(
        Update,
        apply_texture_worker_results.before(apply_pending_material_textures),
    );
    web_sys::console::log_1(&"Cosmic Systems Simulator initialized successfully".into());
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
