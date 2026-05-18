use super::craft_components::*;
use crate::domain::services::craft_physics;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::CursorGrabMode;
use bevy::window::CursorOptions;

const CAMERA_MODES: &[CraftCameraMode] = &[
    CraftCameraMode::Chase,
    CraftCameraMode::Orbit,
    CraftCameraMode::FirstPerson,
    CraftCameraMode::Free,
    CraftCameraMode::Cinematic,
];

pub fn update_craft_physics(
    time: Res<Time>,
    control: Res<CraftControlState>,
    mut craft_query: Query<(&mut CraftComponent, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.05);
    for (mut craft, mut transform) in craft_query.iter_mut() {
        let dc = control.dc_current;
        let pulse = control.pulse_current;
        craft.dc_field = dc;
        craft.pulse_resonance = pulse;
        let domain_craft = craft.craft.clone();
        craft_physics::compute_physics(&domain_craft, &mut craft.physics, dc, pulse, dt);

        craft.angular_velocity *= 1.0 - 4.0 * dt;
        let rot = Quat::from_axis_angle(Vec3::X, craft.angular_velocity.x * dt)
            * Quat::from_axis_angle(Vec3::Y, craft.angular_velocity.y * dt)
            * Quat::from_axis_angle(Vec3::Z, craft.angular_velocity.z * dt);
        transform.rotation = (transform.rotation * rot).normalize();

        let speed_mul = match craft.speed_mode {
            SpeedMode::Hover => 0.0,
            SpeedMode::Cruise => 1.0,
            SpeedMode::Sprint => 3.0,
        };
        let max_speed = 8.0 * dc * speed_mul;
        let accel = if max_speed > 0.01 { 6.0 } else { 0.0 };
        let target_vel = craft.throttle * max_speed * transform.forward().as_vec3();
        craft.linear_velocity = craft
            .linear_velocity
            .lerp(target_vel, (accel * dt).min(1.0));
        craft.linear_velocity *= 1.0 - 2.0 * dt;

        let lift = craft.physics.lift_force;
        let net_vert = lift - craft.craft.weight_kilonewtons;
        let grav_accel = net_vert / craft.craft.mass_tonnes - 0.29;
        craft.physics.vertical_velocity += grav_accel * dt;
        craft.physics.vertical_position += craft.physics.vertical_velocity * dt;

        transform.translation = Vec3::new(
            transform.translation.x + craft.linear_velocity.x * dt,
            craft.physics.vertical_position,
            transform.translation.z + craft.linear_velocity.z * dt,
        );
    }
}

pub fn handle_craft_input(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut craft_query: Query<(&mut CraftComponent, &Transform)>,
    mut control: ResMut<CraftControlState>,
    mut cam_state: ResMut<CraftCameraState>,
    mut cursor_query: Query<&mut CursorOptions>,
) {
    let dt = time.delta_secs().min(0.05);

    if keyboard.pressed(KeyCode::Comma) {
        control.dc_target = (control.dc_target - 0.5 * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::Period) {
        control.dc_target = (control.dc_target + 0.5 * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::BracketLeft) {
        control.pulse_target = (control.pulse_target - 0.3 * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::BracketRight) {
        control.pulse_target = (control.pulse_target + 0.3 * dt).clamp(0.0, 1.0);
    }

    let smooth = 3.0 * dt;
    control.dc_current += (control.dc_target - control.dc_current) * smooth;
    control.pulse_current += (control.pulse_target - control.pulse_current) * smooth;

    for wheel in mouse_wheel.read() {
        cam_state.target_distance = (cam_state.target_distance - wheel.y * 2.0).clamp(2.0, 60.0);
    }

    if keyboard.just_pressed(KeyCode::KeyV) {
        let len = CAMERA_MODES.len();
        control.camera_index = (control.camera_index + 1) % len;
        for (mut craft, _) in craft_query.iter_mut() {
            craft.camera_mode = CAMERA_MODES[control.camera_index];
            if craft.camera_mode == CraftCameraMode::FirstPerson {
                if let Ok(mut cursor) = cursor_query.single_mut() {
                    cursor.visible = false;
                    cursor.grab_mode = CursorGrabMode::Confined;
                }
            } else {
                if let Ok(mut cursor) = cursor_query.single_mut() {
                    cursor.visible = true;
                    cursor.grab_mode = CursorGrabMode::None;
                }
            }
        }
    }

    if keyboard.just_pressed(KeyCode::Escape) {
        for (mut craft, _) in craft_query.iter_mut() {
            if craft.camera_mode == CraftCameraMode::FirstPerson {
                control.camera_index = 0;
                craft.camera_mode = CraftCameraMode::Chase;
                if let Ok(mut cursor) = cursor_query.single_mut() {
                    cursor.visible = true;
                    cursor.grab_mode = CursorGrabMode::None;
                }
            }
        }
    }

    let mouse_down = mouse_buttons.pressed(MouseButton::Left);
    if mouse_down {
        cam_state.locked = false;
    } else {
        cam_state.locked = true;
    }

    for (mut craft, _) in craft_query.iter_mut() {
        let pitch_rate = 2.0;
        let yaw_rate = 2.5;
        let roll_rate = 3.0;

        if keyboard.pressed(KeyCode::KeyW) {
            craft.angular_velocity.x -= pitch_rate * dt;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            craft.angular_velocity.x += pitch_rate * dt;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            craft.angular_velocity.z += roll_rate * dt;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            craft.angular_velocity.z -= roll_rate * dt;
        }
        if keyboard.pressed(KeyCode::KeyQ) {
            craft.angular_velocity.y -= yaw_rate * dt;
        }
        if keyboard.pressed(KeyCode::KeyE) {
            craft.angular_velocity.y += yaw_rate * dt;
        }

        craft.angular_velocity = craft.angular_velocity.clamp_length_max(6.0);

        let mut throttle_dir = 0.0;
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            craft.speed_mode = SpeedMode::Sprint;
        } else if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight) {
            craft.speed_mode = SpeedMode::Hover;
        } else {
            craft.speed_mode = SpeedMode::Cruise;
        }

        if craft.speed_mode == SpeedMode::Hover {
            throttle_dir = 0.0;
            craft.linear_velocity = Vec3::ZERO;
            craft.angular_velocity *= 1.0 - 6.0 * dt;
        } else {
            if keyboard.pressed(KeyCode::ArrowUp) {
                throttle_dir += 1.0;
            }
            if keyboard.pressed(KeyCode::ArrowDown) {
                throttle_dir -= 1.0;
            }
        }

        craft.throttle = (craft.throttle + throttle_dir * 2.0 * dt).clamp(0.0, 1.0);

        if keyboard.pressed(KeyCode::KeyR) {
            craft.physics.vertical_velocity += 3.0 * dt;
        }
        if keyboard.pressed(KeyCode::KeyF) {
            craft.physics.vertical_velocity -= 3.0 * dt;
        }

    }
}

pub fn update_craft_camera(
    time: Res<Time>,
    mut mouse_motion: MessageReader<MouseMotion>,
    craft_query: Query<(&CraftComponent, &Transform)>,
    mut cam_state: ResMut<CraftCameraState>,
    mut camera_query: Query<&mut Transform, (With<CraftCameraTag>, Without<CraftComponent>)>,
) {
    let dt = time.delta_secs().min(0.05);
    let mut mouse_delta = Vec2::ZERO;
    for motion in mouse_motion.read() {
        mouse_delta += motion.delta;
    }

    let Ok((craft, craft_transform)) = craft_query.single() else {
        return;
    };
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    let target = craft_transform.translation;
    let forward = -craft_transform.forward().as_vec3();

    match craft.camera_mode {
        CraftCameraMode::Chase => {
            let sensitivity = 0.006;
            if !cam_state.locked {
                cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
                cam_state.orbit_pitch =
                    (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-0.8, 0.8);
            }
            let dist = cam_state.target_distance;
            let yaw = cam_state.orbit_yaw;
            let pitch = cam_state.orbit_pitch;
            let desired_pos = target
                + Quat::from_rotation_y(yaw) * (Vec3::new(0.0, pitch.sin() * dist + 2.0, pitch.cos() * dist))
                + forward * 4.0;
            let lerp = 1.0 - (-4.0 * dt).exp();
            let new_pos = camera_transform.translation.lerp(desired_pos, lerp);
            camera_transform.translation = new_pos;
            camera_transform.look_at(target, Vec3::Y);
        }
        CraftCameraMode::Orbit => {
            let sensitivity = 0.006;
            cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
            cam_state.orbit_pitch =
                (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-1.2, 1.2);
            let dist = cam_state.target_distance;
            let yaw = cam_state.orbit_yaw;
            let pitch = cam_state.orbit_pitch;
            let desired_pos = target
                + Quat::from_rotation_y(yaw)
                    * Vec3::new(
                        pitch.cos() * dist * yaw.sin(),
                        pitch.sin() * dist,
                        pitch.cos() * dist * yaw.cos(),
                    );
            let lerp = 1.0 - (-4.0 * dt).exp();
            let new_pos = camera_transform.translation.lerp(desired_pos, lerp);
            camera_transform.translation = new_pos;
            camera_transform.look_at(target, Vec3::Y);
        }
        CraftCameraMode::FirstPerson => {
            let sensitivity = 0.005;
            cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
            cam_state.orbit_pitch =
                (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-1.5, 1.5);
            let view_rot =
                Quat::from_rotation_y(cam_state.orbit_yaw) * Quat::from_rotation_x(cam_state.orbit_pitch);
            let local_offset = Vec3::new(0.0, 0.5, 0.8);
            let world_offset = view_rot * local_offset;
            let cam_pos = target + world_offset;
            camera_transform.translation = cam_pos;
            let look = view_rot * -Vec3::Z;
            camera_transform.look_at(cam_pos + look, Vec3::Y);
        }
        CraftCameraMode::Free => {
            let sensitivity = 0.004;
            cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
            cam_state.orbit_pitch =
                (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-1.2, 1.2);
            if cam_state.smooth_position == Vec3::ZERO {
                cam_state.smooth_position = camera_transform.translation;
                cam_state.smooth_look = target - camera_transform.translation;
            }
            let desired_look = target - cam_state.smooth_position;
            let lerp = 1.0 - (-2.0 * dt).exp();
            let new_look = cam_state.smooth_look.lerp(desired_look, lerp);
            cam_state.smooth_look = new_look;
            camera_transform.translation = cam_state.smooth_position;
            let look_target = cam_state.smooth_position + cam_state.smooth_look;
            camera_transform.look_at(look_target, Vec3::Y);
        }
        CraftCameraMode::Cinematic => {
            let time_secs = time.elapsed_secs();
            let orbit_speed = 0.3;
            let dist = cam_state.target_distance * 1.2;
            let yaw = time_secs * orbit_speed;
            let pitch = 0.3 + (time_secs * 0.15).sin() * 0.15;
            let desired_pos = target
                + Quat::from_rotation_y(yaw)
                    * Vec3::new(pitch.cos() * dist, pitch.sin() * dist + 1.5, 0.0);
            let lerp = 1.0 - (-2.0 * dt).exp();
            let new_pos = camera_transform.translation.lerp(desired_pos, lerp);
            camera_transform.translation = new_pos;
            camera_transform.look_at(target, Vec3::Y);
        }
    }
}
