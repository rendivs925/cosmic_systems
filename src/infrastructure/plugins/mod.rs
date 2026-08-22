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
use crate::application::rocket_spawning::spawn_rockets;
use crate::application::solar_system_startup::setup_space;
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
use crate::infrastructure::bevy_adapters::rocket_systems::{
    accumulate_forces, actuation_system, aerodynamic_forces, aerodynamic_torque,
    atmosphere_properties, compute_ablation, compute_heating, compute_parachute_forces,
    compute_plasma_blackout, compute_retro_propulsion, control_system, guidance_system,
    integrate_6dof, propulsion_consumption, propulsion_gimbal, propulsion_staging,
    propulsion_thrust, sync_render_transform, update_rocket_gravity,
    update_rocket_terrain_interaction,
};
use crate::infrastructure::bevy_adapters::systems::*;
use crate::infrastructure::bevy_adapters::terrain_render::{
    TerrainRenderConfig, TerrainRenderPlugin,
};
use crate::infrastructure::bevy_adapters::terrain_streaming::{
    stream_terrain_patches, TerrainStreamingResource,
};
use crate::infrastructure::bevy_adapters::terrain_systems::{
    generate_terrain_mesh, initialize_terrain_lod, update_terrain_lod,
    update_terrain_synchronization, update_terrain_visibility,
};
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

        // Terrain systems
        app.add_systems(Update, update_terrain_visibility);
        app.add_systems(Update, generate_terrain_mesh);
        app.add_systems(Update, initialize_terrain_lod);
        app.add_systems(Update, update_terrain_lod);
        // Terrain orbital synchronization - high priority
        app.add_systems(Update, update_terrain_synchronization);

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
) {
    spawn_rockets(&mut commands, &mut meshes, &mut materials);
}

/// Rocket flight mode: composes the shared solar-system world and registers
/// rocket-only systems.
pub struct RocketModePlugin;

impl Plugin for RocketModePlugin {
    fn build(&self, app: &mut App) {
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Startup, spawn_rockets_system);

        // Terrain rendering configuration.
        app.init_resource::<TerrainRenderConfig>();

        // Entry physics configuration.
        app.init_resource::<EntryPhysicsConfig>();

        // Cube-sphere terrain streaming around the rocket.
        app.insert_resource(TerrainStreamingResource::default());
        app.add_systems(Update, stream_terrain_patches);

        // Terrain rendering plugin (spawns meshes from streaming patches).
        app.add_plugins(TerrainRenderPlugin);

        app.configure_sets(
            Update,
            (RocketSet::Guidance
                .before(RocketSet::Control)
                .before(RocketSet::Actuation)
                .before(RocketSet::Gravity)
                .before(RocketSet::TerrainInteraction)
                .before(RocketSet::Atmosphere)
                .before(RocketSet::EntryPhysics)
                .before(RocketSet::AeroForces)
                .before(RocketSet::AeroTorque)
                .before(RocketSet::PropulsionThrust)
                .before(RocketSet::PropulsionGimbal)
                .before(RocketSet::PropulsionConsumption)
                .before(RocketSet::PropulsionStaging)
                .before(RocketSet::AccumulateForces)
                .before(RocketSet::Integrate)
                .before(RocketSet::SyncRender),),
        );

        app.add_systems(
            Update,
            (
                guidance_system.in_set(RocketSet::Guidance),
                control_system.in_set(RocketSet::Control),
                actuation_system.in_set(RocketSet::Actuation),
                update_rocket_gravity.in_set(RocketSet::Gravity),
                update_rocket_terrain_interaction.in_set(RocketSet::TerrainInteraction),
                atmosphere_properties.in_set(RocketSet::Atmosphere),
                compute_heating.in_set(RocketSet::EntryPhysics),
                compute_ablation.in_set(RocketSet::EntryPhysics),
                compute_plasma_blackout.in_set(RocketSet::EntryPhysics),
                compute_parachute_forces.in_set(RocketSet::EntryPhysics),
                compute_retro_propulsion.in_set(RocketSet::EntryPhysics),
                aerodynamic_forces.in_set(RocketSet::AeroForces),
                aerodynamic_torque.in_set(RocketSet::AeroTorque),
                propulsion_thrust.in_set(RocketSet::PropulsionThrust),
                propulsion_gimbal.in_set(RocketSet::PropulsionGimbal),
                propulsion_consumption.in_set(RocketSet::PropulsionConsumption),
                propulsion_staging.in_set(RocketSet::PropulsionStaging),
                accumulate_forces.in_set(RocketSet::AccumulateForces),
                integrate_6dof.in_set(RocketSet::Integrate),
                sync_render_transform.in_set(RocketSet::SyncRender),
            )
                .chain(),
        );
    }
}
