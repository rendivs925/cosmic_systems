//! Android presentation: touch camera control.
//!
//! The desktop camera is driven by keyboard and mouse; Android has neither, so
//! the solar view spawns in `FreeFlight` mode and touch supports that mode as
//! well as the orbital mode used when a body is framed.
//!
//! UI interactions use Bevy's built-in touch system - no custom conversion needed.

use bevy::math::{Dir3, Rect};
use bevy::prelude::*;

use crate::infrastructure::bevy_adapters::entity_components::{CameraController, CameraMode};

#[derive(Default)]
struct TouchState {
    last_position: Option<Vec2>,
    pinch_distance: Option<f32>,
}

/// Register the Android-only presentation systems.
pub fn install(app: &mut App) {
    app.add_systems(Update, touch_camera_controls);
}

/// One-finger drag looks/orbits; two-finger pinch zooms (or dollies in free
/// flight). Both write through the same `CameraController` / `Transform` state
/// the desktop input path uses.
///
/// Touches that are over UI elements are ignored to prevent buttons, menus,
/// and cards from being unresponsive on mobile. Bevy's built-in touch system
/// handles UI interactions automatically.
fn touch_camera_controls(
    touches: Res<Touches>,
    ui_interactions: Query<(Entity, &Interaction, &GlobalTransform, &ComputedNode)>,
    mut state: Local<TouchState>,
    mut cameras: Query<(&mut CameraController, &mut Transform)>,
) {
    // Collect touches that are NOT over UI elements
    let mut camera_touches: Vec<Vec2> = Vec::new();

    for touch in touches.iter() {
        let touch_pos = touch.position();
        
        // Check if this specific touch is over any UI element
        let is_over_ui = is_touch_over_ui_element(touch_pos, &ui_interactions);
        
        if !is_over_ui {
            camera_touches.push(touch_pos);
        }
    }

    // Process camera controls with non-UI touches
    match camera_touches.len() {
        0 => {
            state.last_position = None;
            state.pinch_distance = None;
        }
        1 => {
            let position = camera_touches[0];
            if let Some(previous) = state.last_position {
                let delta = position - previous;
                for (mut controller, mut transform) in &mut cameras {
                    match controller.mode {
                        CameraMode::FreeFlight => {
                            // Same rotation model as the desktop mouse-look path.
                            let sensitivity = controller.sensitivity * 2.0;
                            transform.rotate_y(-delta.x * sensitivity);
                            let right = *transform.right();
                            transform.rotate_axis(
                                Dir3::new(right).unwrap_or(Dir3::X),
                                -delta.y * sensitivity,
                            );
                            // Clamp pitch so the horizon cannot flip.
                            let euler = transform.rotation.to_euler(EulerRot::YXZ);
                            let clamped = euler
                                .1
                                .clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
                            transform.rotation =
                                Quat::from_euler(EulerRot::YXZ, euler.0, clamped, euler.2);
                        }
                        CameraMode::Orbit => {
                            controller.orbit_angle -= delta.x * 0.005;
                            let zoom = (1.0 + delta.y * 0.002).clamp(0.9, 1.1);
                            controller.orbit_distance =
                                (controller.orbit_distance * zoom).clamp(20.0, 200_000.0);
                        }
                        CameraMode::TerrainView => {}
                    }
                }
            }
            state.last_position = Some(position);
            state.pinch_distance = None;
        }
        _ => {
            // Two or more touches - handle pinch zoom
            let first = camera_touches[0];
            let second = camera_touches[1];
            let distance = first.distance(second);
            
            if let Some(previous) = state.pinch_distance {
                let ratio = (distance / previous.max(1.0) - 1.0).clamp(-0.25, 0.25);
                for (mut controller, mut transform) in &mut cameras {
                    match controller.mode {
                        CameraMode::FreeFlight => {
                            // Scale the dolly step by the current distance from
                            // the scene centre so it works from 1e3 to 1e7 units.
                            let scale = transform.translation.length().max(1.0);
                            let step = scale * ratio * 0.8;
                            let forward = *transform.forward();
                            transform.translation += forward * step;
                        }
                        CameraMode::Orbit => {
                            let factor = 1.0 / (1.0 + ratio);
                            controller.orbit_distance =
                                (controller.orbit_distance * factor).clamp(20.0, 200_000.0);
                        }
                        CameraMode::TerrainView => {}
                    }
                }
            }
            state.pinch_distance = Some(distance);
            state.last_position = None;
        }
    }
}

/// Check if a touch position is over any UI element using Bevy's built-in Interaction system.
fn is_touch_over_ui_element(
    touch_pos: Vec2,
    ui_interactions: &Query<(Entity, &Interaction, &GlobalTransform, &ComputedNode)>,
) -> bool {
    for (_entity, interaction, transform, computed) in ui_interactions.iter() {
        // Only block touches when UI element is actually Pressed, not Hovered
        if *interaction == Interaction::Pressed {
            // Get the UI element's bounds
            let translation = transform.translation();
            let ui_pos = Vec2::new(translation.x, translation.y);
            let size = computed.size();
            
            // Check if touch is within this element's bounds
            let bounds = Rect::from_center_size(ui_pos, size);
            if bounds.contains(touch_pos) {
                return true;
            }
        }
    }
    false
}

