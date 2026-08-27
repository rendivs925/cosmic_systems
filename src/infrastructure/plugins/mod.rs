// Application mode composition plugins.
//
// These plugins move the system-registration code formerly living in
// `src/main.rs` into composable Bevy plugins. Each mode is composed from the
// shared solar-system plugin plus mode-specific plugins, keeping mode-specific
// behavior isolated per AGENTS.md sections 5, 35, and 66.

use bevy::app::App;
use bevy::prelude::*;

use crate::application::craft_startup::spawn_craft;
use crate::application::craft_startup::spawn_craft_model;
use crate::application::craft_startup::spawn_craft_ui;
use crate::application::gyro_startup::setup_gyro;
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::application::rocket_spawning::spawn_rockets;
use crate::application::solar_system_startup::setup_space;
use crate::domain::events::{
    CommsBlackoutEvent, FairingSeparatedEvent, RelaunchRequested, SplashdownDetectedEvent,
    StageSeparatedEvent,
};
use crate::domain::services::simulation_time::{
    advance_fixed_simulation_time, advance_real_time, handle_time_acceleration_input,
    sync_fixed_timestep, SimulationTime,
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
    handle_craft_input, update_craft_camera, update_craft_physics,
};
use crate::infrastructure::bevy_adapters::craft_ui::update_craft_ui;
use crate::infrastructure::bevy_adapters::education_systems::register_education_systems;
use crate::infrastructure::bevy_adapters::gyroscope_systems::{
    handle_input, update_gyroscopes, update_thrust,
};
use crate::infrastructure::bevy_adapters::performance_systems::cap_fixed_overstep;
use crate::infrastructure::bevy_adapters::rocket_camera_systems::{
    handle_free_camera_input, handle_rocket_camera_input, setup_rocket_camera_and_origin,
    setup_rocket_camera_controller, update_rocket_camera, update_rocket_camera_projection,
};
use crate::infrastructure::bevy_adapters::rocket_contact::{
    advance_topple, deploy_landing_legs, resolve_ground_contact,
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
use crate::infrastructure::bevy_adapters::rocket_flight_conditions::refresh_flight_conditions;
use crate::infrastructure::bevy_adapters::rocket_gravity_orbit::{
    update_orbital_elements, update_rocket_gravity,
};
use crate::infrastructure::bevy_adapters::rocket_hud::{
    spawn_rocket_hud_system, update_rocket_hud_system,
};
use crate::infrastructure::bevy_adapters::rocket_lifecycle::{
    apply_relaunch_requests, handle_relaunch_input_system, handle_rocket_launch_input,
};
use crate::infrastructure::bevy_adapters::rocket_planet::{
    isolate_rocket_presentation, setup_rocket_planets, update_rocket_planets, RocketBoundPlanet,
};
use crate::infrastructure::bevy_adapters::rocket_presentation::{
    capture_render_state, interpolate_render_transform,
};
use crate::infrastructure::bevy_adapters::rocket_propulsion::{
    propulsion_consumption, propulsion_gimbal, propulsion_staging, propulsion_thrust,
};
use crate::infrastructure::bevy_adapters::rocket_separation::{
    check_fairing_separation, spent_stage_aerodynamics, update_spent_stage_lifecycle,
};
use crate::infrastructure::bevy_adapters::rocket_systems::{
    guidance_system, setup_rocket_earth_sphere, setup_rocket_sky_color, setup_rocket_sun_light,
    update_rocket_earth_sphere, update_rocket_sky_color, update_sun_day_night_cycle,
};
use crate::infrastructure::bevy_adapters::rocket_telemetry::{
    compute_rocket_telemetry_system, handle_flight_recorder_export_system,
    handle_flight_recorder_input_system, record_flight_data_system, rocket_event_feed_system,
    RocketEventFeed,
};
use crate::infrastructure::bevy_adapters::systems::*;
use crate::infrastructure::bevy_adapters::terrain_render::{
    recenter_render_origin, TerrainRenderConfig, TerrainRenderPlugin,
};
use crate::infrastructure::bevy_adapters::terrain_streaming::{
    stream_terrain_patches, TerrainStreamingResource,
};
use crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState;
#[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
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

        // Physics systems run on Update for display-synced updates (eliminates FixedUpdate jitter)
        app.add_systems(
            Update,
            (
                update_planet_positions,
                update_planet_rotations,
                update_moon_orbit_positions,
            )
                .chain(),
        );

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

        // Performance and quality systems
        app.add_systems(Update, update_performance_stats);
        app.add_systems(Update, log_performance_stats);
        app.add_systems(Update, adaptive_quality_system);

        // Vulkan compute (native only)
        #[cfg(all(not(target_arch = "wasm32"), feature = "ash"))]
        app.add_systems(Update, init_vulkan_solver);

        // Screenshot and recording
        app.add_systems(Update, take_pending_screenshot);
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
        app.add_systems(Startup, spawn_orbital_planes);
        app.add_systems(Startup, spawn_eccentricity_markers);

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
            Update,
            (
                update_craft_physics,
                handle_craft_input,
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
        app.add_systems(Startup, spawn_rockets_system.after(setup_space));

        // Rocket telemetry resource for HUD and flight log.
        app.init_resource::<RocketTelemetry>();
        app.init_resource::<RocketEventFeed>();

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

        // Rocket camera resources.
        app.init_resource::<RocketCameraMode>();
        app.init_resource::<RocketCameraConfig>();
        // Rocket mode flag for conditional shared systems.
        app.init_resource::<RocketMode>();
        // Rocket planet system resource.
        app.init_resource::<RocketBoundPlanet>();

        // Simulation time resource for fixed-timestep physics and time acceleration.
        app.insert_resource(SimulationTime::default());

        // Cube-sphere terrain streaming around the rocket.
        app.insert_resource(TerrainStreamingResource::default());
        app.add_systems(Update, stream_terrain_patches);

        // Terrain rendering plugin (spawns meshes from streaming patches).
        app.add_plugins(TerrainRenderPlugin);

        // Rocket debug visualization plugin.
        app.add_plugins(RocketDebugPlugin);

        // Wall-clock time is updated per rendered frame; simulation time is
        // advanced only by completed fixed physics ticks below.
        app.add_systems(Update, advance_real_time);

        // Cap fixed timestep overstep (runs in Update).
        app.add_systems(Update, cap_fixed_overstep);

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
        app.add_systems(Update, handle_flight_recorder_input_system);

        // Flight-recorder CSV export (F11, runs in Update).
        app.add_systems(Update, handle_flight_recorder_export_system);

        // Relaunch input (runs in Update; mutation happens in FixedUpdate).
        app.add_systems(Update, handle_relaunch_input_system);

        // Time acceleration adjusts fixed-update frequency while every physics
        // tick keeps the bounded SimulationTime timestep.
        app.add_systems(
            Update,
            (handle_time_acceleration_input, sync_fixed_timestep).chain(),
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
                RocketSet::PropulsionConsumption,
                RocketSet::PropulsionStaging,
                RocketSet::AccumulateForces,
                RocketSet::Integrate,
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
                guidance_system.in_set(RocketSet::Guidance),
                apply_relaunch_requests
                    .in_set(RocketSet::Guidance)
                    .before(guidance_system),
                control_system.in_set(RocketSet::Control),
                actuation_system.in_set(RocketSet::Actuation),
                update_rocket_gravity.in_set(RocketSet::Gravity),
                refresh_flight_conditions.in_set(RocketSet::Atmosphere),
                spent_stage_aerodynamics.in_set(RocketSet::SpentStage),
                update_spent_stage_lifecycle.in_set(RocketSet::SpentStage),
                check_fairing_separation.in_set(RocketSet::SpentStage),
                compute_heating.in_set(RocketSet::EntryPhysics),
                compute_ablation.in_set(RocketSet::EntryPhysics),
                compute_plasma_blackout.in_set(RocketSet::EntryPhysics),
                compute_parachute_forces.in_set(RocketSet::EntryPhysics),
                compute_retro_propulsion.in_set(RocketSet::EntryPhysics),
                deploy_landing_legs.in_set(RocketSet::EntryPhysics),
            ),
        );
        app.add_systems(
            FixedUpdate,
            (
                aerodynamic_forces.in_set(RocketSet::AeroForces),
                aerodynamic_torque.in_set(RocketSet::AeroTorque),
                propulsion_thrust.in_set(RocketSet::PropulsionThrust),
                propulsion_gimbal.in_set(RocketSet::PropulsionGimbal),
                propulsion_consumption.in_set(RocketSet::PropulsionConsumption),
                propulsion_staging.in_set(RocketSet::PropulsionStaging),
                accumulate_forces.in_set(RocketSet::AccumulateForces),
                integrate_6dof.in_set(RocketSet::Integrate),
                advance_fixed_simulation_time.in_set(RocketSet::AdvanceTime),
                update_orbital_elements.in_set(RocketSet::OrbitalElements),
                resolve_ground_contact.in_set(RocketSet::GroundContact),
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
            ),
        );

        // Rocket-specific startup: camera controller, sun light, Earth sphere, sky color.
        // Must run AFTER setup_space (spawns camera + planets) and spawn_rockets_system
        // (spawns rocket) so the render origin is set to the rocket's physical position
        // and the camera is framed on the already-spawned camera entity.
        app.add_systems(
            Startup,
            (
                isolate_rocket_presentation,
                setup_rocket_camera_and_origin,
                setup_rocket_camera_controller,
                setup_rocket_sun_light,
                setup_rocket_planets,
                // Earth sphere disabled: 6371km radius sphere doesn't work in
                // flight frame where camera is at rocket position (origin).
                // Terrain patches provide the local spherical terrain.
                // setup_rocket_earth_sphere,
                setup_rocket_sky_color,
            )
                .chain()
                .after(setup_space)
                .after(spawn_rockets_system),
        );

        // Pre-launch hold: Space to launch.
        app.add_systems(Update, handle_rocket_launch_input);

        // Keep the rocket-mode sky clear and space-black.
        app.add_systems(Update, update_rocket_sky_color);

        // True-scale Earth sphere follows render origin.
        app.add_systems(Update, update_rocket_earth_sphere);

        // Rocket-mode planets (bound planet, moons, Sun) in flight units with real textures.
        // Runs after solar system planet positions are updated.
        app.add_systems(
            Update,
            update_rocket_planets
                .after(update_planet_positions)
                .after(recenter_render_origin),
        );
        // Day/night cycle: rotates the sun around the planet as simulation time advances.
        app.add_systems(Update, update_sun_day_night_cycle);
    }
}
