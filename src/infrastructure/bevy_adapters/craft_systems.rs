use super::components::PlanetComponent;
use super::craft_components::*;
use crate::domain::services::craft_physics;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::math::DVec3;
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

const CRAFT_CRUISE_SPEED_UNITS: f32 = 40_000.0;
const CHASE_CAMERA_HEIGHT: f32 = 4.0;
const CHASE_CAMERA_LOOK_HEIGHT: f32 = 1.0;
pub fn update_craft_physics(
    fixed_time: Res<Time<Fixed>>,
    control: Res<CraftControlState>,
    solar_params: Res<SolarSystemParameters>,
    mut travel_target: ResMut<CraftTravelTarget>,
    planet_query: Query<&PlanetComponent>,
    mut craft_query: Query<&mut CraftComponent>,
) {
    let dt = fixed_time.delta_secs();
    for mut craft in craft_query.iter_mut() {
        let dc = control.dc_current;
        let pulse = control.pulse_current;
        craft.dc_field = dc;
        craft.pulse_resonance = pulse;
        let domain_craft = craft.craft.clone();
        craft_physics::compute_physics(&domain_craft, &mut craft.physics, dc, pulse, dt);

        let angular_input = craft.angular_input;
        craft.angular_velocity += angular_input * dt;
        craft.angular_velocity = craft.angular_velocity.clamp_length_max(6.0);
        let angular_damping = if craft.speed_mode == SpeedMode::Hover {
            6.0
        } else {
            4.0
        };
        craft.angular_velocity *= (1.0 - angular_damping * dt).max(0.0);
        let rot = Quat::from_axis_angle(Vec3::X, craft.angular_velocity.x * dt)
            * Quat::from_axis_angle(Vec3::Y, craft.angular_velocity.y * dt)
            * Quat::from_axis_angle(Vec3::Z, craft.angular_velocity.z * dt);
        craft.orientation = (craft.orientation * rot).normalize();

        let speed_mul = match craft.speed_mode {
            SpeedMode::Hover => 0.0,
            SpeedMode::Cruise => 1.0,
            SpeedMode::Sprint => 3.0,
        };
        let max_speed = CRAFT_CRUISE_SPEED_UNITS * dc * speed_mul;
        let accel = 3.0;
        let mut autopilot_position = None;
        if let Some(target_entity) = travel_target.entity {
            if let Ok(planet) = planet_query.get(target_entity) {
                autopilot_position = Some((
                    planet,
                    catalog_planet_position(
                        planet,
                        fixed_time.elapsed_secs_f64(),
                        &solar_params,
                        &planet_query,
                    ),
                ));
            } else {
                travel_target.entity = None;
                travel_target.name = None;
            }
        }

        let autopilot_active = autopilot_position.is_some();

        if let Some((planet, target_position)) = autopilot_position {
            let radius = if planet.domain_planet.name == "Sun" {
                physics::calculate_sun_visual_radius(&solar_params)
            } else {
                physics::calculate_visual_radius(&planet.domain_planet, &solar_params)
            };
            let away_from_target = (craft.position - target_position).normalize_or_zero();
            let approach_direction = if away_from_target.length_squared() > 0.0 {
                away_from_target
            } else {
                Vec3::X
            };
            let destination = target_position + approach_direction * (radius * 3.0 + 500.0);

            // Smooth travel: approach the destination with a bounded per-frame step so the
            // renderer never sees an instantaneous teleport onto a body. Exponential decay
            // decelerates near arrival and a hard step cap prevents any single-frame jump,
            // which is what triggered GPU device loss when snapping to the Sun.
            let to_destination = destination - craft.position;
            let distance = to_destination.length();
            let approach_step = (1.0 - (-1.2 * dt).exp()).clamp(0.001, 1.0);
            let max_step = 500_000.0;
            let step = (distance * approach_step).min(max_step);

            if step < 1.0 {
                craft.position = destination;
                travel_target.entity = None;
                travel_target.name = None;
            } else {
                craft.position += to_destination.normalize() * step;
                craft.linear_velocity = Vec3::ZERO;
            }
            let look_direction = (target_position - craft.position).normalize_or_zero();
            if look_direction.length_squared() > 0.0 {
                craft.orientation = Quat::from_rotation_arc(Vec3::NEG_Z, look_direction);
            }
            craft.physics.vertical_velocity = 0.0;
            craft.physics.vertical_position = craft.position.y;
        } else {
            let move_input = craft.move_input.clamp_length_max(1.0);
            let magnitude = move_input.length();
            let local_dir = if magnitude > 0.001 {
                Vec3::new(move_input.x, 0.0, move_input.y).normalize()
            } else {
                Vec3::ZERO
            };
            let world_dir = craft.orientation * local_dir;
            let target_vel = world_dir * magnitude * max_speed;
            craft.linear_velocity = craft
                .linear_velocity
                .lerp(target_vel, (accel * dt).min(1.0));
            craft.linear_velocity *= 1.0 - 2.0 * dt;
        }

        if !autopilot_active {
            // `compute_physics` is the single vertical integration authority.
            // Presentation movement consumes its velocity without applying the
            // lift/gravity acceleration a second time.
            craft.linear_velocity.y = craft.physics.vertical_velocity;
        }

        let linear_velocity = craft.linear_velocity;
        craft.position += linear_velocity * dt;
        craft.physics.vertical_position = craft.position.y;
        if autopilot_active {
            craft.physics.vertical_velocity = craft.linear_velocity.y;
        }
    }
}

/// Derive a travel target from the same orbital catalog used for solar motion.
/// This keeps autopilot independent of render-transform propagation.
fn catalog_planet_position(
    planet: &PlanetComponent,
    time_seconds: f64,
    solar_params: &SolarSystemParameters,
    planets: &Query<&PlanetComponent>,
) -> Vec3 {
    let time_days = solar_params.time_to_days_f64(time_seconds);
    let (parent_position, parent_tilt) = planet
        .domain_planet
        .parent_entity
        .as_ref()
        .and_then(|parent_name| {
            planets
                .iter()
                .find(|candidate| candidate.domain_planet.name == *parent_name)
                .map(|parent| {
                    (
                        physics::calculate_planet_position_f64(
                            &parent.domain_planet,
                            time_days,
                            solar_params,
                            DVec3::ZERO,
                            None,
                        )
                        .as_vec3(),
                        Some(parent.domain_planet.axial_tilt_deg),
                    )
                })
        })
        .unwrap_or((Vec3::ZERO, None));

    physics::calculate_planet_position_f64(
        &planet.domain_planet,
        time_days,
        solar_params,
        DVec3::from(parent_position),
        parent_tilt,
    )
    .as_vec3()
}

/// Copy authoritative craft state to its render transform. Register this in
/// `Update` after fixed physics so cameras and visuals never feed back into
/// flight integration.
pub fn sync_craft_transform(mut craft_query: Query<(&CraftComponent, &mut Transform)>) {
    for (craft, mut transform) in craft_query.iter_mut() {
        transform.translation = craft.position;
        transform.rotation = craft.orientation;
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
    mut effects_enabled: ResMut<CraftEffectsEnabled>,
) {
    let dt = time.delta_secs().min(0.05);

    if keyboard.pressed(KeyCode::Comma) {
        control.dc_target = (control.dc_target - 0.5 * dt).clamp(0.0, 1.0);
    }
    if keyboard.pressed(KeyCode::Period) {
        control.dc_target = (control.dc_target + 0.5 * dt).clamp(0.0, 1.0);
    }
    let smooth = 3.0 * dt;
    control.dc_current += (control.dc_target - control.dc_current) * smooth;
    control.pulse_current = 0.0;

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

    if keyboard.just_pressed(KeyCode::KeyB) {
        effects_enabled.0 = !effects_enabled.0;
    }

    let mouse_down = mouse_buttons.pressed(MouseButton::Left);
    cam_state.locked = !mouse_down;

    for (mut craft, _) in craft_query.iter_mut() {
        let mut move_x = 0.0;
        let mut move_z = 0.0;
        if keyboard.pressed(KeyCode::KeyW) {
            move_z -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyS) {
            move_z += 1.0;
        }
        if keyboard.pressed(KeyCode::KeyA) {
            move_x -= 1.0;
        }
        if keyboard.pressed(KeyCode::KeyD) {
            move_x += 1.0;
        }
        craft.move_input = Vec2::new(move_x, move_z);

        let roll =
            (keyboard.pressed(KeyCode::KeyQ) as i8 - keyboard.pressed(KeyCode::KeyE) as i8) as f32;
        let pitch =
            (keyboard.pressed(KeyCode::KeyK) as i8 - keyboard.pressed(KeyCode::KeyJ) as i8) as f32;
        let yaw =
            (keyboard.pressed(KeyCode::KeyH) as i8 - keyboard.pressed(KeyCode::KeyL) as i8) as f32;
        craft.angular_input = Vec3::new(pitch * 2.0, yaw * 2.5, roll * 3.0);

        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            craft.speed_mode = SpeedMode::Sprint;
        } else if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)
        {
            craft.speed_mode = SpeedMode::Hover;
        } else {
            craft.speed_mode = SpeedMode::Cruise;
        }

        if craft.speed_mode == SpeedMode::Hover {
            craft.move_input = Vec2::ZERO;
            craft.linear_velocity = Vec3::ZERO;
        }
    }
}

pub fn update_craft_camera(
    time: Res<Time>,
    mut mouse_motion: MessageReader<MouseMotion>,
    craft_query: Query<&CraftComponent>,
    mut cam_state: ResMut<CraftCameraState>,
    mut camera_query: Query<
        (&mut Transform, &mut Projection),
        (With<CraftCameraTag>, Without<CraftComponent>),
    >,
    planet_query: Query<&GlobalTransform, With<PlanetComponent>>,
) {
    let dt = time.delta_secs().min(0.05);
    let elapsed = time.elapsed_secs();
    let mut mouse_delta = Vec2::ZERO;
    for motion in mouse_motion.read() {
        mouse_delta += motion.delta;
    }

    let Ok(craft) = craft_query.single() else {
        return;
    };
    let Ok((mut camera_transform, mut projection)) = camera_query.single_mut() else {
        return;
    };

    let target = craft.position;
    let look_target = target + Vec3::Y * CHASE_CAMERA_LOOK_HEIGHT;

    let speed = craft.linear_velocity.length();
    let is_hover = matches!(craft.speed_mode, SpeedMode::Hover);
    let is_sprint = matches!(craft.speed_mode, SpeedMode::Sprint);
    let parametric = craft.physics.parametric_gain;
    let zpe = craft.physics.zpe_kilowatts;

    match craft.camera_mode {
        CraftCameraMode::Chase => {
            let sensitivity = 0.006;
            if !cam_state.locked {
                cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
                cam_state.orbit_pitch =
                    (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-0.8, 0.8);
            }
            let mut dist = cam_state.target_distance;
            let mut height_offset = CHASE_CAMERA_HEIGHT;
            if is_hover {
                dist *= 0.6;
                height_offset = 2.0;
            } else if is_sprint {
                dist *= 1.3;
                height_offset = 5.0;
            }
            let yaw = cam_state.orbit_yaw;
            let pitch = cam_state.orbit_pitch;
            let chase_offset = Vec3::new(0.0, height_offset + pitch.sin() * dist, dist);
            let mut desired_pos =
                target + craft.orientation * Quat::from_rotation_y(yaw) * chase_offset;
            if parametric {
                let shake_amp = 0.02 + (zpe / 1250.0) * 0.04;
                desired_pos += Vec3::new(
                    (elapsed * 44.8).sin() * shake_amp,
                    (elapsed * 37.3 + 1.2).sin() * shake_amp * 0.5,
                    (elapsed * 52.1 + 0.7).sin() * shake_amp * 0.7,
                );
            }
            camera_transform.translation = desired_pos;
            camera_transform.look_at(look_target, Vec3::Y);
            if let Projection::Perspective(ref mut proj) = *projection {
                proj.fov = (55.0_f32.to_radians()
                    + (is_sprint as u32 as f32 * 2.0 + is_hover as u32 as f32 * -3.0).to_radians())
                .clamp(50.0_f32.to_radians(), 65.0_f32.to_radians());
            }
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
            camera_transform.look_at(look_target, Vec3::Y);
        }
        CraftCameraMode::FirstPerson => {
            let sensitivity = 0.005;
            cam_state.orbit_yaw -= mouse_delta.x * sensitivity;
            cam_state.orbit_pitch =
                (cam_state.orbit_pitch - mouse_delta.y * sensitivity).clamp(-1.5, 1.5);
            let view_rot = Quat::from_rotation_y(cam_state.orbit_yaw)
                * Quat::from_rotation_x(cam_state.orbit_pitch);
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
            let orbit_speed = 0.3 + (speed / 40000.0).min(1.0) * 0.1;
            let dist = cam_state.target_distance * 1.2 + (speed / 40000.0).min(1.0) * 4.0;
            let yaw = time_secs * orbit_speed;
            let pitch = 0.3 + (time_secs * 0.15).sin() * 0.15;
            let desired_pos = target
                + Quat::from_rotation_y(yaw)
                    * Vec3::new(pitch.cos() * dist, pitch.sin() * dist + 1.5, 0.0);
            let lerp = 1.0 - (-2.0 * dt).exp();
            let new_pos = camera_transform.translation.lerp(desired_pos, lerp);
            camera_transform.translation = new_pos;
            camera_transform.look_at(look_target, Vec3::Y);
            if let Projection::Perspective(ref mut proj) = *projection {
                proj.fov = 55.0_f32.to_radians() + (time_secs * 0.1).sin() * 2.0_f32.to_radians();
            }
        }
    }

    // Adaptive near/far planes: keep the depth range proportional to the distance to the
    // nearest planet. This bounds the GPU workload (depth precision, light clusters, and
    // culling) when the craft is next to a massive body like the Sun, preventing the
    // pathological frame that previously caused swap-chain/device loss.
    if let Projection::Perspective(ref mut proj) = *projection {
        let camera_pos = camera_transform.translation;
        let mut nearest_distance = f32::MAX;
        for planet_gt in planet_query.iter() {
            let d = camera_pos.distance(planet_gt.translation());
            if d < nearest_distance {
                nearest_distance = d;
            }
        }
        let world_radius = nearest_distance.max(1.0);
        let desired_far = (world_radius * 8.0).clamp(100_000.0, 10_000_000.0);
        let desired_near = (desired_far * 0.00001).clamp(0.1, 1.0);
        let far_lerp = 1.0 - (-2.0 * dt).exp();
        let near_lerp = 1.0 - (-4.0 * dt).exp();
        proj.far = proj.far.lerp(desired_far, far_lerp);
        proj.near = proj.near.lerp(desired_near, near_lerp);
        if proj.near >= proj.far {
            proj.near = proj.far * 0.01;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_sync_copies_but_does_not_source_craft_pose() {
        let mut app = App::new();
        let mut craft = CraftComponent::saucer();
        craft.position = Vec3::new(12.0, 34.0, 56.0);
        craft.orientation = Quat::from_rotation_y(0.5);
        let entity = app
            .world_mut()
            .spawn((craft, Transform::from_xyz(-1.0, -2.0, -3.0)))
            .id();
        app.add_systems(Update, sync_craft_transform);

        app.update();

        let transform = app.world().get::<Transform>(entity).unwrap();
        assert_eq!(transform.translation, Vec3::new(12.0, 34.0, 56.0));
        assert_eq!(transform.rotation, Quat::from_rotation_y(0.5));
    }
}
