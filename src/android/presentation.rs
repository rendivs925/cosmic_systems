//! Android presentation: touch camera control and UI interaction.
//!
//! The desktop camera is driven by keyboard and mouse; Android has neither, so
//! the solar view spawns in `FreeFlight` mode and touch supports that mode as
//! well as the orbital mode used when a body is framed.
//!
//! Touch input is converted to UI interactions to enable button presses on mobile.

use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::math::Dir3;
use bevy::prelude::*;
use bevy::ui::{UiGlobalTransform, UiSystems};

use crate::infrastructure::bevy_adapters::entity_components::{CameraController, CameraMode};
use crate::presentation::ui_components::UiCapture;

#[derive(Default)]
struct TouchState {
    last_position: Option<Vec2>,
    pinch_distance: Option<f32>,
}

/// Register the Android-only presentation systems.
pub fn install(app: &mut App) {
    app.add_systems(
        PreUpdate,
        android_touch_to_ui_interaction.before(UiSystems::Focus),
    )
    .add_systems(Update, touch_camera_controls);
}

/// Convert Android touch events to UI Interaction states.
/// This system runs before Bevy's ui_focus_system to ensure touch input
/// properly updates UI button states on Android devices.
///
/// The system:
/// 1. Reads all touch events
/// 2. Converts touch positions to UI coordinate space
/// 3. Uses ComputedNode::contains_point() for accurate hit testing
/// 4. Sets Interaction components on touched UI nodes
fn android_touch_to_ui_interaction(
    mut touch_events: MessageReader<TouchInput>,
    windows: Query<&Window>,
    ui_nodes: Query<
        (Entity, &ComputedNode, &UiGlobalTransform, Option<&Interaction>),
        With<UiCapture>,
    >,
    mut commands: Commands,
) {
    // Get the primary window for coordinate conversion
    let Ok(window) = windows.single() else {
        return;
    };

    let scale_factor = window.scale_factor();
    let window_size = window.physical_size();

    // Track which nodes are currently touched
    let mut touched_nodes: Vec<Entity> = Vec::new();

    // Process all touch events
    for touch in touch_events.read() {
        let touch_pos = touch.position;

        // Convert touch position from logical pixels (origin top-left, Y down)
        // to UI coordinate space (origin center, Y up) in physical pixels
        let ui_touch_pos = Vec2::new(
            touch_pos.x * scale_factor - window_size.x as f32 / 2.0,
            window_size.y as f32 / 2.0 - touch_pos.y * scale_factor,
        );

        // Find which UI node is under this touch
        for (entity, computed, transform, interaction) in ui_nodes.iter() {
            // Use Bevy's built-in contains_point method for accurate hit testing
            if computed.contains_point(*transform, ui_touch_pos) {
                touched_nodes.push(entity);

                // Set interaction state based on touch phase
                match touch.phase {
                    TouchPhase::Started => {
                        if let Some(current) = interaction {
                            if *current != Interaction::Pressed {
                                commands.entity(entity).insert(Interaction::Pressed);
                            }
                        } else {
                            commands.entity(entity).insert(Interaction::Pressed);
                        }
                    }
                    TouchPhase::Moved | TouchPhase::Canceled => {
                        if let Some(current) = interaction {
                            if *current == Interaction::Pressed {
                                commands.entity(entity).insert(Interaction::Hovered);
                            }
                        }
                    }
                    TouchPhase::Ended => {
                        // Clear interaction when touch ends
                        commands.entity(entity).insert(Interaction::None);
                    }
                }
            }
        }
    }

    // Clear interaction on nodes that are no longer touched
    for (entity, _computed, _transform, interaction) in ui_nodes.iter() {
        if let Some(current) = interaction {
            if *current != Interaction::None && !touched_nodes.contains(&entity) {
                commands.entity(entity).insert(Interaction::None);
            }
        }
    }
}

/// One-finger drag looks/orbits; two-finger pinch zooms (or dollies in free
/// flight). Both write through the same `CameraController` / `Transform` state
/// the desktop input path uses.
///
/// Touches that are over UI elements are ignored to prevent buttons, menus,
/// and cards from being unresponsive on mobile.
fn touch_camera_controls(
    touches: Res<Touches>,
    windows: Query<&Window>,
    ui_nodes: Query<(&ComputedNode, &UiGlobalTransform), With<UiCapture>>,
    mut state: Local<TouchState>,
    mut cameras: Query<(&mut CameraController, &mut Transform)>,
) {
    // Get window for coordinate conversion
    let Ok(window) = windows.single() else {
        return;
    };
    let scale_factor = window.scale_factor();
    let window_size = window.physical_size();

    // Collect at most two positions without allocating every frame.
    // Skip touches that are over UI elements to allow button interactions.
    let mut first = Vec2::ZERO;
    let mut second = Vec2::ZERO;
    let mut count = 0usize;

    for touch in touches.iter() {
        let pos = touch.position();

        // Convert touch position to UI coordinate space for hit testing
        let ui_pos = Vec2::new(
            pos.x * scale_factor - window_size.x as f32 / 2.0,
            window_size.y as f32 / 2.0 - pos.y * scale_factor,
        );

        // Check if this touch is over any UI element
        let is_over_ui = is_touch_over_ui(ui_pos, &ui_nodes);

        if is_over_ui {
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

/// Check if a touch position (in UI coordinate space) is over any UI element.
/// Uses Bevy's built-in contains_point method for accurate bounds checking.
fn is_touch_over_ui(
    ui_touch_pos: Vec2,
    ui_nodes: &Query<(&ComputedNode, &UiGlobalTransform), With<UiCapture>>,
) -> bool {
    for (computed, transform) in ui_nodes.iter() {
        // Use Bevy's built-in contains_point method
        if computed.contains_point(*transform, ui_touch_pos) {
            return true;
        }
    }
    false
}
