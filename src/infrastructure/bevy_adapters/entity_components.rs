use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use bevy::prelude::*;

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
    pub tilt: Vec2,
    pub wobble_speed: f32,
    pub wobble_amount: f32,
    pub spin_speed: f32,
    pub phase: f32,
    pub distance_rank: f32, // 0.0 (closest to sun) to 1.0 (farthest) for hierarchy
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
    pub apoapsis_entity: Entity,
    pub periapsis_entity: Entity,
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
    pub position_offset: Vec3,     // Offset from planet center
    pub scale: f32,                // Terrain scale factor
    pub heightmap: Handle<Image>,
    pub surface_texture: Handle<Image>,
    pub normal_texture: Handle<Image>, // Normal map for surface details
    pub size_km: f32,              // Terrain patch size in km
    pub resolution: u32,           // Heightmap resolution
    pub launch_site_type: LaunchSiteType, // Type of launch site for terrain generation
}

// Component for rocket entities
#[derive(Component)]
pub struct RocketComponent {
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

// Component for launch sites (terrain markers)
#[derive(Component)]
pub struct LaunchSiteComponent {
    pub name: String,
    pub planet_entity: Entity,
    pub position: Vec3,  // Local position on terrain
    pub launch_pad_model: Option<Handle<Scene>>,
}