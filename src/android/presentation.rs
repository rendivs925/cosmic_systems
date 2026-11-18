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

#[derive(Default)]
struct TouchState {
    last_position: Option<Vec2>,
    pinch_distance: Option<f32>,
}

/// Small onboarding pill in the top-left corner. Auto-dismisses after the first
/// touch or after a fixed timeout — players should not see it for long.
#[derive(Component)]
struct OnboardingHint;

/// How long the onboarding hint stays visible after first appearance.
const ONBOARDING_AUTO_DISMISS_SECS: f32 = 6.0;

/// Register the Android-only presentation systems.
pub fn install(app: &mut App) {
    app.add_systems(Startup, spawn_overlays)
        .add_systems(Update, (touch_camera_controls, update_onboarding_hint));
}

fn spawn_overlays(mut commands: Commands, asset_server: Res<AssetServer>) {
    let ui_font: Handle<Font> = asset_server.load(FONT_PATH);

    // Onboarding hint: a compact pill that teaches the core gesture vocabulary.
    // Auto-hides on first touch or after the timeout.
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                top: px(SAFE_TOP + SPACE_3),
                left: px(SPACE_3),
                padding: UiRect::new(px(SPACE_3), px(SPACE_3), px(SPACE_2), px(SPACE_2)),
                ..default()
            },
            BackgroundColor(SURFACE_PANEL),
            BorderRadius::all(px(RADIUS_MD)),
            OnboardingHint,
        ))
        .with_child((
            Text::new("Drag to look · Pinch to zoom · Tap to select"),
            TextFont {
                font: ui_font,
                font_size: FONT_CAPTION,
                ..default()
            },
            TextColor(TEXT_SECONDARY),
        ));
}

/// One-finger drag looks/orbits; two-finger pinch zooms (or dollies in free
/// flight). Both write through the same `CameraController` / `Transform` state
/// the desktop input path uses.
fn touch_camera_controls(
    touches: Res<Touches>,
    mut state: Local<TouchState>,
    mut cameras: Query<(&mut CameraController, &mut Transform)>,
) {
    // Collect at most two positions without allocating every frame.
    let mut first = Vec2::ZERO;
    let mut second = Vec2::ZERO;
    let mut count = 0usize;
    for touch in touches.iter() {
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

/// Dismiss the onboarding hint on first touch, or automatically after a timeout.
fn update_onboarding_hint(
    touches: Res<Touches>,
    real_time: Res<Time<Real>>,
    mut hints: Query<(Entity, &mut Node), With<OnboardingHint>>,
    mut shown_at: Local<Option<f32>>,
    mut dismissed: Local<bool>,
) {
    if *dismissed {
        return;
    }
    if shown_at.is_none() {
        *shown_at = Some(real_time.elapsed_secs());
    }
    let elapsed = real_time.elapsed_secs() - shown_at.unwrap();
    if touches.any_just_pressed() || elapsed >= ONBOARDING_AUTO_DISMISS_SECS {
        *dismissed = true;
        for (_entity, mut node) in &mut hints {
            node.display = Display::None;
        }
    }
}
