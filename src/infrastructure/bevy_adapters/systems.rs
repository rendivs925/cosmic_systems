use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use super::components::*;

// System to update gyroscopes
pub fn update_gyroscopes(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    mut query: Query<(&mut Transform, &mut GyroscopeComponent)>,
) {
    for (mut transform, mut gyro_comp) in query.iter_mut() {
        let spin_axis = Vec3::Y; // Assuming Y axis for spin
        SimulationService::update_gyroscope(&mut gyro_comp.domain_gyro, &params, spin_axis);
        let precession_angle = SimulationService::get_precession_angle(&gyro_comp.domain_gyro, time.delta_seconds());
        transform.rotate_y(precession_angle);
    }
}

// System to update thrust visualization
pub fn update_thrust(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    gyro_query: Query<&GyroscopeComponent>,
    mut arrow_query: Query<&mut Transform, With<ThrustArrow>>,
) {
    let gyros: Vec<_> = gyro_query.iter().map(|g| &g.domain_gyro).collect();
    if gyros.is_empty() {
        return;
    }
    let total_thrust = SimulationService::calculate_thrust(&gyros, &params);

    for mut transform in arrow_query.iter_mut() {
        let scale = crate::domain::services::physics::calculate_arrow_scale(total_thrust);
        transform.scale = Vec3::new(0.1, 0.1, scale);

        if total_thrust.length() > 0.001 {
            let translation = transform.translation;
            let target = translation + total_thrust.normalize();
            transform.look_at(target, Vec3::Y);
        }

        transform.scale *= 1.0 + 0.1 * (time.elapsed_seconds() * 5.0).sin();
    }
}

// System to handle user input for controlling simulation parameters
pub fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut params: ResMut<SimulationParameters>,
    time: Res<Time>,
) {
    let rpm_delta = 5000.0 * time.delta_seconds(); // Adjust RPM by 5000 per second

    if keyboard.pressed(KeyCode::ArrowUp) {
        params.rpm += rpm_delta;
        println!("RPM increased to: {:.0}", params.rpm);
    }
    if keyboard.pressed(KeyCode::ArrowDown) {
        params.rpm -= rpm_delta;
        if params.rpm < 0.0 {
            params.rpm = 0.0;
        }
        println!("RPM decreased to: {:.0}", params.rpm);
    }

    // Optional: Add controls for other parameters
    let param_delta = 10.0 * time.delta_seconds();
    if keyboard.pressed(KeyCode::KeyW) {
        params.precession_hz += param_delta;
        println!("Precession Hz increased to: {:.1}", params.precession_hz);
    }
    if keyboard.pressed(KeyCode::KeyS) {
        params.precession_hz -= param_delta;
        if params.precession_hz < 0.0 {
            params.precession_hz = 0.0;
        }
        println!("Precession Hz decreased to: {:.1}", params.precession_hz);
    }

    if keyboard.pressed(KeyCode::KeyA) {
        params.asymmetry -= param_delta * 0.1;
        params.asymmetry = params.asymmetry.clamp(0.0, 1.0);
        println!("Asymmetry decreased to: {:.2}", params.asymmetry);
    }
    if keyboard.pressed(KeyCode::KeyD) {
        params.asymmetry += param_delta * 0.1;
        params.asymmetry = params.asymmetry.clamp(0.0, 1.0);
        println!("Asymmetry increased to: {:.2}", params.asymmetry);
    }
}

// System to update planet positions in their orbits
pub fn update_planet_positions(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let new_position = physics::calculate_planet_position(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
        );
        transform.translation = new_position;
    }
}

// System to update planet rotations
pub fn update_planet_rotations(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let rotation_angle = physics::calculate_planet_rotation(
            &planet_comp.domain_planet,
            time_days,
        );

        // Rotate around the planet's local Y axis (for simplicity)
        // In reality, planets have different rotation axes, but this works for visualization
        transform.rotation = Quat::from_rotation_y(rotation_angle);
    }
}

// System to handle planet selection
pub fn handle_planet_selection(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
) {
    let mut selection_changed = false;
    let mut new_selected_entity = selected_planet.entity;
    let mut new_selected_name = selected_planet.name.clone();

    // Cycle through planets with Tab key
    if keyboard.just_pressed(KeyCode::Tab) {
        // Collect all selectable entities first
        let all_entities: Vec<Entity> = selectable_query.iter().map(|(entity, _)| entity).collect();

        if all_entities.is_empty() {
            return;
        }

        // Find current selection index
        let current_index = if let Some(current_entity) = selected_planet.entity {
            all_entities.iter().position(|&entity| entity == current_entity).unwrap_or(0)
        } else {
            0
        };

        // Move to next planet (wrap around)
        let next_index = (current_index + 1) % all_entities.len();
        let next_entity = all_entities[next_index];

        // Get the name from the entity (we'll need to query again, but this avoids borrowing issues)
        if let Ok((_, selectable)) = selectable_query.get(next_entity) {
            new_selected_entity = Some(next_entity);
            new_selected_name = Some(selectable.name.clone());
            selection_changed = true;
            println!("Selected planet: {}", selectable.name);
        }
    }

    // Deselect with Escape
    if keyboard.just_pressed(KeyCode::Escape) {
        new_selected_entity = None;
        new_selected_name = None;
        selection_changed = true;
        println!("Deselected planet");
    }

    // Update selection resource
    if selection_changed {
        selected_planet.entity = new_selected_entity;
        selected_planet.name = new_selected_name;

        // Update all selectable components
        let target_entity = selected_planet.entity;
        for (entity, mut selectable) in selectable_query.iter_mut() {
            selectable.selected = Some(entity) == target_entity;
        }
    }
}

// System to handle mouse clicking for planet selection
pub fn handle_mouse_planet_selection(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable, &GlobalTransform)>,
) {
    // Only handle left mouse button clicks
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    // Get camera transform
    let camera_transform = camera_query.single();

    // Simple distance-based selection: find closest planet to camera
    let mut closest_entity: Option<Entity> = None;
    let mut closest_distance = f32::INFINITY;

    for (entity, _selectable, transform) in selectable_query.iter() {
        let planet_pos = transform.translation();
        let camera_pos = camera_transform.translation();
        let distance = (planet_pos - camera_pos).length();

        // If this planet is closer and in a reasonable range, select it
        if distance < closest_distance && distance < 1000.0 { // Max selection distance
            closest_distance = distance;
            closest_entity = Some(entity);
        }
    }

    // Update selection
    if let Some(selected_entity) = closest_entity {
        if let Ok((_, selectable, _)) = selectable_query.get(selected_entity) {
            selected_planet.entity = Some(selected_entity);
            selected_planet.name = Some(selectable.name.clone());
            println!("Selected planet: {}", selectable.name);
        }
    } else {
        // Clicked on empty space - deselect
        selected_planet.entity = None;
        selected_planet.name = None;
        println!("Deselected planet");
    }

    // Update all selectable components
    let target_entity = selected_planet.entity;
    for (_, mut selectable, _) in selectable_query.iter_mut() {
        selectable.selected = false; // Reset all first
    }
    if let Some(entity) = target_entity {
        if let Ok((_, mut selectable, _)) = selectable_query.get_mut(entity) {
            selectable.selected = true;
        }
    }
}

// System to update visual feedback for selected planets
pub fn update_planet_selection_visuals(
    time: Res<Time>,
    mut query: Query<(&Selectable, &mut Transform)>,
) {
    let pulse = (time.elapsed_seconds() * 3.0).sin() * 0.1 + 1.0; // Gentle pulsing effect

    for (selectable, mut transform) in query.iter_mut() {
        if selectable.selected {
            // Make selected planet slightly larger with pulsing effect
            transform.scale = Vec3::splat(pulse);
        } else {
            // Reset scale for unselected planets
            transform.scale = Vec3::ONE;
        }
    }
}

// System to handle solar system controls (time scale, etc.)
pub fn handle_solar_system_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut solar_params: ResMut<SolarSystemParameters>,
) {
    // Time scale controls
    if keyboard.pressed(KeyCode::KeyT) {
        solar_params.time_scale *= 1.1;
        println!("Time scale: {:.1}x", solar_params.time_scale);
    }
    if keyboard.pressed(KeyCode::KeyR) && solar_params.time_scale > 0.1 {
        solar_params.time_scale /= 1.1;
        println!("Time scale: {:.1}x", solar_params.time_scale);
    }

    // Reset time scale
    if keyboard.pressed(KeyCode::KeyY) {
        solar_params.time_scale = 1.0;
        println!("Time scale reset to: {:.1}x", solar_params.time_scale);
    }

    // Toggle orbit visualization (placeholder for future feature)
    if keyboard.just_pressed(KeyCode::KeyO) {
        solar_params.show_orbits = !solar_params.show_orbits;
        println!("Orbit visualization: {}", if solar_params.show_orbits { "ON" } else { "OFF" });
    }
}

// System to update camera controller based on input
pub fn update_camera_controller(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut query: Query<(&mut CameraController, &mut Transform)>,
) {
    for (mut controller, mut transform) in query.iter_mut() {
        if controller.mode != CameraMode::FreeFlight {
            continue; // Only handle input for free flight mode for now
        }

        let dt = time.delta_seconds();

        // Handle mouse look for rotation (right mouse button held)
        let mut mouse_delta = Vec2::ZERO;
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }

        // Apply mouse sensitivity and update rotation
        if mouse_delta != Vec2::ZERO && mouse_buttons.pressed(MouseButton::Right) {
            let sensitivity = controller.sensitivity;
            let yaw = -mouse_delta.x * sensitivity;
            let pitch = -mouse_delta.y * sensitivity;

            // Apply rotation to camera transform
            transform.rotate_y(yaw);
            let right = *transform.right();
            transform.rotate_axis(bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X), pitch);

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler.1.clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, euler.0, clamped_pitch, euler.2);
        }

        // Handle keyboard movement (relative to camera orientation)
        let mut movement = Vec3::ZERO;

        if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
            movement += *transform.forward();
        }
        if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
            movement -= *transform.forward();
        }
        if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
            movement -= *transform.right();
        }
        if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
            movement += *transform.right();
        }
        if keyboard.pressed(KeyCode::Space) {
            movement += Vec3::Y;
        }
        if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight) {
            movement -= Vec3::Y;
        }

        // Handle mouse wheel for zooming
        let mut zoom_factor = 1.0;
        for wheel_event in mouse_wheel.read() {
            // Mouse wheel creates zoom based on y delta (scroll up = zoom in, scroll down = zoom out)
            zoom_factor *= (1.0 - wheel_event.y * 0.1).clamp(0.1, 10.0);
        }

        // Apply zoom by moving camera forward/backward along its look direction
        if zoom_factor != 1.0 {
            let forward = *transform.forward();
            let zoom_movement = forward * (zoom_factor - 1.0) * controller.speed * 2.0; // Zoom speed multiplier
            movement += zoom_movement;
        }

        // Apply speed
        if movement != Vec3::ZERO {
            movement = movement.normalize() * controller.speed;
            if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                movement *= 5.0; // Speed boost
            }
        }

        // Apply movement to velocity with damping
        controller.velocity += movement * dt;
        controller.velocity *= 0.9; // Velocity damping for smooth movement
    }
}

// System to apply camera transformations based on controller state
pub fn apply_camera_transform(
    time: Res<Time>,
    mut query: Query<(&mut CameraController, &mut Transform)>,
) {
    for (mut controller, mut transform) in query.iter_mut() {
        match controller.mode {
            CameraMode::FreeFlight => {
                // Apply velocity to position (rotation is handled in input system for mouse look)
                let dt = time.delta_seconds();
                transform.translation += controller.velocity * dt;
            }
            CameraMode::Orbit => {
                // Orbit around the solar system center
                controller.orbit_angle += time.delta_seconds() * 0.5;
                let orbit_pos = Vec3::new(
                    controller.orbit_distance * controller.orbit_angle.cos(),
                    10.0, // Slight elevation
                    controller.orbit_distance * controller.orbit_angle.sin(),
                );
                transform.translation = orbit_pos;
                transform.look_at(Vec3::ZERO, Vec3::Y);
            }
            CameraMode::FollowPlanet => {
                // Follow a specific planet (placeholder)
                // Would need to track the target entity's position
            }
            CameraMode::ApproachPlanet => {
                // Approach a planet (placeholder)
                // Would smoothly interpolate toward target
            }
        }
    }
}