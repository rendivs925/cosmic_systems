use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::{BodyClass, Planet};
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::atmosphere::atmosphere_for;
use crate::domain::services::atmosphere::AtmosphereSource;
use crate::domain::services::physics_orbital::OrbitShape;
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use bevy::math::DVec3;
use bevy::prelude::*;
use std::sync::Arc;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RocketMissionState {
    PreLaunch,
    Launch,
    Ascent,
    Orbit,
    Deorbit,
    Descent,
    Landing,
    Landed,
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

// Component for launch sites (terrain markers)
#[derive(Component)]
pub struct LaunchSiteComponent {
    pub name: String,
    pub planet_entity: Entity,
    pub position: Vec3, // Local position on terrain
    pub launch_pad_model: Option<Handle<Scene>>,
}
