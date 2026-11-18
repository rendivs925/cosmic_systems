//! Shared application composition for every platform entry point.
//!
//! Desktop (`main.rs`) and Android (`android.rs`) differ only in how launch
//! options arrive and in presentation defaults. The Bevy app itself — plugins,
//! schedule composition, mode selection — is built here so both entries stay
//! identical (AGENTS.md sections 5 and 66).

use bevy::app::{TaskPoolOptions, TaskPoolPlugin, TaskPoolThreadAssignmentPolicy};
#[cfg(target_os = "android")]
use bevy::asset::AssetMetaCheck;
use bevy::asset::AssetPlugin;
#[cfg(target_os = "android")]
use bevy::light::DirectionalLight;
use bevy::log::LogPlugin;
use bevy::prelude::*;
#[cfg(target_os = "android")]
use bevy::window::{MonitorSelection, WindowMode};
#[cfg(not(target_os = "android"))]
use std::num::NonZero;

#[cfg(not(target_os = "android"))]
use crate::application::modes::window_size_from_env;
use crate::application::modes::{LaunchOptions, Mode};
use crate::application::offscreen_capture::{OffscreenCapture, OffscreenCapturePlugin};
use crate::application::plugins::{
    CraftModePlugin, RocketModePlugin, SharedSimulationPlugin, SolarSystemModePlugin,
};
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::application::solar_system_startup::SolarCameraEnabled;
use crate::domain::services::gravity::ForceModelConfig;
use crate::infrastructure::bevy_adapters::rocket::gravity_orbit::ActiveForceModel;
use crate::infrastructure::bevy_adapters::rocket::recovery::RecoveryDeckRequest;

/// Reject an unknown requested vehicle before creating the window or renderer.
///
/// Returns the user-facing error instead of exiting: the desktop entry maps it
/// to `exit(2)`, while Android reports it and keeps running.
pub fn validate_vehicle_selection(options: &LaunchOptions) -> Result<(), String> {
    if !matches!(options.mode, Mode::Rocket) {
        return Ok(());
    }
    let selection = VehicleSelection::from(options.vehicle.clone());
    let Some(requested) = selection.requested() else {
        return Ok(());
    };
    let Ok(catalog) = RocketCatalog::from_dir() else {
        // Plugin startup reports catalog load failures.
        return Ok(());
    };
    if catalog.resolve(&selection).is_some() {
        return Ok(());
    }

    let available = catalog.keys().collect::<Vec<_>>().join(", ");
    Err(format!(
        "Unknown vehicle '{}'. Available vehicles: {}",
        requested.as_str(),
        available
    ))
}

pub fn rocket_task_pool_options() -> TaskPoolOptions {
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

/// Asset plugin configuration for the platform.
///
/// Native runs load the repository's checked-in assets from disk. Android has
/// no such directory: the packaged `assets/` tree is served by Android's
/// `AssetManager`, which `bevy_asset` selects automatically for this target.
fn asset_plugin() -> AssetPlugin {
    #[cfg(target_os = "android")]
    {
        AssetPlugin {
            file_path: "assets".to_string(),
            // The APK ships no `.meta` sidecars.
            meta_check: AssetMetaCheck::Never,
            ..default()
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        AssetPlugin {
            file_path: concat!(env!("CARGO_MANIFEST_DIR"), "/assets").to_string(),
            ..default()
        }
    }
}

fn window_plugin(options: &LaunchOptions, offscreen: bool) -> WindowPlugin {
    if offscreen {
        return WindowPlugin {
            primary_window: None,
            exit_condition: bevy::window::ExitCondition::DontExit,
            close_when_requested: false,
            ..default()
        };
    }

    // Bevy's default is hard FIFO vsync, which quantizes any frame that overruns
    // a vblank to the next one and reads as judder. AutoVsync selects relaxed
    // FIFO where the driver supports it, and a short present queue keeps pacing
    // and input latency consistent. `COSMIC_SYSTEMS_PRESENT_MODE=none` disables
    // vsync for profiling only, so measured frame cost is not quantized.
    let present_mode = match std::env::var("COSMIC_SYSTEMS_PRESENT_MODE").as_deref() {
        Ok("none") => bevy::window::PresentMode::AutoNoVsync,
        _ => bevy::window::PresentMode::AutoVsync,
    };

    #[cfg(target_os = "android")]
    {
        WindowPlugin {
            primary_window: Some(Window {
                title: options.mode.title().to_string(),
                // The surface size is owned by the activity; requesting a
                // windowed resolution has no meaning on mobile.
                mode: WindowMode::BorderlessFullscreen(MonitorSelection::Primary),
                present_mode,
                ..default()
            }),
            ..default()
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        WindowPlugin {
            primary_window: Some(Window {
                title: options.mode.title().to_string(),
                resolution: window_size_from_env().unwrap_or((1280, 720)).into(),
                present_mode,
                desired_maximum_frame_latency: NonZero::<u32>::new(2),
                ..default()
            }),
            ..default()
        }
    }
}

/// Compose the simulator for the selected mode.
///
/// The caller owns platform-specific setup (argument parsing, asset staging)
/// and runs the returned app.
pub fn build_app(options: LaunchOptions) -> App {
    let mode = options.mode;
    let vehicle_selection = if matches!(mode, Mode::Rocket) {
        Some(VehicleSelection::from(options.vehicle.clone()))
    } else {
        None
    };
    let launch_site = options.site.unwrap_or_default();
    let recovery_deck = options.recovery_deck;
    let force_model = options.force_model;

    // Offscreen capture renders into an image and writes a PNG, so it needs no
    // presentable swapchain. That is the only way to get a frame on a machine
    // whose Vulkan presentation path is broken while rendering itself works.
    let offscreen = OffscreenCapture::from_env();

    let plugins = DefaultPlugins
        .set(window_plugin(&options, offscreen.is_some()))
        .set(asset_plugin())
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

    let plugins = if offscreen.is_some() {
        plugins.disable::<bevy::winit::WinitPlugin>()
    } else {
        plugins
    };

    let mut app = App::new();
    app.add_plugins(plugins);

    #[cfg(target_os = "android")]
    {
        // `WinitSettings::mobile()` is *reactive*: it redraws only on input,
        // which is right for a static menu but freezes a continuously
        // animating simulation. The simulator needs a continuous loop.
        app.insert_resource(bevy::winit::WinitSettings::continuous());
        // Loading screen temporarily disabled for diagnostics
        // app.add_plugins(crate::application::loading::AndroidLoadingPlugin);
        app.add_systems(Update, (android_presentation_defaults, android_diagnostics));
        // Install touch camera controls and presentation
        crate::android::presentation::install(&mut app);
    }

    if let Some(capture) = offscreen {
        app.add_plugins(OffscreenCapturePlugin::runner());
        app.add_plugins(OffscreenCapturePlugin::new(capture));
    }

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
            app.insert_resource(launch_site);
            if let Some(deck) = recovery_deck {
                app.insert_resource(RecoveryDeckRequest(deck));
            }
            if let Some(tier) = force_model {
                app.insert_resource(ActiveForceModel(ForceModelConfig::new(tier)));
            }
            // Rocket mode owns its camera, HUD, and input context.
            app.add_plugins((SharedSimulationPlugin, RocketModePlugin));
        }
    }

    app
}

/// Apply Android-safe presentation settings to newly spawned views and lights.
///
/// MSAA and shadow-map rendering are known to panic or crash on several Android
/// driver stacks (bevyengine/bevy#8229 and #8214), and Bevy's own mobile example
/// disables both. Applying it here also covers cameras and lights spawned later
/// by a mode plugin.
#[cfg(target_os = "android")]
fn android_presentation_defaults(
    mut commands: Commands,
    cameras: Query<Entity, Added<Camera>>,
    mut lights: Query<&mut DirectionalLight, Added<DirectionalLight>>,
) {
    for entity in &cameras {
        commands.entity(entity).insert(Msaa::Off);
    }
    for mut light in &mut lights {
        light.shadows_enabled = false;
    }
}

/// Accumulated on-device diagnostics state (Android only).
#[cfg(target_os = "android")]
#[derive(Default)]
struct AndroidDiagnosticsState {
    elapsed_s: f32,
    frames: u32,
    samples: u32,
    max_delta_s: f32,
    stalls: u32,
    adapter_logged: bool,
    presentation_logged: bool,
}

/// Report the renderer and frame pacing into the startup log so optimisation
/// work can be measured on the device without adb.
#[cfg(target_os = "android")]
fn android_diagnostics(
    time: Res<Time>,
    adapter: Option<Res<bevy::render::renderer::RenderAdapterInfo>>,
    cameras: Query<&Msaa>,
    lights: Query<&DirectionalLight>,
    windows: Query<&Window>,
    mut state: Local<AndroidDiagnosticsState>,
) {
    use crate::android::diagnostics;

    if !state.adapter_logged {
        if let Some(adapter) = adapter {
            diagnostics::log(&format!(
                "render: adapter='{}' backend={:?} driver='{}'",
                adapter.0.name, adapter.0.backend, adapter.0.driver
            ));
            state.adapter_logged = true;
        }
    }

    let delta = time.delta_secs();
    state.elapsed_s += delta;
    state.frames += 1;
    state.max_delta_s = state.max_delta_s.max(delta);
    if delta > 0.033 {
        state.stalls += 1;
    }

    if !state.presentation_logged && state.elapsed_s >= 3.0 {
        let camera_count = cameras.iter().count();
        let msaa_off = cameras.iter().filter(|msaa| **msaa == Msaa::Off).count();
        let shadow_casters = lights.iter().filter(|light| light.shadows_enabled).count();
        diagnostics::log(&format!(
            "render: cameras={camera_count} msaa_off={msaa_off} shadow_casters={shadow_casters}"
        ));
        state.presentation_logged = true;
    }

    if state.elapsed_s >= 5.0 {
        if state.samples < 6 {
            let fps = state.frames as f32 / state.elapsed_s;
            let memory = crate::android::diagnostics::memory_mb().unwrap_or(0);
            let (width, height) = windows
                .single()
                .map(|window| (window.width(), window.height()))
                .unwrap_or((0.0, 0.0));
            diagnostics::log(&format!(
                "perf: fps={fps:.1} avg_ms={:.2} max_ms={:.2} stalls={} mem_mb={memory} window={width:.0}x{height:.0}",
                1000.0 / fps.max(0.001),
                state.max_delta_s * 1000.0,
                state.stalls
            ));
        }
        state.samples += 1;
        state.elapsed_s = 0.0;
        state.frames = 0;
        state.max_delta_s = 0.0;
        state.stalls = 0;
    }
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
