//! Android presentation: touch camera control and a subtle onboarding hint.
//!
//! Debug overlays (FPS, touch counter, finger dot, READY badge) are not shown
//! — a shipped product never exposes raw engine diagnostics to the player.
//! The same metrics continue to be written to the on-device startup log for
//! engineering use.
//!
//! The desktop camera is driven by keyboard and mouse; Android has neither, so
//! the solar view spawns in `FreeFlight` mode and touch supports that mode as
//! well as the orbital mode used when a body is framed.

use bevy::input::touch::Touches;
use bevy::math::Dir3;
use bevy::prelude::*;
use bevy::time::Real;

use crate::android::theme::*;
use crate::infrastructure::bevy_adapters::entity_components::{CameraController, CameraMode};
use crate::presentation::ui_components::UiCapture;

#[derive(Default)]
struct TouchState {
    last_position: Option<Vec2>,
    pinch_distance: Option<f32>,
}



/// Register the Android-only presentation systems.
pub fn install(app: &mut App) {
    app.add_systems(Startup, spawn_overlays)
        .add_systems(Update, touch_camera_controls);
}

fn spawn_overlays(_commands: Commands, _asset_server: Res<AssetServer>) {
    // Onboarding hint removed for mobile - UI should be self-explanatory
    // No "drag to look" or similar hints on mobile devices
}

/// One-finger drag looks/orbits; two-finger pinch zooms (or dollies in free
/// flight). Both write through the same `CameraController` / `Transform` state
/// the desktop input path uses.
///
/// Touches that start on UI elements are ignored to prevent buttons, menus,
/// and cards from being unresponsive on mobile.
fn touch_camera_controls(
    touches: Res<Touches>,
    ui_captures: Query<(&Node, &GlobalTransform, &ComputedNode, &Interaction), With<UiCapture>>,
    mut state: Local<TouchState>,
    mut cameras: Query<(&mut CameraController, &mut Transform)>,
) {
    // Collect at most two positions without allocating every frame.
    // Skip touches that are over UI elements to allow button interactions.
    let mut first = Vec2::ZERO;
    let mut second = Vec2::ZERO;
    let mut count = 0usize;
    for touch in touches.iter() {
        let pos = touch.position();
        
        // Skip touches that are over UI elements
        if is_touch_over_ui(pos, &ui_captures) {
            continue;
        }
        
        match count {
            0 => first = pos,
            1 => second = pos,
            _ => {}
        }
        count += 1;
    }

    match count {
        0 => {
            state.last_position = None;
            state.pinch_distance = None;
        }
        1 => {
            let position = first;
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



/// Check if a touch position is over any UI element.
fn is_touch_over_ui(
    touch_pos: Vec2,
    ui_captures: &Query<(&Node, &GlobalTransform, &ComputedNode, &Interaction), With<UiCapture>>,
) -> bool {
    for (node, transform, computed, interaction) in ui_captures.iter() {
        // Only check visible UI elements
        if node.display == Display::None {
            continue;
        }
        
        // Check if this UI element is being interacted with
        if *interaction != Interaction::None {
            let size = computed.size();
            let translation = transform.translation();
            let position = Vec2::new(translation.x, translation.y);
            let bounds = Rect::from_center_size(position, size);
            
            if bounds.contains(touch_pos) {
                return true;
            }
        }
    }
    false
}
