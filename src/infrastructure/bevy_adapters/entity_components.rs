use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::{BodyClass, Planet};
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::atmosphere::atmosphere_for;
use crate::domain::services::atmosphere::AtmosphereSource;
use crate::domain::services::physics_orbital::OrbitShape;
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::domain::services::terrain_source::{terrain_source_for, TerrainSource};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use std::sync::Arc;

pub use crate::domain::entities::rocket::RocketMissionState;

/// Types of launch sites with different terrain characteristics
#[derive(Debug, Clone, Copy, PartialEq, Component)]
pub enum LaunchSiteType {
    KennedySpaceCenter,
    RtlsLandingPad,
    DroneShip,
    LunarLanding,
}

// Component for gyroscope entities
#[derive(Component)]
pub struct GyroscopeComponent {
    pub domain_gyro: Gyroscope,
}

// Component for thrust visualization (arrow entity)
#[derive(Component)]
pub struct ThrustArrow;

// Component for planet entities
#[derive(Component)]
pub struct PlanetComponent {
    pub domain_planet: Planet,
    pub material: Handle<StandardMaterial>,
    pub has_texture: bool,
    pub base_reflectance: f32,
    pub base_roughness: f32,
}

// Component for orbital path visualization
#[derive(Component)]
pub struct OrbitComponent {
    pub radius: f32,
    pub planet_entity: Entity,
    pub material: Handle<StandardMaterial>,
    pub base_color: Color,
    pub body_class: BodyClass,
    pub orbit_shape: OrbitShape,
    pub thickness: f32,
    pub segments: usize,
    pub tilt: Vec2,
    pub wobble_speed: f32,
    pub wobble_amount: f32,
    pub spin_speed: f32,
    pub phase: f32,
    pub distance_rank: f32,
}

// Component for orbital plane visualization with inclination-based effects
#[derive(Component)]
pub struct OrbitalPlaneComponent {
    pub planet_entity: Entity,
    pub inclination_rad: f32,
    pub ascending_node_rad: f32,
    pub semi_major_axis: f32,
    pub eccentricity: f32,
    pub material: Handle<StandardMaterial>,
    pub opacity: f32,
}

// Component for apoapsis/periapsis markers showing orbit eccentricity
#[derive(Component)]
pub struct EccentricityMarkersComponent {
    pub planet_entity: Entity,
    pub apoapsis_position: Vec3,
    pub periapsis_position: Vec3,
    pub apoapsis_material: Handle<StandardMaterial>,
    pub periapsis_material: Handle<StandardMaterial>,
    pub eccentricity: f32,
}

// Component for velocity trail particle system
#[derive(Component)]
pub struct VelocityTrailComponent {
    pub planet_entity: Entity,
    pub trail_length: usize,
    pub particle_entities: Vec<Entity>,
    pub last_positions: Vec<Vec3>,
    pub update_interval: f32,
    pub trail_timer: f32,
}

#[derive(Component)]
pub struct Starfield;

// Component for the orbital position tracker marker (small dot on orbit at planet's position)
#[derive(Component)]
pub struct PositionTracker {
    pub planet_entity: Entity,
    pub planet_name: String,
}

// Marker spawned on orbit entities after their position tracker is created
#[derive(Component)]
pub struct TrackerSpawned;

// Marker component for moon orbits (orbits that need to follow their parent planet)
#[derive(Component)]
pub struct MoonOrbit;

// Component for cloud layers to control rotation speed
#[derive(Component)]
pub struct CloudLayer {
    pub rotation_period_hours: f32,
}

// Camera control modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CameraMode {
    FreeFlight,     // Free movement in 3D space
    Orbit,          // Orbital view around solar system center
    FollowPlanet,   // Follow a specific planet
    ApproachPlanet, // Approach and potentially "land" on a planet
    TerrainView,    // Ground-level terrain exploration view
}

// Component for camera controller
#[derive(Component)]
pub struct CameraController {
    pub mode: CameraMode,
    pub speed: f32,
    pub sensitivity: f32,
    pub velocity: Vec3,
    pub target_entity: Option<Entity>,
    pub orbit_distance: f32,
    pub orbit_angle: f32,
    pub acceleration: f32,            // Smooth acceleration
    pub deceleration: f32,            // Smooth deceleration
    pub adaptive_speed_enabled: bool, // Auto-adjust speed based on distance
    pub min_speed: f32,               // Minimum movement speed
    pub max_speed: f32,               // Maximum movement speed
    pub zoom_sensitivity: f32,        // Mouse wheel zoom sensitivity
}

// Component for selectable objects (planets, etc.)
#[derive(Component)]
pub struct Selectable {
    pub name: String,
    pub selected: bool,
}

// Component for terrain patches (localized high-detail terrain)
#[derive(Component)]
pub struct TerrainComponent {
    pub planet_entity: Entity,
    pub planet_name: String,
    pub position_offset: Vec3, // Offset from planet center
    pub scale: f32,            // Terrain scale factor
    pub heightmap: Handle<Image>,
    pub surface_texture: Handle<Image>,
    pub normal_texture: Handle<Image>, // Normal map for surface details
    pub size_km: f32,                  // Terrain patch size in km
    pub resolution: u32,               // Heightmap resolution
    pub launch_site_type: LaunchSiteType, // Type of launch site for terrain generation
}

// Component for rocket entities
#[derive(Component)]
pub struct RocketComponent {
    /// Authoritative 6-DOF physical state (f64, planet-centered inertial meters).
    pub dynamics: RocketDynamicsState,
    /// Net force accumulator (world/planet-inertial frame), consumed by integration.
    pub force_accum_n: DVec3,
    /// Net torque accumulator (body frame), consumed by integration.
    pub torque_accum_nm: DVec3,
    /// Vehicle geometry (radius/height in meters) for the inertia model.
    pub radius_m: f32,
    pub height_m: f32,
    // ------------------------------------------------------------------
    // Compatible facade fields synced from the f64 dynamics state each tick.
    // Existing consumers that have not yet migrated to the f64 state read
    // these; they are never the source of truth for motion.
    // ------------------------------------------------------------------
    pub position: Vec3,
    pub velocity: Vec3,
    pub orientation: Quat,
    pub angular_velocity: Vec3,
    pub mass: f32,
    pub dry_mass_kg: f32,
    pub fuel_mass: f32,
    pub thrust: Vec3,
    pub mission_state: RocketMissionState,
}

/// Runtime propulsion state: the vehicle definition plus throttle, gimbal
/// commands, active stage, and per-stage remaining propellant. Propulsion
/// systems consume this and feed the 6-DOF accumulators.
#[derive(Component, Debug, Clone)]
pub struct RocketPropulsion {
    pub vehicle: Rocket,
    pub active_stage: usize,
    pub propellant_remaining_kg: Vec<f32>,
    pub throttle: f32,
    pub gimbal_pitch_rad: f32,
    pub gimbal_yaw_rad: f32,
}

/// Binds a rocket to its dominant-body frame parent. Gravity is computed from
/// the planet with this name (resolved against `PlanetComponent`), per the
/// dominant-body selection rule in the gravity design.
#[derive(Component, Debug, Clone)]
pub struct RocketPlanetBinding {
    pub planet_name: String,
}

/// Authoritative gravitational acceleration (m/s², f64) acting on a vehicle.
/// Computed each tick by the gravity system and stored for the 6-DOF
/// integration to consume.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GravityAcceleration {
    pub value: DVec3,
}

/// Per-planet atmosphere source (AGENTS.md section 19). Attached to planet
/// entities; the shared single implementation for all atmosphere consumers.
#[derive(Component, Debug, Clone)]
pub struct PlanetAtmosphere {
    pub source: Arc<dyn AtmosphereSource>,
}

impl PlanetAtmosphere {
    pub fn default_for(name: &str) -> Self {
        Self {
            source: atmosphere_for(name),
        }
    }
}

/// Per-planet terrain height source (AGENTS.md sections 20-21). Attached to
/// planet entities; render mesh and collision query the same source.
#[derive(Component, Debug, Clone)]
pub struct PlanetTerrain {
    pub source: Arc<dyn TerrainSource>,
}

impl PlanetTerrain {
    pub fn default_for(name: &str) -> Self {
        Self {
            source: terrain_source_for(name),
        }
    }
}

/// Cached terrain collision state for a vehicle, computed each tick by the
/// rocket interaction system for telemetry/debug.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TerrainCollisionState {
    pub radar_altitude_m: f64,
    pub slope_deg: f64,
    pub ground_contact: crate::domain::services::terrain_collision::GroundContact,
}

/// Cached atmosphere state at the vehicle's current altitude, computed by the
/// `atmosphere_properties` system before aero and propulsion consume it.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AtmosphereState {
    pub altitude_m: f64,
    pub temperature_k: f64,
    pub pressure_pa: f64,
    pub density_kg_m3: f64,
    pub speed_of_sound_mps: f64,
}

/// Aerodynamic force computed by `aerodynamic_forces`, consumed by
/// `aerodynamic_torque`. Body frame; fed to the 6-DOF accumulators.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AerodynamicForces {
    pub force_body: DVec3,
    pub center_of_pressure_body: DVec3,
}

/// Running maximum dynamic pressure (Max Q) reached during flight, Pa.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct MaxQTracker {
    pub max_q_pa: f64,
}

/// The flight-computer command interface between the guidance, control,
/// actuation, and physics layers (AGENTS.md section 18). Each layer writes its
/// outputs here; no layer writes the rocket's motion directly.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct RocketCommands {
    /// Guidance output: the target body→world attitude.
    pub target_attitude: DQuat,
    /// Control output: commanded throttle (0..1).
    pub throttle_cmd: f32,
    /// Control output: commanded gimbal pitch deflection, radians.
    pub gimbal_pitch_cmd_rad: f32,
    /// Control output: commanded gimbal yaw deflection, radians.
    pub gimbal_yaw_cmd_rad: f32,
    /// Control output: commanded RCS torque, body frame, N·m.
    pub rcs_torque_cmd_body: DVec3,
}

/// Autopilot configuration and state for the rocket: PID gains, integral
/// accumulation, the ascent guidance profile, and actuator limits.
#[derive(Component, Debug, Clone, Default)]
pub struct RocketAutopilot {
    pub gains: crate::domain::services::control::PidGains,
    pub integral: DVec3,
    pub ascent_profile: crate::domain::services::guidance::AscentGuidanceProfile,
    pub actuation: crate::domain::services::actuation::ActuationLimits,
}

// Component for launch sites (terrain markers)
#[derive(Component)]
pub struct LaunchSiteComponent {
    pub name: String,
    pub planet_entity: Entity,
    pub position: Vec3, // Local position on terrain
    pub launch_pad_model: Option<Handle<Scene>>,
}

/// Cached thermal state at the vehicle's current trajectory point, computed by
/// the `compute_heating` system for ablation and telemetry.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ThermalState {
    pub convective_heat_flux_w_m2: f64,
    pub radiative_heat_flux_w_m2: f64,
    pub total_heat_flux_w_m2: f64,
    pub wall_temperature_k: f64,
    pub stagnation_point_heat_flux_w_m2: f64,
}

/// Ablation state tracking TPS recession and mass loss from aerothermal heating.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct AblationState {
    pub cumulative_heat_load_j_m2: f64,
    pub recession_depth_m: f64,
    pub nose_radius_m: f64,
    pub mass_loss_kg: f64,
    pub tps_thickness_remaining_m: f64,
}

/// Parachute deployment state for drogue and main parachutes.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct ParachuteState {
    pub drogue_deployed: bool,
    pub drogue_reefed: bool,
    pub drogue_fully_inflated: bool,
    pub drogue_timer_s: f64,
    pub main_deployed: bool,
    pub main_reefed: bool,
    pub main_fully_inflated: bool,
    pub main_timer_s: f64,
    pub canopy_attach_point_body: DVec3,
    pub current_cd: f64,
    pub reference_area_m2: f64,
}

/// Configuration for entry physics per celestial body.
#[derive(Resource, Debug, Clone)]
pub struct EntryPhysicsConfig {
    pub convective_coefficient: f64,
    pub radiative_coefficient: f64,
    pub nose_radius_initial_m: f64,
    pub tps_density_kg_m3: f64,
    pub heat_of_ablation_j_kg: f64,
    pub tps_initial_thickness_m: f64,
    pub comms_frequency_hz: f64,
    pub critical_electron_density_m3: f64,
    pub drogue_deploy_mach: f64,
    pub drogue_deploy_altitude_m: f64,
    pub drogue_reef_time_s: f64,
    pub drogue_reef_cd: f64,
    pub drogue_full_cd: f64,
    pub drogue_reference_area_m2: f64,
    pub main_deploy_altitude_m: f64,
    pub main_reef_time_s: f64,
    pub main_reef_cd: f64,
    pub main_full_cd: f64,
    pub main_reference_area_m2: f64,
    pub retro_propulsion_enabled: bool,
    pub retro_propulsion_mach_threshold: f64,
    pub base_pressure_coefficient: f64,
}

impl Default for EntryPhysicsConfig {
    fn default() -> Self {
        Self::for_body("Earth")
    }
}

impl EntryPhysicsConfig {
    pub fn for_body(name: &str) -> Self {
        match name {
            "Earth" => Self {
                convective_coefficient: 1.83e-4, // Sutton-Graves k for Earth
                radiative_coefficient: 1.0e-10,  // Tauber-Sutton
                nose_radius_initial_m: 2.5,
                tps_density_kg_m3: 1500.0,
                heat_of_ablation_j_kg: 1.5e7,
                tps_initial_thickness_m: 0.05,
                comms_frequency_hz: 2.3e9,
                critical_electron_density_m3: 6.6e16,
                drogue_deploy_mach: 2.5,
                drogue_deploy_altitude_m: 15_000.0,
                drogue_reef_time_s: 5.0,
                drogue_reef_cd: 0.5,
                drogue_full_cd: 1.2,
                drogue_reference_area_m2: 20.0,
                main_deploy_altitude_m: 3_000.0,
                main_reef_time_s: 3.0,
                main_reef_cd: 0.8,
                main_full_cd: 2.2,
                main_reference_area_m2: 150.0,
                retro_propulsion_enabled: true,
                retro_propulsion_mach_threshold: 1.2,
                base_pressure_coefficient: 0.1,
            },
            "Mars" => Self {
                convective_coefficient: 1.5e-4,
                radiative_coefficient: 5.0e-11,
                nose_radius_initial_m: 2.5,
                tps_density_kg_m3: 1500.0,
                heat_of_ablation_j_kg: 1.5e7,
                tps_initial_thickness_m: 0.05,
                comms_frequency_hz: 2.3e9,
                critical_electron_density_m3: 6.6e16,
                drogue_deploy_mach: 2.0,
                drogue_deploy_altitude_m: 10_000.0,
                drogue_reef_time_s: 3.0,
                drogue_reef_cd: 0.5,
                drogue_full_cd: 1.2,
                drogue_reference_area_m2: 25.0,
                main_deploy_altitude_m: 2_000.0,
                main_reef_time_s: 2.0,
                main_reef_cd: 0.8,
                main_full_cd: 2.2,
                main_reference_area_m2: 200.0,
                retro_propulsion_enabled: true,
                retro_propulsion_mach_threshold: 1.5,
                base_pressure_coefficient: 0.08,
            },
            "Moon" => Self {
                convective_coefficient: 1.0e-4,
                radiative_coefficient: 1.0e-11,
                nose_radius_initial_m: 2.5,
                tps_density_kg_m3: 1500.0,
                heat_of_ablation_j_kg: 1.5e7,
                tps_initial_thickness_m: 0.05,
                comms_frequency_hz: 2.3e9,
                critical_electron_density_m3: 6.6e16,
                drogue_deploy_mach: 0.0, // No atmosphere
                drogue_deploy_altitude_m: 0.0,
                drogue_reef_time_s: 0.0,
                drogue_reef_cd: 0.0,
                drogue_full_cd: 0.0,
                drogue_reference_area_m2: 0.0,
                main_deploy_altitude_m: 0.0,
                main_reef_time_s: 0.0,
                main_reef_cd: 0.0,
                main_full_cd: 0.0,
                main_reference_area_m2: 0.0,
                retro_propulsion_enabled: true,
                retro_propulsion_mach_threshold: 0.0,
                base_pressure_coefficient: 0.0,
            },
            _ => Self::for_body("Earth"),
        }
    }
}
