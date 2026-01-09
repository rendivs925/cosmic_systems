use super::components::*;
use super::entity_components::Starfield;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;

// System to update camera controller based on input
pub fn update_camera_controller(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: MessageReader<MouseMotion>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    ui_state: Res<UiPointerState>,
    selected_planet: Res<SelectedPlanet>,
    mut query: Query<(&mut CameraController, &mut Transform)>,
    mut input_state: ResMut<CameraInputState>,
    mut notifications: ResMut<NotificationQueue>,
) {
    // Check if UI is using the mouse (hovering over UI)
    let ui_has_pointer = ui_state.is_over_ui;
    for (mut controller, mut transform) in query.iter_mut() {
        if controller.mode != CameraMode::FreeFlight {
            continue; // Only handle input for free flight mode for now
        }

        let dt = time.delta_secs();
        let mut user_input = false;

        // Handle mouse look for rotation
        let mut mouse_delta = Vec2::ZERO;
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }

        // Apply mouse sensitivity and update rotation only when left mouse is held.
        if mouse_delta != Vec2::ZERO && mouse_buttons.pressed(MouseButton::Left) {
            user_input = true;
            let sensitivity = controller.sensitivity;
            let yaw = -mouse_delta.x * sensitivity;
            let pitch = -mouse_delta.y * sensitivity;

            // Apply rotation to camera transform
            transform.rotate_y(yaw);
            let right = *transform.right();
            transform.rotate_axis(
                bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X),
                pitch,
            );

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler
                .1
                .clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, euler.0, clamped_pitch, euler.2);
        }

        // Handle keyboard rotation (cursor keys for looking around)
        let mut rotation_delta = Vec2::ZERO;
        if keyboard.pressed(KeyCode::ArrowUp) {
            rotation_delta.y -= 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowDown) {
            rotation_delta.y += 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowLeft) {
            rotation_delta.x -= 1.0;
        }
        if keyboard.pressed(KeyCode::ArrowRight) {
            rotation_delta.x += 1.0;
        }

        // Apply keyboard-based rotation
        if rotation_delta != Vec2::ZERO {
            user_input = true;
            let key_sensitivity = controller.sensitivity * 50.0; // Keyboard rotation sensitivity
            let yaw = -rotation_delta.x * key_sensitivity;
            let pitch = -rotation_delta.y * key_sensitivity;

            // Apply rotation to camera transform
            transform.rotate_y(yaw);
            let right = *transform.right();
            transform.rotate_axis(
                bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X),
                pitch,
            );

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler
                .1
                .clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
            transform.rotation = Quat::from_euler(EulerRot::YXZ, euler.0, clamped_pitch, euler.2);
        }

        // Handle keyboard movement - Full 3D spaceship-style controls
        let mut movement = Vec3::ZERO;

        // Primary movement (WASD + Space/Ctrl)
        if keyboard.pressed(KeyCode::KeyW) {
            movement += *transform.forward(); // Forward
        }
        if keyboard.pressed(KeyCode::KeyS) {
            movement -= *transform.forward(); // Backward
        }
        if keyboard.pressed(KeyCode::KeyA) {
            movement -= *transform.right(); // Strafe left
        }
        if keyboard.pressed(KeyCode::KeyD) {
            movement += *transform.right(); // Strafe right
        }

        // Vertical movement (multiple options for flexibility)
        if keyboard.pressed(KeyCode::Space) || keyboard.pressed(KeyCode::KeyQ) {
            movement += Vec3::Y; // Up
        }
        if keyboard.pressed(KeyCode::KeyE) {
            movement -= Vec3::Y; // Down
        }

        // Alternative movement controls for enhanced 3D navigation
        // Arrow keys provide additional movement options
        if keyboard.pressed(KeyCode::ArrowUp) && !keyboard.pressed(KeyCode::KeyW) {
            movement += *transform.forward() * 0.7; // Slower forward with arrows
        }
        if keyboard.pressed(KeyCode::ArrowDown) && !keyboard.pressed(KeyCode::KeyS) {
            movement -= *transform.forward() * 0.7; // Slower backward with arrows
        }

        // Allow free movement in any direction by combining controls
        // This enables full 6DOF (degrees of freedom) movement

        // Additional zoom controls with keyboard for precise control
        let zoom_speed = controller.speed * 12.0; // Smoother keyboard zoom for astronomical navigation
        if keyboard.pressed(KeyCode::Equal) || keyboard.pressed(KeyCode::NumpadAdd) {
            // Zoom in with = or numpad +
            let forward = *transform.forward();
            movement += forward * zoom_speed;
        }
        if keyboard.pressed(KeyCode::Minus) || keyboard.pressed(KeyCode::NumpadSubtract) {
            // Zoom out with - or numpad -
            let forward = *transform.forward();
            movement -= forward * zoom_speed;
        }

        // Handle mouse wheel for zooming and speed adjustment (only if not over UI)
        if !ui_has_pointer {
            for wheel_event in mouse_wheel.read() {
                user_input = true;
                if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight)
                {
                    // Ctrl+Wheel: Adjust base movement speed
                    let speed_change = wheel_event.y * controller.speed * 0.15;
                    controller.speed = (controller.speed + speed_change)
                        .clamp(controller.min_speed, controller.max_speed);
                    println!("Camera speed: {:.0} units/s", controller.speed);
                } else if keyboard.pressed(KeyCode::ShiftLeft)
                    || keyboard.pressed(KeyCode::ShiftRight)
                {
                    // Shift+Wheel: Adjust zoom sensitivity
                    let sensitivity_change = wheel_event.y * 5.0;
                    controller.zoom_sensitivity =
                        (controller.zoom_sensitivity + sensitivity_change).clamp(0.1, 500.0);

                    // Show notification with current zoom sensitivity
                    notifications.notifications.push(Notification {
                        message: format!("Zoom Sensitivity: {:.1}", controller.zoom_sensitivity),
                        notification_type: NotificationType::Info,
                        created_at: time.elapsed_secs(),
                        duration: 2.0,
                    });
                } else {
                    // Normal wheel: Zoom in/out
                    let zoom_distance =
                        wheel_event.y * controller.speed * controller.zoom_sensitivity;
                    let forward = *transform.forward();
                    transform.translation += forward * zoom_distance;
                }
            }
        } else {
            // Clear wheel events when over UI to prevent them from being processed
            mouse_wheel.clear();
        }

        // Apply speed with multiple speed options for better 3D navigation
        if movement != Vec3::ZERO {
            movement = movement.normalize() * controller.speed;

            // Speed modifiers for flexible 3D movement
            if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                movement *= 5.0; // Fast mode - quick travel between planets
            } else if keyboard.pressed(KeyCode::AltLeft) || keyboard.pressed(KeyCode::AltRight) {
                movement *= 0.2; // Slow mode - precise positioning near objects
            }

            // Allow free movement in any 3D direction without normalization constraints
            // This enables smooth, intuitive spaceship-like movement
        }

        // Smooth acceleration/deceleration for better control
        let target_velocity = movement;
        let accel_rate = if target_velocity.length() > controller.velocity.length() {
            controller.acceleration
        } else {
            controller.deceleration
        };

        controller.velocity = controller.velocity.lerp(target_velocity, dt * accel_rate);

        // Apply damping to stop completely when no input
        if movement == Vec3::ZERO && controller.velocity.length() < 1.0 {
            controller.velocity = Vec3::ZERO;
        }

        if movement != Vec3::ZERO {
            user_input = true;
        }

        if user_input {
            input_state.last_input_time = time.elapsed_secs();
            if let Some(entity) = selected_planet.entity {
                input_state.suppress_auto_inspect_for = Some(entity);
            }
        }
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
        let dt = time.delta_secs();
                transform.translation += controller.velocity * dt;
            }
            CameraMode::Orbit => {
                // Orbit around the solar system center
                controller.orbit_angle += time.delta_secs() * 0.5;
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
            CameraMode::TerrainView => {
                // Ground-level terrain exploration
                // Use free flight controls but constrain to terrain surface
                let dt = time.delta_secs();
                transform.translation += controller.velocity * dt;
                // Keep camera above terrain (simplified - would need terrain height sampling)
                if transform.translation.y < 5.0 {
                    transform.translation.y = 5.0;
                }
            }
        }
    }
}

// System to auto-inspect a selected planet by smoothly framing it at a readable distance
pub fn auto_inspect_selected_planet(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    mut input_state: ResMut<CameraInputState>,
    mut camera_query: Query<(&mut CameraController, &mut Transform, &Projection)>,
    planet_query: Query<(&PlanetComponent, &GlobalTransform)>,
    mut state: Local<AutoInspectState>,
) {
    let selected_entity = match selected_planet.entity {
        Some(entity) => entity,
        None => return,
    };

    let (planet_comp, planet_transform) = match planet_query.get(selected_entity) {
        Ok(data) => data,
        Err(_) => return,
    };

    let (mut controller, mut camera_transform, projection) = match camera_query.single_mut() {
        Ok(data) => data,
        Err(_) => return,
    };

    // Handle Earth terrain view with double-click detection
    if planet_comp.domain_planet.name == "Earth" {
        // Track double-clicks for Earth (within 0.3 seconds)
        let current_time = time.elapsed_secs();
        if let Some(last_earth_click) = state.last_earth_click {
            if current_time - last_earth_click < 0.3 {
                // Double-click detected - switch to terrain view
                if controller.mode != CameraMode::TerrainView {
                    controller.mode = CameraMode::TerrainView;
                    // Position camera at terrain level
                    camera_transform.translation = Vec3::new(0.0, 100.0, 0.0); // Above Kennedy Space Center
                    camera_transform.look_at(Vec3::ZERO, Vec3::Y);
                }
                state.last_earth_click = None; // Reset to prevent triple-clicks
                return; // Skip orbital inspection for terrain view
            }
        }
        // Single click - record time and continue with orbital inspection
        state.last_earth_click = Some(current_time);

        // If currently in terrain view, switch back to orbital view on single click
        if controller.mode == CameraMode::TerrainView {
            controller.mode = CameraMode::FreeFlight;
        }
    } else {
        // Reset Earth double-click timer for non-Earth planets
        state.last_earth_click = None;

        // Use orbital inspection for other planets
        if controller.mode == CameraMode::TerrainView {
            controller.mode = CameraMode::FreeFlight;
        }
    }

    // if controller.mode != CameraMode::FreeFlight {
    //     return;
    // }

    // Always reset suppression when a planet is selected (makes it idempotent)
    if selected_planet.entity.is_some() {
        input_state.last_selected_entity = selected_planet.entity;
        input_state.suppress_auto_inspect_for = None;
    }

    if input_state.suppress_auto_inspect_for == Some(selected_entity) {
        return;
    }

    let planet_radius = if planet_comp.domain_planet.name == "Sun" {
        physics::calculate_sun_visual_radius(&solar_params)
    } else {
        physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
    };

    let planet_pos = planet_transform.translation();
    let mut focus_point = planet_pos;
    let mut target_distance = planet_radius * 5.0;
    let mut moon_axis: Option<Vec3> = None;
    let mut moon_up: Option<Vec3> = None;
    let mut moon_distance: Option<f32> = None;
    let is_moon = planet_comp.domain_planet.parent_entity.is_some();

    let fov_y = match projection {
        Projection::Perspective(perspective) => perspective.fov,
        Projection::Orthographic(_) => std::f32::consts::FRAC_PI_2,
        Projection::Custom(_) => std::f32::consts::FRAC_PI_2, // Default to orthographic-like FOV
    };
    let fit_radius = |radius: f32, fill: f32| -> f32 {
        let half_fov = (fov_y * 0.5 * fill).max(0.05);
        radius / half_fov.sin()
    };

    if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
        for (other_comp, other_transform) in planet_query.iter() {
            if other_comp.domain_planet.name == *parent_name {
                let parent_pos = other_transform.translation();
                let axis = parent_pos - planet_pos;
                let axis_dir = if axis.length_squared() > 0.0 {
                    axis.normalize()
                } else {
                    Vec3::Z
                };
                let mut lateral = axis_dir.cross(Vec3::Y);
                if lateral.length_squared() < 1e-4 {
                    lateral = axis_dir.cross(Vec3::X);
                }
                let lateral = lateral.normalize();
                let up = lateral.cross(axis_dir).normalize();

                moon_axis = Some(axis_dir);
                moon_up = Some(up);

                let parent_radius = if other_comp.domain_planet.name == "Sun" {
                    physics::calculate_sun_visual_radius(&solar_params)
                } else {
                    physics::calculate_visual_radius(&other_comp.domain_planet, &solar_params)
                };
                let size_ratio = (parent_radius / planet_radius).clamp(1.2, 50.0);
                let fill = (0.78 - size_ratio.log10() * 0.04).clamp(0.62, 0.78);
                let desired_distance = fit_radius(planet_radius, fill);
                let min_distance = (planet_radius * 3.2).max(120.0);
                target_distance = desired_distance.max(min_distance);
                moon_distance = Some(target_distance);
                break;
            }
        }
    }

    // Initialize or reset state when selection changes
    if state.selected != Some(selected_entity) {
        state.selected = Some(selected_entity);
        state.orbit_angle = 0.0;
        // Start with a nice 3/4 view angle
        state.orbit_elevation = 0.3; // 30 degrees up
        state.smooth_axis = Vec3::ZERO;
        state.smooth_up = Vec3::ZERO;
        state.smooth_focus = Vec3::ZERO;
        state.smooth_offset = Vec3::ZERO;
    }

    // Cinematic slow orbit around the planet for aesthetic viewing
    if !is_moon {
        state.orbit_angle += time.delta_secs() * 0.15; // Slow orbit
    }

    if let (Some(axis_dir), Some(up)) = (moon_axis, moon_up) {
        // Frame the moon large in the foreground with the parent in the background.
        let distance = moon_distance.unwrap_or(target_distance);
        let smooth_lerp = 1.0 - (-3.0 * time.delta_secs()).exp();
        state.smooth_axis = if state.smooth_axis.length_squared() > 0.0 {
            state
                .smooth_axis
                .lerp(axis_dir, smooth_lerp)
                .normalize_or_zero()
        } else {
            axis_dir
        };
        state.smooth_up = if state.smooth_up.length_squared() > 0.0 {
            state.smooth_up.lerp(up, smooth_lerp).normalize_or_zero()
        } else {
            up
        };

        let smooth_axis = state.smooth_axis;
        let smooth_up = state.smooth_up;
        let mut smooth_lateral = smooth_axis.cross(Vec3::Y);
        if smooth_lateral.length_squared() < 1e-4 {
            smooth_lateral = smooth_axis.cross(Vec3::X);
        }
        let smooth_lateral = smooth_lateral.normalize();

        let rotation = Quat::from_axis_angle(smooth_axis, state.orbit_angle * 0.06);
        let side_offset =
            rotation * (smooth_up * (distance * 0.35) + smooth_lateral * (distance * 0.12));
        state.offset = (-smooth_axis * distance) + side_offset;
        focus_point = planet_pos;
    } else {
        // Get aesthetic viewing angle based on planet type
        let (orbit_distance, elevation_offset) =
            get_aesthetic_view_params(&planet_comp.domain_planet.name);
        let actual_distance = target_distance * orbit_distance;
        let elevation = state.orbit_elevation + elevation_offset;

        // Calculate cinematic orbit position (3/4 view with elevation)
        let horizontal = Vec3::new(
            state.orbit_angle.cos() * actual_distance,
            0.0,
            state.orbit_angle.sin() * actual_distance,
        );
        let elevated = horizontal + Vec3::Y * (actual_distance * elevation);
        state.offset = elevated;
    }

    // Ultra-cinematic interpolation - buttery smooth elegance
    let smooth_factor = if is_moon {
        1.0 - (-3.5 * time.delta_secs()).exp() // Refined smoothness for moons
    } else {
        1.0 - (-5.5 * time.delta_secs()).exp() // Premium smoothness for planets
    };
    if state.smooth_focus.length_squared() > 0.0 {
        state.smooth_focus = state.smooth_focus.lerp(focus_point, smooth_factor);
    } else {
        state.smooth_focus = focus_point;
    }
    if state.smooth_offset.length_squared() > 0.0 {
        state.smooth_offset = state.smooth_offset.lerp(state.offset, smooth_factor);
    } else {
        state.smooth_offset = state.offset;
    }

    let target_pos = state.smooth_focus + state.smooth_offset;
    let lerp_factor = 1.0 - (-2.5 * time.delta_secs()).exp(); // Smoother transitions
    camera_transform.translation = camera_transform.translation.lerp(target_pos, lerp_factor);

    // Look at the focus point to frame moon + parent when applicable
    camera_transform.look_at(state.smooth_focus, Vec3::Y);
}

// Get aesthetic viewing parameters for different celestial bodies
fn get_aesthetic_view_params(name: &str) -> (f32, f32) {
    // Returns (distance_multiplier, elevation_offset)
    match name {
        "Sun" => (1.2, 0.2),      // Further back, slightly elevated for the massive sun
        "Saturn" => (1.15, 0.3),  // Extra elevation to showcase rings
        "Jupiter" => (1.1, 0.25), // Show off the gas giant with nice elevation
        "Earth" | "Mars" => (0.95, 0.35), // Closer, higher angle for detail
        "Moon" | "Io" | "Europa" | "Titan" | "Triton" => (0.95, 0.35), // Keep close while showing parent
        "Neptune" | "Uranus" => (1.0, 0.3), // Ice giants with good elevation
        _ => (1.0, 0.3),                    // Default: standard distance, nice 3/4 view
    }
}

#[derive(Default)]
pub struct AutoInspectState {
    selected: Option<Entity>,
    offset: Vec3,
    orbit_angle: f32,     // Cinematic orbit angle
    orbit_elevation: f32, // Vertical orbit component
    smooth_axis: Vec3,
    smooth_up: Vec3,
    smooth_focus: Vec3,
    smooth_offset: Vec3,
    last_earth_click: Option<f32>, // Timestamp of last Earth click for double-click detection
}

// System to keep starfield positioned relative to camera for constant visibility
pub fn update_starfield_position(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut starfield_query: Query<&mut Transform, With<Starfield>>,
) {
    if let Ok(camera_transform) = camera_query.single() {
        let camera_pos = camera_transform.translation();

        for mut starfield_transform in starfield_query.iter_mut() {
            // Keep starfield centered on camera position
            // This ensures stars are always visible regardless of camera movement
            starfield_transform.translation = camera_pos;
        }
    }
}