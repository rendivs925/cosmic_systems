//! Android presentation: touch camera control and on-screen readouts.
//!
//! The desktop camera is driven by keyboard and mouse; Android has neither, so
//! the solar view appeared frozen even while rendering at 70–90 Hz. The solar
//! camera spawns in `FreeFlight` mode, so touch must support that mode as well
//! as the orbital mode used when a body is framed.

use bevy::input::touch::Touches;
use bevy::math::Dir3;
use bevy::prelude::*;

use crate::android::controls::ControlPanelExtent;
use crate::android::theme::*;
use crate::infrastructure::bevy_adapters::entity_components::{CameraController, CameraMode};

/// FPS / frame-time label in the top-left corner.
#[derive(Component)]
struct FrameReadout;

#[derive(Default)]
struct TouchState {
    last_position: Option<Vec2>,
    pinch_distance: Option<f32>,
}

/// Register the Android-only presentation systems.
pub fn install(app: &mut App) {
    app.add_systems(Startup, spawn_overlays)
        .add_systems(Update, (touch_camera_controls, update_frame_readout));
}

fn spawn_overlays(mut commands: Commands) {
    // FPS pill.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(SAFE_TOP + SPACE_2),
                left: px(SPACE_3),
                padding: UiRect::new(px(SPACE_2), px(SPACE_2), px(SPACE_1), px(SPACE_1)),
                ..default()
            },
            BackgroundColor(SURFACE),
            BorderRadius::all(px(RADIUS_SM)),
        ))
        .with_child((
            Text::new("-- fps"),
            TextFont {
                font_size: FONT_CAPTION,
                ..default()
            },
            TextColor(TEXT_PRIMARY),
            FrameReadout,
        ));

    // Touch hint, one step down the type scale from the readout.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(SAFE_TOP + SPACE_8 + SPACE_2),
                left: px(SPACE_3),
                padding: UiRect::new(px(SPACE_2), px(SPACE_2), px(SPACE_1), px(SPACE_1)),
                ..default()
            },
            BackgroundColor(SURFACE),
            BorderRadius::all(px(RADIUS_SM)),
        ))
        .with_child((
            Text::new("Drag look · Pinch zoom · Tap select"),
            TextFont {
                font_size: FONT_CAPTION,
                ..default()
            },
            TextColor(TEXT_SECONDARY),
        ));
}

fn update_frame_readout(
    time: Res<Time>,
    mut window: Local<(f32, u32)>,
    mut readouts: Query<&mut Text, With<FrameReadout>>,
) {
    window.0 += time.delta_secs();
    window.1 += 1;
    if window.0 < 0.5 {
        return;
    }
    let fps = window.1 as f32 / window.0.max(0.001);
    let frame_ms = 1000.0 / fps.max(0.001);
    for mut text in &mut readouts {
        text.0 = format!("{fps:.0} fps  {frame_ms:.1} ms");
    }
    *window = (0.0, 0);
}

/// One-finger drag looks/orbits; two-finger pinch zooms (or dollies in free
/// flight). Both write through the same `CameraController` / `Transform` state
/// the desktop input path uses.
fn touch_camera_controls(
    touches: Res<Touches>,
    windows: Query<&Window>,
    extent: Res<ControlPanelExtent>,
    mut state: Local<TouchState>,
    mut cameras: Query<(&mut CameraController, &mut Transform)>,
) {
    let panel_top = match windows.single() {
        Ok(window) => {
            if extent.0 > 0.0 {
                window.height() - extent.0
            } else {
                window.height() * 0.55
            }
        }
        Err(_) => f32::MAX,
    };

    // Collect at most two positions without allocating every frame.
    let mut first = Vec2::ZERO;
    let mut second = Vec2::ZERO;
    let mut count = 0usize;
    for touch in touches.iter() {
        if touch.start_position().y > panel_top {
            continue;
        }
        match count {
            0 => first = touch.position(),
            1 => second = touch.position(),
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
