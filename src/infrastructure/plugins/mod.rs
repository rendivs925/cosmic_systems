// Application mode composition plugins.
//
// These plugins move the system-registration code formerly living in
// `src/main.rs` into composable Bevy plugins. Each mode is composed from the
// shared solar-system plugin plus mode-specific plugins, keeping mode-specific
// behavior isolated per AGENTS.md sections 5, 35, and 66.

use bevy::app::{App, RunFixedMainLoop, RunFixedMainLoopSystems};
use bevy::prelude::*;
use bevy::time::TimeSystems;

use crate::application::craft_startup::spawn_craft;
use crate::application::craft_startup::spawn_craft_model;
use crate::application::craft_startup::spawn_craft_ui;
use crate::application::gyro_startup::setup_gyro;
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::application::rocket_spawning::spawn_rockets;
use crate::application::solar_system_startup::setup_space;
#[cfg(feature = "dem")]
use crate::application::terrain_config::EarthTerrainConfig;
use crate::components::rocket::RocketMode;
use crate::domain::events::{
    CommsBlackoutEvent, FairingSeparatedEvent, RelaunchRequested, SplashdownDetectedEvent,
    StageSeparatedEvent,
};
use crate::domain::services::simulation_time::{
    accrue_time_warp, advance_fixed_simulation_time, handle_time_acceleration_input,
    run_bounded_fixed_main_schedule, sync_fixed_timestep, SimulationTime,
};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::infrastructure::bevy_adapters::components::{
    CameraInputState, HoveredPlanet, NotificationQueue, PerformanceStats, ScreenshotState,
    SelectedPlanet, UiPointerState, ZenMode,
};
use crate::infrastructure::bevy_adapters::craft_components::{
    CraftCameraState, CraftControlState, CraftEffectsEnabled, CraftTravelTarget,
};
use crate::infrastructure::bevy_adapters::craft_effects::update_craft_visuals;
use crate::infrastructure::bevy_adapters::craft_systems::{
    handle_craft_input, sync_craft_transform, update_craft_camera, update_craft_physics,
};
use crate::infrastructure::bevy_adapters::craft_ui::update_craft_ui;
use crate::infrastructure::bevy_adapters::education_systems::register_education_systems;
use crate::infrastructure::bevy_adapters::gyroscope_systems::{
    handle_input, update_gyroscopes, update_thrust,
};
use crate::infrastructure::bevy_adapters::performance_systems::{
    request_screenshot_input, take_pending_screenshot,
};
use crate::infrastructure::bevy_adapters::rocket_camera_systems::{
    handle_free_camera_input, handle_rocket_camera_input, setup_rocket_camera_and_origin,
    setup_rocket_camera_controller, update_rocket_camera, update_rocket_camera_projection,
};
use crate::infrastructure::bevy_adapters::rocket_contact::{
    advance_topple, deploy_landing_legs, resolve_ground_contact, TerrainSurfaceSampleCache,
};
use crate::infrastructure::bevy_adapters::rocket_control::{actuation_system, control_system};
use crate::infrastructure::bevy_adapters::rocket_debug::RocketDebugPlugin;
use crate::infrastructure::bevy_adapters::rocket_dynamics::{
    accumulate_forces, aerodynamic_forces, aerodynamic_torque, integrate_6dof,
};
use crate::infrastructure::bevy_adapters::rocket_entry::{
    compute_ablation, compute_heating, compute_parachute_forces, compute_plasma_blackout,
    compute_retro_propulsion,
};
use crate::infrastructure::bevy_adapters::rocket_environment::{
    setup_rocket_sky_color, setup_rocket_sun_light, update_rocket_sky_color,
    update_sun_day_night_cycle,
};
use crate::infrastructure::bevy_adapters::rocket_flight_conditions::refresh_flight_conditions;
use crate::infrastructure::bevy_adapters::rocket_gravity_orbit::{
    update_orbital_elements, update_rocket_gravity,
};
use crate::infrastructure::bevy_adapters::rocket_guidance::{
    guidance_system, update_drone_ship_landing_targets,
};
use crate::infrastructure::bevy_adapters::rocket_hud::{
    spawn_rocket_hud_system, update_rocket_hud_system,
};
use crate::infrastructure::bevy_adapters::rocket_lifecycle::{
    apply_relaunch_requests, handle_relaunch_input_system, handle_rocket_launch_input,
    RelaunchCommandQueue,
};
use crate::infrastructure::bevy_adapters::rocket_orbit::RocketOrbitPlugin;
use crate::infrastructure::bevy_adapters::rocket_planet::{
    isolate_rocket_presentation, setup_rocket_planets, update_rocket_planets, RocketBoundPlanet,
};
use crate::infrastructure::bevy_adapters::rocket_presentation::{
    capture_render_state, interpolate_render_transform,
};
use crate::infrastructure::bevy_adapters::rocket_propulsion::{
    propulsion_consumption, propulsion_gimbal, propulsion_staging, propulsion_thrust,
};
use crate::infrastructure::bevy_adapters::rocket_recovery::{
    resolve_drone_ship_deck_contact, station_keep_drone_ships,
};
use crate::infrastructure::bevy_adapters::rocket_replay::{
    apply_replay_actions_system, record_replay_snapshot_system, replay_active, replay_inactive,
    ReplayAction, ReplaySnapshotStream,
};
use crate::infrastructure::bevy_adapters::rocket_separation::{
    check_fairing_separation, spent_stage_aerodynamics, update_spent_stage_lifecycle,
};
use crate::infrastructure::bevy_adapters::rocket_telemetry::{
    compute_rocket_telemetry_system, handle_flight_recorder_export_system,
    handle_flight_recorder_input_system, record_flight_data_system, rocket_event_feed_system,
    RocketEventFeed,
};
use crate::infrastructure::bevy_adapters::rocket_terrain_map::RocketTerrainMapPlugin;
use crate::infrastructure::bevy_adapters::systems::*;
use crate::infrastructure::bevy_adapters::terrain_render::{
    recenter_render_origin, TerrainRenderConfig, TerrainRenderPlugin,
};
use crate::infrastructure::bevy_adapters::terrain_streaming::{
    collect_terrain_warmup_tasks, prebake_prelaunch_launchpad_patch, stream_terrain_patches,
    warmup_terrain_system, TerrainStreamingResource, TerrainWarmupTasks,
};
use crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState;
#[cfg(all(not(target_arch = "wasm32"), feature = "ash", feature = "parallel"))]
use crate::infrastructure::bevy_adapters::webgpu_systems::init_vulkan_solver;
use crate::presentation::ui::*;
use crate::presentation::ui_setup::setup_ui;
use crate::systems::sets::RocketSet;

/// Run condition for visual updates (every 3 frames).
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

/// A paused simulation must not keep advancing its fixed physics pipeline.
fn simulation_unpaused(sim_time: Res<SimulationTime>) -> bool {
    !sim_time.paused
}

/// Rocket time warp owns fixed-loop execution so high warp cannot make Bevy
/// exhaust an unbounded fixed-time backlog before it renders another frame.
fn never_run_default_fixed_loop() -> bool {
    false
}

/// The native Kepler solver serves the solar-system presentation, not rocket
/// flight's fixed-step dynamics. Avoid its costly device setup in rocket mode.
#[cfg(any(
    test,
    all(not(target_arch = "wasm32"), feature = "ash", feature = "parallel")
))]
fn shared_solar_presentation_requires_vulkan(rocket_mode_enabled: bool) -> bool {
    !rocket_mode_enabled
}

#[cfg(all(not(target_arch = "wasm32"), feature = "ash", feature = "parallel"))]
fn vulkan_solver_required(rocket_mode: Option<Res<RocketMode>>) -> bool {
    shared_solar_presentation_requires_vulkan(rocket_mode.is_some())
}

fn solar_presentation_enabled(rocket_mode: Option<Res<RocketMode>>) -> bool {
    rocket_mode.is_none()
}

/// Shared solar-system world: resources, physics, orbit visuals, performance,
/// Vulkan compute, screenshot/recording, terrain, and starfield.
pub struct SharedSimulationPlugin;

impl Plugin for SharedSimulationPlugin {
    fn build(&self, app: &mut App) {
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
        app.insert_resource(VideoRecordingState::default());
        app.insert_resource(
            crate::infrastructure::bevy_adapters::systems::QualityAdaptationResource::default(),
        );

        // Startup systems
        app.add_systems(Startup, setup_space);

        // Celestial state is fixed-step and camera-independent. Rendering reads
        // the resulting transforms during Update.
        app.add_systems(
            FixedUpdate,
            (update_planet_positions, update_planet_rotations)
                .chain()
                .run_if(solar_presentation_enabled),
        );

        // Visual and interaction systems
        app.add_systems(
            Update,
            update_orbit_visuals
                .after(auto_inspect_selected_planet)
                .run_if(solar_presentation_enabled),
        );
        app.add_systems(
            Update,
            update_orbit_thickness
                .after(auto_inspect_selected_planet)
                .run_if(solar_presentation_enabled),
        );
        app.add_systems(
            Update,
            update_orbit_visibility.run_if(solar_presentation_enabled),
        );
        app.add_systems(
            Update,
            (
                interpolate_planet_transforms,
                rebase_solar_presentation,
                update_moon_orbit_positions,
            )
                .chain()
                .after(handle_planet_selection)
                .after(handle_mouse_planet_selection)
                .before(update_camera_controller)
                .before(update_craft_camera)
                .run_if(solar_presentation_enabled),
        );
        app.add_systems(
            Update,
            preserve_sun_disc_at_overview_distances
                .after(rebase_solar_presentation)
                .before(update_camera_controller)
                .run_if(solar_presentation_enabled),
        );
        app.add_systems(
            Update,
            update_planet_reflections.run_if(solar_presentation_enabled),
        );
        app.add_systems(Update, apply_pending_material_textures);

        // Performance and quality systems
        app.add_systems(
            Update,
            (
                update_performance_stats,
                adaptive_quality_system,
                log_performance_stats,
            )
                .chain(),
        );

        // Vulkan compute (native only)
        #[cfg(all(not(target_arch = "wasm32"), feature = "ash", feature = "parallel"))]
        app.add_systems(Update, init_vulkan_solver.run_if(vulkan_solver_required));

        // Screenshot and recording
        app.add_systems(
            Update,
            (request_screenshot_input, take_pending_screenshot).chain(),
        );
        app.add_systems(Update, toggle_video_recording);
        app.add_systems(Update, handle_video_recording);

        app.add_systems(Update, update_starfield_position);
    }
}

/// Solar-system mode: UI, navigation, camera, and selection behavior.
pub struct SolarSystemModePlugin;

impl Plugin for SolarSystemModePlugin {
    fn build(&self, app: &mut App) {
        // Startup systems
        app.add_systems(Startup, setup_ui);

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

        // Camera input and selected-body framing consume the interpolated
        // celestial presentation state in a defined order.
        app.add_systems(
            Update,
            (
                update_camera_controller,
                apply_camera_transform,
                auto_inspect_selected_planet,
            )
                .chain()
                .after(interpolate_planet_transforms)
                .after(rebase_solar_presentation),
        );

        // Selection visuals
        app.add_systems(
            Update,
            update_planet_selection_visuals.run_if(every_n_frames(2)),
        );
    }
}

/// Craft / UFO mode: craft state, spawning, and control systems.
pub struct CraftModePlugin;

impl Plugin for CraftModePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CraftControlState::default());
        app.insert_resource(CraftCameraState::default());
        app.insert_resource(CraftTravelTarget::default());
        app.insert_resource(CraftEffectsEnabled(false));
        app.add_systems(Startup, spawn_craft);
        app.add_systems(Startup, spawn_craft_ui);
        app.add_systems(
            FixedUpdate,
            update_craft_physics.after(update_planet_positions),
        );
        app.add_systems(
            Update,
            (
                handle_craft_input,
                sync_craft_transform,
                update_craft_camera,
                spawn_craft_model,
                update_craft_visuals,
                update_craft_ui,
            )
                .chain(),
        );

        register_education_systems(app);
    }
}

/// Gyro propulsion mode.
pub struct GyroModePlugin;

impl Plugin for GyroModePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SimulationParameters::new());
        app.add_systems(Startup, setup_gyro);
        app.add_systems(Update, update_gyroscopes);
        app.add_systems(Update, update_thrust);
        app.add_systems(Update, handle_input);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_rockets_system(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    catalog: Res<RocketCatalog>,
    selection: Res<VehicleSelection>,
    terrain_query: Query<(
        &crate::infrastructure::bevy_adapters::components::PlanetComponent,
        &crate::infrastructure::bevy_adapters::components::PlanetTerrain,
    )>,
) {
    let launch_body = CelestialBodyId::earth();
    let terrain_source = terrain_query
        .iter()
        .find(|(planet, _)| planet.matches_body(&launch_body))
        .map(|(_, terrain)| terrain.source.as_ref());
    if terrain_source.is_none() {
        panic!("Rocket launch configuration references unknown body '{launch_body}'");
    }
    spawn_rockets(
        &mut commands,
        &mut meshes,
        &mut materials,
        &catalog,
        selection.0.as_deref(),
        terrain_source,
    );
}

/// Rocket flight mode: composes the shared solar-system world and registers
/// rocket-only systems.
pub struct RocketModePlugin;

impl Plugin for RocketModePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "dem")]
        app.insert_resource(EarthTerrainConfig::from_environment());

        // Vehicle catalog: data-driven definitions from assets/configs/rockets
        // (AGENTS.md section 65: fail fast on invalid configuration).
        #[cfg(not(target_arch = "wasm32"))]
        match RocketCatalog::from_dir() {
            Ok(catalog) => {
                app.insert_resource(catalog);
            }
            Err(e) => panic!("Rocket vehicle configuration failed to load: {e}"),
        }
        #[cfg(target_arch = "wasm32")]
        app.init_resource::<RocketCatalog>();
        app.init_resource::<VehicleSelection>();

        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Startup,
            spawn_rockets_system
                .after(setup_space)
                .after(warmup_terrain_system),
        );

        // Rocket telemetry resource for HUD and flight log.
        app.init_resource::<RocketTelemetry>();
        app.init_resource::<RocketEventFeed>();
        app.init_resource::<ReplaySnapshotStream>();
        app.add_message::<ReplayAction>();

        // Terrain rendering configuration.
        app.init_resource::<TerrainRenderConfig>();

        // Entry physics configuration.
        app.init_resource::<EntryPhysicsConfig>();

        // Rocket domain messages (blackout edges, splashdown, staging, relaunch).
        app.add_message::<CommsBlackoutEvent>();
        app.add_message::<SplashdownDetectedEvent>();
        app.add_message::<StageSeparatedEvent>();
        app.add_message::<FairingSeparatedEvent>();
        app.add_message::<RelaunchRequested>();
        app.init_resource::<RelaunchCommandQueue>();

        // Rocket camera resources.
        app.init_resource::<RocketCameraMode>();
        app.init_resource::<RocketCameraConfig>();
        // Rocket mode flag for conditional shared systems.
        app.init_resource::<RocketMode>();
        // Rocket planet system resource.
        app.init_resource::<RocketBoundPlanet>();

        // Simulation time resource for fixed-timestep physics and time acceleration.
        app.insert_resource(SimulationTime::default());
        // Configure the first fixed tick as well as later time-warp changes.
        app.add_systems(Startup, sync_fixed_timestep);

        // Cube-sphere terrain streaming around the rocket.
        app.insert_resource(TerrainStreamingResource::default());
        app.init_resource::<TerrainWarmupTasks>();
        app.init_resource::<TerrainSurfaceSampleCache>();
        app.add_systems(Startup, warmup_terrain_system.after(setup_space));
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Startup,
            prebake_prelaunch_launchpad_patch
                .after(spawn_rockets_system)
                .after(warmup_terrain_system),
        );
        // Terrain priorities use the current presentation camera frustum.
        app.add_systems(Update, stream_terrain_patches.after(update_rocket_camera));
        app.add_systems(Update, collect_terrain_warmup_tasks);

        // Terrain rendering plugin (spawns meshes from streaming patches).
        app.add_plugins(TerrainRenderPlugin);

        // Rocket debug visualization plugin.
        app.add_plugins(RocketDebugPlugin);

        // Presentation-only patched-conics prediction and maneuver markers.
        app.add_plugins(RocketOrbitPlugin);

        // Compact body-fixed terrain map and trajectory overlays.
        app.add_plugins(RocketTerrainMapPlugin);

        // Time warp accrues demand from real time, while the replacement fixed
        // runner below consumes a bounded deterministic batch each frame.
        app.add_systems(First, accrue_time_warp.after(TimeSystems));
        app.configure_sets(
            RunFixedMainLoop,
            RunFixedMainLoopSystems::FixedMainLoop.run_if(never_run_default_fixed_loop),
        );
        app.add_systems(RunFixedMainLoop, run_bounded_fixed_main_schedule);

        // Rocket camera systems (run in Update for smooth rendering).
        app.add_systems(
            Update,
            (
                handle_rocket_camera_input,
                handle_free_camera_input,
                interpolate_render_transform
                    .after(recenter_render_origin)
                    .after(handle_rocket_launch_input),
                update_rocket_camera,
                update_rocket_camera_projection,
            )
                .chain(),
        );

        // Rocket HUD UI (runs in Update).
        app.add_systems(Startup, spawn_rocket_hud_system);
        app.add_systems(Update, update_rocket_hud_system);

        // Rocket mode shares the existing exploration selector and orbital
        // visibility controls. They operate on the shared solar-system data,
        // while the flight camera remains independent from solar-map camera
        // controls.
        app.add_systems(
            Startup,
            setup_ui.after(setup_space).after(spawn_rocket_hud_system),
        );
        app.add_systems(
            Update,
            (handle_nav_interactions, update_navbar, update_info_card),
        );

        // Flight recorder input (runs in Update).
        app.add_systems(
            Update,
            handle_flight_recorder_input_system.run_if(replay_inactive),
        );

        // Flight-recorder CSV export (F11, runs in Update).
        app.add_systems(Update, handle_flight_recorder_export_system);

        // Relaunch input (runs in Update; mutation happens in FixedUpdate).
        app.add_systems(Update, handle_relaunch_input_system.run_if(replay_inactive));

        // Time acceleration adjusts fixed-update frequency while every physics
        // tick keeps the bounded SimulationTime timestep.
        app.add_systems(
            Update,
            (
                handle_time_acceleration_input.run_if(replay_inactive),
                sync_fixed_timestep,
            )
                .chain(),
        );

        // Replay controls are message-driven so a future HUD can issue seeks
        // without coupling playback to the telemetry recorder.
        app.add_systems(
            Update,
            apply_replay_actions_system
                .before(handle_rocket_launch_input)
                .before(update_rocket_hud_system),
        );
        app.add_systems(
            Update,
            compute_rocket_telemetry_system
                .after(apply_replay_actions_system)
                .before(update_rocket_hud_system)
                .run_if(replay_active),
        );

        // Event feed: domain messages → HUD line + flight-log entries (Update).
        app.add_systems(Update, rocket_event_feed_system);

        // Total execution order for the fixed-step flight loop (AGENTS.md
        // sections 9 and 47). `.chain()` gives real pairwise ordering — the
        // previous chained-`.before()` form only ordered Guidance against
        // each set, leaving force writers ambiguous against accumulation.
        app.configure_sets(
            FixedUpdate,
            (
                RocketSet::Atmosphere,
                RocketSet::Recovery,
                RocketSet::Guidance,
                RocketSet::Control,
                RocketSet::Actuation,
                RocketSet::Gravity,
                RocketSet::TerrainInteraction,
                RocketSet::SpentStage,
                RocketSet::EntryPhysics,
                RocketSet::AeroForces,
                RocketSet::AeroTorque,
                RocketSet::PropulsionThrust,
                RocketSet::PropulsionGimbal,
                RocketSet::AccumulateForces,
                RocketSet::Integrate,
                RocketSet::PropulsionConsumption,
                RocketSet::PropulsionStaging,
                RocketSet::AdvanceTime,
                RocketSet::OrbitalElements,
            )
                .chain()
                .run_if(simulation_unpaused),
        );
        app.configure_sets(
            FixedUpdate,
            (
                RocketSet::GroundContact,
                RocketSet::SyncRender,
                RocketSet::Telemetry,
                RocketSet::Replay,
            )
                .chain()
                .run_if(simulation_unpaused),
        );
        app.configure_sets(
            FixedUpdate,
            RocketSet::OrbitalElements.before(RocketSet::GroundContact),
        );

        app.add_systems(
            FixedUpdate,
            (
                refresh_flight_conditions.in_set(RocketSet::Atmosphere),
                station_keep_drone_ships.in_set(RocketSet::Recovery),
                update_drone_ship_landing_targets.in_set(RocketSet::Recovery),
                apply_relaunch_requests
                    .in_set(RocketSet::Guidance)
                    .before(guidance_system),
                guidance_system.in_set(RocketSet::Guidance),
                control_system.in_set(RocketSet::Control),
                actuation_system.in_set(RocketSet::Actuation),
                update_rocket_gravity.in_set(RocketSet::Gravity),
                spent_stage_aerodynamics.in_set(RocketSet::SpentStage),
                update_spent_stage_lifecycle.in_set(RocketSet::SpentStage),
                check_fairing_separation.in_set(RocketSet::SpentStage),
                compute_heating.in_set(RocketSet::EntryPhysics),
                compute_ablation.in_set(RocketSet::EntryPhysics),
                compute_plasma_blackout.in_set(RocketSet::EntryPhysics),
                compute_parachute_forces.in_set(RocketSet::EntryPhysics),
                compute_retro_propulsion.in_set(RocketSet::EntryPhysics),
                deploy_landing_legs.in_set(RocketSet::EntryPhysics),
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            (
                aerodynamic_forces.in_set(RocketSet::AeroForces),
                aerodynamic_torque.in_set(RocketSet::AeroTorque),
                propulsion_thrust.in_set(RocketSet::PropulsionThrust),
                propulsion_gimbal.in_set(RocketSet::PropulsionGimbal),
                accumulate_forces.in_set(RocketSet::AccumulateForces),
                integrate_6dof.in_set(RocketSet::Integrate),
                propulsion_consumption.in_set(RocketSet::PropulsionConsumption),
                propulsion_staging.in_set(RocketSet::PropulsionStaging),
                advance_fixed_simulation_time.in_set(RocketSet::AdvanceTime),
                update_orbital_elements.in_set(RocketSet::OrbitalElements),
                resolve_ground_contact.in_set(RocketSet::GroundContact),
                resolve_drone_ship_deck_contact
                    .in_set(RocketSet::GroundContact)
                    .before(resolve_ground_contact),
                advance_topple
                    .in_set(RocketSet::GroundContact)
                    .after(resolve_ground_contact),
                capture_render_state.in_set(RocketSet::SyncRender),
            ),
        );
        app.add_systems(
            FixedUpdate,
            (
                compute_rocket_telemetry_system.in_set(RocketSet::Telemetry),
                record_flight_data_system.in_set(RocketSet::Telemetry),
                record_replay_snapshot_system.in_set(RocketSet::Replay),
            ),
        );

        // Rocket-specific startup: camera controller, sun light, planets, and sky color.
        // Must run AFTER setup_space (spawns camera + planets) and spawn_rockets_system
        // (spawns rocket) so the render origin is set to the rocket's physical position
        // and the camera is framed on the already-spawned camera entity.
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(
            Startup,
            (
                isolate_rocket_presentation,
                setup_rocket_camera_and_origin,
                setup_rocket_camera_controller,
                setup_rocket_sun_light,
                setup_rocket_planets,
                setup_rocket_sky_color,
            )
                .chain()
                .after(setup_space)
                .after(spawn_rockets_system),
        );
        #[cfg(target_arch = "wasm32")]
        app.add_systems(
            Startup,
            (
                isolate_rocket_presentation,
                setup_rocket_camera_and_origin,
                setup_rocket_camera_controller,
                setup_rocket_sun_light,
                setup_rocket_planets,
                setup_rocket_sky_color,
            )
                .chain()
                .after(setup_space),
        );

        // Pre-launch hold: Space to launch.
        app.add_systems(Update, handle_rocket_launch_input.run_if(replay_inactive));

        // Keep the rocket-mode sky clear and space-black.
        app.add_systems(Update, update_rocket_sky_color);

        // Rocket-mode celestial proxies use SimulationTime directly, independent
        // of the wall-clock shared solar-map presentation.
        app.add_systems(Update, update_rocket_planets.after(recenter_render_origin));
        // Day/night cycle: rotates the sun around the planet as simulation time advances.
        app.add_systems(Update, update_sun_day_night_cycle);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vulkan_compute_is_not_required_for_rocket_presentation() {
        assert!(!shared_solar_presentation_requires_vulkan(true));
        assert!(shared_solar_presentation_requires_vulkan(false));
    }

    #[test]
    fn atmosphere_recovery_and_guidance_schedule_initializes() {
        let mut app = App::new();
        app.insert_resource(SimulationTime::default());
        app.configure_sets(
            FixedUpdate,
            (
                RocketSet::Atmosphere,
                RocketSet::Recovery,
                RocketSet::Guidance,
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            (
                refresh_flight_conditions.in_set(RocketSet::Atmosphere),
                station_keep_drone_ships.in_set(RocketSet::Recovery),
                update_drone_ship_landing_targets.in_set(RocketSet::Recovery),
                guidance_system.in_set(RocketSet::Guidance),
            )
                .chain(),
        );

        app.world_mut().run_schedule(FixedUpdate);
    }
}
