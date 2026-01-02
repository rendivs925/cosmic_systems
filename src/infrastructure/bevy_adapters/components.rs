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
    pub material: Handle<StandardMaterial>,
    pub base_color: Color,
    pub tilt: Vec2,
    pub wobble_speed: f32,
    pub wobble_amount: f32,
    pub spin_speed: f32,
    pub phase: f32,
}

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

// Notification types for user feedback
#[derive(Clone, Debug)]
pub enum NotificationType {
    Success,
    Error,
    Info,
}

// Individual notification message
#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub notification_type: NotificationType,
    pub created_at: f32, // Time in seconds
    pub duration: f32,   // How long to display (seconds)
}

// Resource to manage notification queue
#[derive(Resource)]
pub struct NotificationQueue {
    pub notifications: Vec<Notification>,
    pub hide_for_screenshot: bool, // Temporarily hide notifications during screenshot
}

// Resource to track pending screenshot capture
#[derive(Resource)]
pub struct ScreenshotState {
    pub pending: bool, // Screenshot requested, will capture next frame
}

// Resource to track if UI is currently under the cursor
#[derive(Resource, Default)]
pub struct UiPointerState {
    pub is_over_ui: bool,
}

#[derive(Resource)]
pub struct CameraInputState {
    pub last_input_time: f32,
    pub suppress_auto_inspect_for: Option<Entity>,
    pub last_selected_entity: Option<Entity>,
}

impl Default for CameraInputState {
    fn default() -> Self {
        Self {
            last_input_time: -1000.0,
            suppress_auto_inspect_for: None,
            last_selected_entity: None,
        }
    }
}

// Performance monitoring and quality adjustment
#[derive(Resource)]
pub struct PerformanceStats {
    pub frame_time: f32,             // Current frame time in milliseconds
    pub fps: f32,                    // Current FPS
    pub average_frame_time: f32,     // Rolling average frame time
    pub frame_count: u64,            // Total frames rendered
    pub quality_level: QualityLevel, // Current quality setting
    pub target_fps: f32,             // Target FPS for quality adjustment
    pub adaptive_enabled: bool,      // Whether automatic quality adjustment is enabled
}

// Quality levels for automatic adjustment
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityLevel {
    Ultra,   // Highest quality, no performance optimizations
    High,    // High quality with minimal optimizations
    Medium,  // Balanced quality and performance
    Low,     // Lower quality for better performance
    Minimal, // Minimum quality for maximum performance
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            frame_time: 16.67, // Assume 60 FPS initially
            fps: 60.0,
            average_frame_time: 16.67,
            frame_count: 0,
            quality_level: QualityLevel::High,
            target_fps: 60.0,
            adaptive_enabled: true,
        }
    }
}
