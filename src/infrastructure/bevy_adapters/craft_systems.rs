use super::craft_components::*;
use crate::domain::services::craft_physics;
use bevy::input::mouse::MouseMotion;
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;

pub fn update_craft_physics(
    time: Res<Time>,
    control: Res<CraftControlState>,
    mut craft_query: Query<(&mut CraftComponent, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut craft, mut transform) in craft_query.iter_mut() {
        let dc = control.dc_current;
        let pulse = control.pulse_current;
        craft.dc_field = dc;
        craft.pulse_resonance = pulse;
        let domain_craft = craft.craft.clone();
        craft_physics::compute_physics(&domain_craft, &mut craft.physics, dc, pulse, dt);
        transform.translation.y = craft.physics.vertical_position;
        transform.translation += craft.horizontal_velocity * dt;
    }
}

pub fn handle_craft_input(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut craft_query: Query<(&mut CraftComponent, &mut Transform)>,
    mut control: ResMut<CraftControlState>,
    mut cursor_query: Query<&mut CursorOptions>,
) {
    let dt = time.delta_secs();
    let dc_rate = 0.5;
    let pulse_rate = 0.3;

    if keyboard.pressed(KeyCode::Comma) {
        control.dc_target = (control.dc_target - dc_rate * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::Period) {
        control.dc_target = (control.dc_target + dc_rate * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::BracketLeft) {
        control.pulse_target = (control.pulse_target - pulse_rate * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::BracketRight) {
        control.pulse_target = (control.pulse_target + pulse_rate * dt).clamp(0.0, 1.0);
    }

    let smooth = 3.0 * dt;
    control.dc_current += (control.dc_target - control.dc_current) * smooth;
    control.pulse_current += (control.pulse_target - control.pulse_current) * smooth;

    for (mut craft, mut transform) in craft_query.iter_mut() {
        if keyboard.just_pressed(KeyCode::KeyV) {
            craft.camera_mode = match craft.camera_mode {
                CraftCameraMode::External => CraftCameraMode::FirstPerson,
                CraftCameraMode::FirstPerson => CraftCameraMode::External,
            };
            if let Ok(mut cursor) = cursor_query.single_mut() {
                if craft.camera_mode == CraftCameraMode::FirstPerson {
                    cursor.visible = false;
                    cursor.grab_mode = CursorGrabMode::Confined;
                } else {
                    cursor.visible = true;
                    cursor.grab_mode = CursorGrabMode::None;
                }
            }
        }
        if keyboard.just_pressed(KeyCode::Escape) {
            if craft.camera_mode == CraftCameraMode::FirstPerson {
                craft.camera_mode = CraftCameraMode::External;
                if let Ok(mut cursor) = cursor_query.single_mut() {
                    cursor.visible = true;
                    cursor.grab_mode = CursorGrabMode::None;
                }
            }
        }

        let dc = craft.dc_field;
        let move_speed = 4.0 * (dc * 0.5 + 0.5).max(0.1);
        let mut move_dir = Vec3::ZERO;
        if keyboard.pressed(KeyCode::KeyW) {
            move_dir -= transform.forward().as_vec3();
        }
        if keyboard.pressed(KeyCode::KeyS) {
            move_dir += transform.forward().as_vec3();
        }
        if keyboard.pressed(KeyCode::KeyA) {
            move_dir -= transform.right().as_vec3();
        }
        if keyboard.pressed(KeyCode::KeyD) {
            move_dir += transform.right().as_vec3();
        }
        if move_dir != Vec3::ZERO {
            move_dir = move_dir.normalize_or_zero();
            craft.horizontal_velocity =
                craft
                    .horizontal_velocity
                    .lerp(move_dir * move_speed, 3.0 * dt);
        } else {
            craft.horizontal_velocity =
                craft.horizontal_velocity.lerp(Vec3::ZERO, 5.0 * dt);
        }

        if keyboard.pressed(KeyCode::ArrowUp) {
            craft.yaw += 1.5 * dt;
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            craft.yaw -= 1.5 * dt;
        }
        transform.rotation = Quat::from_rotation_y(craft.yaw);
    }
}

pub fn update_craft_camera(
    time: Res<Time>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut craft_query: Query<(&mut CraftComponent, &Transform)>,
    mut camera_query: Query<&mut Transform, (With<CraftCameraTag>, Without<CraftComponent>)>,
) {
    let dt = time.delta_secs();
    let mut mouse_delta = Vec2::ZERO;
    for motion in mouse_motion.read() {
        mouse_delta += motion.delta;
    }

    for (mut craft, craft_transform) in craft_query.iter_mut() {
        for mut camera_transform in camera_query.iter_mut() {
            match craft.camera_mode {
                CraftCameraMode::External => {
                    let sensitivity = 0.005;
                    let yaw = -mouse_delta.x * sensitivity;
                    let pitch = -mouse_delta.y * sensitivity;
                    let dist = 6.0;
                    let orbit_angle = time.elapsed_secs() * 0.0;
                    let height_angle = 0.4 + pitch;
                    let target = craft_transform.translation;
                    let cam_pos = Vec3::new(
                        (orbit_angle + yaw).sin() * dist,
                        height_angle.sin() * dist + 2.0,
                        (orbit_angle + yaw).cos() * dist,
                    );
                    camera_transform.translation =
                        camera_transform.translation.lerp(target + cam_pos, 5.0 * dt);
                    camera_transform.look_at(target, Vec3::Y);
                }
                CraftCameraMode::FirstPerson => {
                    let sensitivity = 0.005;
                    craft.yaw -= mouse_delta.x * sensitivity;
                    craft.pitch = (craft.pitch - mouse_delta.y * sensitivity).clamp(-1.5, 1.5);
                    let local_offset = Vec3::new(0.0, 0.5, 0.8);
                    let view_rot = Quat::from_rotation_y(craft.yaw)
                        * Quat::from_rotation_x(craft.pitch);
                    let world_offset = view_rot * local_offset;
                    let cam_pos = craft_transform.translation + world_offset;
                    camera_transform.translation = cam_pos;
                    let look_target = cam_pos + view_rot * -Vec3::Z;
                    camera_transform.look_at(look_target, Vec3::Y);
                }
            }
        }
    }
}
