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
    pub yaw: f32,
    pub pitch: f32,
    pub horizontal_velocity: Vec3,
}

impl CraftComponent {
    pub fn saucer() -> Self {
        Self {
            craft: Craft::saucer(),
            physics: CraftPhysicsState::default(),
            dc_field: 0.0,
            pulse_resonance: 0.0,
            camera_mode: CraftCameraMode::External,
            yaw: 0.0,
            pitch: 0.0,
            horizontal_velocity: Vec3::ZERO,
        }
    }
}

#[derive(Component)]
pub struct CraftVisual {
    pub kind: CraftKind,
    pub core_pulse_phase: f32,
    pub ring_rotation: f32,
    pub dome_base_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CraftCameraMode {
    External,
    FirstPerson,
}

#[derive(Resource)]
pub struct CraftControlState {
    pub dc_target: f32,
    pub pulse_target: f32,
    pub dc_current: f32,
    pub pulse_current: f32,
}

impl Default for CraftControlState {
    fn default() -> Self {
        Self {
            dc_target: 0.38,
            pulse_target: 0.0,
            dc_current: 0.38,
            pulse_current: 0.0,
        }
    }
}

#[derive(Component)]
pub struct CraftCameraTag;

#[derive(Component)]
pub struct CraftUiRoot;

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
