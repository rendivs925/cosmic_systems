//! Rocket camera mode, configuration, and controller components.

use bevy::math::Vec3;
use bevy::prelude::*;

/// Rocket camera mode for different viewing perspectives.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RocketCameraMode {
    #[default]
    Chase, // Third-person chase camera behind the rocket
    Cockpit, // First-person from rocket body
    Orbital, // Inertial frame showing orbital trajectory
    Surface, // Planet-relative for landing
    Free,    // Free camera (debug)
}

/// Configuration for rocket camera modes.
#[derive(Resource, Debug, Clone)]
pub struct RocketCameraConfig {
    /// Chase camera distance behind rocket
    pub chase_distance: f32,
    /// Chase camera height offset
    pub chase_height: f32,
    /// Chase camera pitch angle in radians; negative values aim toward the planet.
    pub chase_pitch: f32,
    /// Cockpit camera offset from rocket center
    pub cockpit_offset: Vec3,
    /// Orbital camera distance from rocket
    pub orbital_distance: f32,
    /// Orbital camera elevation angle
    pub orbital_elevation: f32,
    /// Surface camera distance from landing target
    pub surface_distance: f32,
    /// Surface camera height above terrain
    pub surface_height: f32,
    /// Transition speed between modes
    pub transition_speed: f32,
    /// Smoothing factor for camera movement
    pub smooth_factor: f32,
}

impl Default for RocketCameraConfig {
    fn default() -> Self {
        Self {
            // For 70m tall rocket: distance ~3x height, height ~0.7x height
            // so the whole rocket from engines to nose is framed.
            chase_distance: 220.0,
            chase_height: 50.0,
            chase_pitch: -0.3,
            cockpit_offset: Vec3::new(0.0, 5.0, 0.0),
            orbital_distance: 500.0,
            orbital_elevation: 0.5,
            surface_distance: 200.0,
            surface_height: 50.0,
            transition_speed: 2.0,
            smooth_factor: 0.1,
        }
    }
}

/// Rocket camera controller for managing camera state and transitions.
#[derive(Component, Debug, Clone)]
pub struct RocketCameraController {
    pub current_mode: RocketCameraMode,
    pub target_mode: RocketCameraMode,
    pub transition_progress: f32,
    /// Camera pose in the target mode's presentation frame at the start of a
    /// mode change. Vehicle-attached modes use the body frame; planet-relative
    /// modes use the render frame so contact attitude cannot rotate the camera.
    pub transition_start_pose: Option<Transform>,
    /// Presentation-only camera pose in the target mode's presentation frame.
    pub smoothed_pose: Option<Transform>,
    /// Free-fly (space) camera orbit angles, radians, and distance from the
    /// rocket. Adjusted by mouse drag / scroll while in `Free` mode.
    pub free_orbit_yaw: f32,
    pub free_orbit_pitch: f32,
    pub free_orbit_distance: f32,
}

impl Default for RocketCameraController {
    fn default() -> Self {
        Self {
            current_mode: RocketCameraMode::default(),
            target_mode: RocketCameraMode::default(),
            transition_progress: 0.0,
            transition_start_pose: None,
            smoothed_pose: None,
            free_orbit_yaw: 0.0,
            free_orbit_pitch: 0.35,
            free_orbit_distance: 600.0,
        }
    }
}

impl RocketCameraController {
    pub fn request_mode(&mut self, mode: RocketCameraMode) {
        if self.target_mode == mode {
            return;
        }
        self.target_mode = mode;
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }

    pub fn begin_transition(&mut self, pose: Transform) -> Transform {
        *self.transition_start_pose.get_or_insert(pose)
    }

    pub fn complete_transition(&mut self) {
        self.current_mode = self.target_mode;
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }

    pub fn cancel_transition(&mut self) {
        self.transition_progress = 0.0;
        self.transition_start_pose = None;
    }
}
