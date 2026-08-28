use crate::domain::entities::craft::{Craft, CraftKind};
use crate::domain::services::craft_physics::CraftPhysicsState;
use bevy::prelude::*;

#[derive(Component)]
pub struct CraftComponent {
    pub craft: Craft,
    pub physics: CraftPhysicsState,
    pub dc_field: f32,
    pub pulse_resonance: f32,
    pub camera_mode: CraftCameraMode,
    /// Authoritative flight position. `Transform` is presentation-only.
    pub position: Vec3,
    /// Authoritative flight attitude. `Transform` is presentation-only.
    pub orientation: Quat,
    pub angular_velocity: Vec3,
    /// Input angular acceleration, integrated by fixed-step physics.
    pub angular_input: Vec3,
    pub linear_velocity: Vec3,
    pub move_input: Vec2,
    pub speed_mode: SpeedMode,
}

impl CraftComponent {
    pub fn saucer() -> Self {
        Self {
            craft: Craft::saucer(),
            physics: CraftPhysicsState::default(),
            dc_field: 0.0,
            pulse_resonance: 0.0,
            camera_mode: CraftCameraMode::Chase,
            position: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            angular_velocity: Vec3::ZERO,
            angular_input: Vec3::ZERO,
            linear_velocity: Vec3::ZERO,
            move_input: Vec2::ZERO,
            speed_mode: SpeedMode::Cruise,
        }
    }
}

#[derive(Component)]
pub struct CraftVisual {
    pub kind: CraftKind,
    pub core_pulse_phase: f32,
    pub ring_rotation: f32,
    pub dome_base_scale: f32,
    pub field_strength: f32,
    pub resonance_phase: f32,
    pub zpe_gain: f32,
    pub polarization_asymmetry: f32,
    pub bubble_radius: f32,
    pub wake_intensity: f32,
}

#[derive(Component)]
pub struct CraftBubble;

#[derive(Component)]
pub struct CraftRing;

#[derive(Component)]
pub struct CraftCoreGlow;

#[derive(Component)]
pub struct CraftLens;

#[derive(Component)]
pub struct CraftWake;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CraftCameraMode {
    Chase,
    Orbit,
    FirstPerson,
    Free,
    Cinematic,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedMode {
    Hover,
    Cruise,
    Sprint,
}

#[derive(Resource)]
pub struct CraftControlState {
    pub dc_target: f32,
    pub pulse_target: f32,
    pub dc_current: f32,
    pub pulse_current: f32,
    pub camera_index: usize,
}

#[derive(Resource, Default)]
pub struct CraftTravelTarget {
    pub entity: Option<Entity>,
    pub name: Option<String>,
}

impl Default for CraftControlState {
    fn default() -> Self {
        Self {
            dc_target: 0.38,
            pulse_target: 0.0,
            dc_current: 0.38,
            pulse_current: 0.0,
            camera_index: 0,
        }
    }
}

#[derive(Resource)]
pub struct CraftCameraState {
    pub target_distance: f32,
    pub zoom: f32,
    pub orbit_yaw: f32,
    pub orbit_pitch: f32,
    pub smooth_position: Vec3,
    pub smooth_look: Vec3,
    pub locked: bool,
}

impl Default for CraftCameraState {
    fn default() -> Self {
        Self {
            target_distance: 14.0,
            zoom: 1.0,
            orbit_yaw: 0.0,
            orbit_pitch: 0.4,
            smooth_position: Vec3::ZERO,
            smooth_look: Vec3::ZERO,
            locked: false,
        }
    }
}

#[derive(Component)]
pub struct CraftCameraTag;

#[derive(Component)]
pub struct CraftUiRoot;

#[derive(Resource)]
pub struct CraftEffectsEnabled(pub bool);

#[derive(Component)]
pub struct CraftPart {
    pub part_type: CraftPartType,
    pub material_handle: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CraftPartType {
    Core,
    Rim,
    Dome,
    Disc,
    InnerRing,
    Sphere,
}
