use crate::components::rocket::*;
use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::{BodyClass, Planet};
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::atmosphere::atmosphere_for;
use crate::domain::services::atmosphere::AtmosphereSource;
use crate::domain::services::physics_orbital::OrbitShape;
use crate::domain::services::rocket_dynamics::RocketDynamicsState;
use crate::domain::services::terrain_source::{terrain_source_for, TerrainSource};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
#[cfg(feature = "dem")]
use std::path::Path;
use std::sync::Arc;

pub use crate::components::rocket::*;
pub use crate::domain::entities::rocket::RocketMissionState;

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

impl PlanetComponent {
    /// Typed rocket-flight boundary over the catalog's presentation-oriented
    /// body name. The celestial registry remains configurable and string-backed.
    pub fn matches_body(&self, body_id: &CelestialBodyId) -> bool {
        self.domain_planet.name == body_id.as_str()
    }
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
    /// Origin used when the heliocentric ribbon was last projected to f32 mesh vertices.
    pub mesh_origin_units: DVec3,
}

/// Authoritative solar-map position in f64 display units. `Transform` is only
/// the camera-relative rendering projection of this state.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct SolarMapPosition(pub DVec3);

/// Solar-map render origin in display units. This is intentionally separate
/// from rocket `RenderOrigin`, whose coordinates are physical meters.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct SolarMapRenderOrigin {
    pub position_units: DVec3,
}

/// Presentation light whose source is fixed at the Sun in solar-map coordinates.
#[derive(Component)]
pub struct SolarMapLight;

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

    #[cfg(feature = "dem")]
    pub fn with_srtm_directory(name: &str, directory: Option<&Path>) -> Self {
        Self {
            source: crate::domain::services::terrain_source::terrain_source_for_with_srtm_dir(
                name, directory,
            ),
        }
    }
}

/// Per-planet atmosphere model. Attached to planet entities.
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

// Component for launch sites (terrain markers)
#[derive(Component)]
pub struct LaunchSiteComponent {
    pub name: String,
    pub planet_entity: Entity,
    pub position: Vec3, // Local position on terrain
    pub launch_pad_model: Option<Handle<Scene>>,
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
    /// Map the per-body entry config into the pure-domain parachute
    /// configuration. Values live here once; the domain state machine
    /// consumes this struct so no constants are duplicated.
    pub fn parachute_config(&self) -> crate::domain::services::entry_physics::ParachuteConfig {
        use crate::domain::services::entry_physics::{CanopyConfig, ParachuteConfig};
        ParachuteConfig {
            drogue: CanopyConfig {
                deploy_mach: self.drogue_deploy_mach,
                deploy_altitude_m: self.drogue_deploy_altitude_m,
                reef_time_s: self.drogue_reef_time_s,
                reef_cd: self.drogue_reef_cd,
                full_cd: self.drogue_full_cd,
                reference_area_m2: self.drogue_reference_area_m2,
            },
            main: CanopyConfig {
                deploy_mach: 0.0,
                deploy_altitude_m: self.main_deploy_altitude_m,
                reef_time_s: self.main_reef_time_s,
                reef_cd: self.main_reef_cd,
                full_cd: self.main_full_cd,
                reference_area_m2: self.main_reference_area_m2,
            },
        }
    }

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
