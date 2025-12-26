use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use bevy::prelude::*;

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
}

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
}

// Component for selectable objects (planets, etc.)
#[derive(Component)]
pub struct Selectable {
    pub name: String,
    pub selected: bool,
}

// Resource to track currently selected planet
#[derive(Resource)]
pub struct SelectedPlanet {
    pub entity: Option<Entity>,
    pub name: Option<String>,
}

// Resource to track hovered planet for information display
#[derive(Resource)]
pub struct HoveredPlanet {
    pub name: Option<String>,
    pub info: Option<String>,
}
