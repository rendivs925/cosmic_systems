use super::components::*;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};
use std::collections::HashMap;

fn normalized_or_zero(vec: Vec3) -> Vec3 {
    if vec.length_squared() > 0.0 {
        vec.normalize()
    } else {
        Vec3::ZERO
    }
}

// System to update gyroscopes
pub fn update_gyroscopes(
    time: Res<Time>,
    params: Res<SimulationParameters>,
    mut query: Query<(&mut Transform, &mut GyroscopeComponent)>,
) {
    for (mut transform, mut gyro_comp) in query.iter_mut() {
        let spin_axis = Vec3::Y; // Assuming Y axis for spin
        SimulationService::update_gyroscope(&mut gyro_comp.domain_gyro, &params, spin_axis);
        let precession_angle =
            SimulationService::get_precession_angle(&gyro_comp.domain_gyro, time.delta_seconds());
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

// System to update planet/moon positions in their orbits (optimized for performance)
pub fn update_planet_positions(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    // Get camera position for distance culling (using GlobalTransform to avoid conflicts)
    let camera_global = camera_query.single();
    let camera_pos = camera_global.translation();

    // First pass: collect parent positions
    let mut parent_positions = std::collections::HashMap::new();
    for (transform, planet_comp) in query.iter() {
        parent_positions.insert(
            planet_comp.domain_planet.name.clone(),
            transform.translation,
        );
    }

    // Second pass: update positions with distance-based optimization
    for (mut transform, planet_comp) in query.iter_mut() {
        // Distance culling: only update objects within reasonable range of camera
        let distance_to_camera = camera_pos.distance(transform.translation);
        let max_update_distance = 2000000.0; // Update all objects within 2M units (covers entire solar system)

        if distance_to_camera > max_update_distance {
            // Skip updating distant objects for performance
            continue;
        }

        // Find parent position
        let parent_position = if let Some(parent_name) = &planet_comp.domain_planet.parent_entity {
            // This is a moon - get its parent planet's position
            *parent_positions.get(parent_name).unwrap_or(&Vec3::ZERO)
        } else {
            // This is a planet orbiting the Sun
            Vec3::ZERO
        };

        let new_position = physics::calculate_planet_position(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
            parent_position,
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
        let rotation_angle =
            physics::calculate_planet_rotation(&planet_comp.domain_planet, time_days);

        // Rotate around the planet's local Y axis (for simplicity)
        // In reality, planets have different rotation axes, but this works for visualization
        transform.rotation = Quat::from_rotation_y(rotation_angle);
    }
}

// System to animate orbit visuals for a more dynamic presentation
pub fn update_orbit_visuals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: Query<(&mut Transform, &OrbitComponent)>,
) {
    let elapsed = time.elapsed_seconds();

    for (mut transform, orbit) in query.iter_mut() {
        let wobble_phase = elapsed * orbit.wobble_speed + orbit.phase;
        let pitch = orbit.tilt.x + wobble_phase.sin() * orbit.wobble_amount;
        let roll = orbit.tilt.y + (wobble_phase * 1.3).cos() * orbit.wobble_amount;
        let yaw = elapsed * orbit.spin_speed + orbit.phase * 0.2;

        transform.rotation = Quat::from_euler(EulerRot::XYZ, pitch, yaw, roll);
        let scale_pulse = 1.0 + (wobble_phase * 0.7).sin() * 0.008;
        transform.scale = Vec3::splat(scale_pulse);

        if let Some(material) = materials.get_mut(&orbit.material) {
            let pulse = 0.5 + 0.5 * (wobble_phase * 0.6).sin();
            let alpha = (0.12 + 0.18 * pulse).clamp(0.08, 0.35);
            material.base_color = orbit.base_color.with_alpha(alpha);
            let linear: LinearRgba = orbit.base_color.into();
            let glow = 0.35 + 0.4 * pulse;
            material.emissive = LinearRgba::new(
                linear.red * glow,
                linear.green * glow,
                linear.blue * glow,
                1.0,
            );
        }
    }
}

// System to add dynamic specular reflection response for planet materials
pub fn update_planet_reflections(
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    for (planet_comp, _global_transform) in query.iter() {
        if planet_comp.domain_planet.name == "Sun" {
            continue;
        }
        if let Some(material) = materials.get_mut(&planet_comp.material) {
            material.perceptual_roughness = planet_comp.base_roughness;
            material.reflectance = planet_comp.base_reflectance;
        }
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
            all_entities
                .iter()
                .position(|&entity| entity == current_entity)
                .unwrap_or(0)
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
    camera_query: Query<(&Camera, &GlobalTransform), With<CameraController>>,
    windows: Query<&Window>,
    solar_params: Res<SolarSystemParameters>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable, &PlanetComponent, &GlobalTransform)>,
) {
    // Only handle left mouse button clicks
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let (camera, camera_transform) = camera_query.single();
    let window = match windows.get_single() {
        Ok(window) => window,
        Err(_) => return,
    };
    let cursor_pos = match window.cursor_position() {
        Some(pos) => pos,
        None => return,
    };
    let ray = match camera.viewport_to_world(camera_transform, cursor_pos) {
        Some(ray) => ray,
        None => return,
    };

    // Raycast against planet spheres to find the clicked body.
    let mut closest_entity: Option<Entity> = None;
    let mut closest_t = f32::INFINITY;

    for (entity, _selectable, planet_comp, transform) in selectable_query.iter() {
        let radius = if planet_comp.domain_planet.name == "Sun" {
            physics::calculate_sun_visual_radius(&solar_params)
        } else {
            physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
        };
        let center = transform.translation();
        let oc = ray.origin - center;
        let b = 2.0 * oc.dot(*ray.direction);
        let c = oc.length_squared() - radius * radius;
        let discriminant = b * b - 4.0 * c;
        if discriminant < 0.0 {
            continue;
        }
        let t = (-b - discriminant.sqrt()) * 0.5;
        if t > 0.0 && t < closest_t {
            closest_t = t;
            closest_entity = Some(entity);
        }
    }

    // Update selection
    if let Some(selected_entity) = closest_entity {
        if let Ok((_, selectable, _, _)) = selectable_query.get(selected_entity) {
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
    for (_, mut selectable, _, _) in selectable_query.iter_mut() {
        selectable.selected = false; // Reset all first
    }
    if let Some(entity) = target_entity {
        if let Ok((_, mut selectable, _, _)) = selectable_query.get_mut(entity) {
            selectable.selected = true;
        }
    }
}

// System to update visual feedback for selected planets (optimized)
pub fn update_planet_selection_visuals(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(&Selectable, &mut Transform, &GlobalTransform)>,
) {
    let camera_pos = camera_query.single().translation();

    for (selectable, mut transform, global_transform) in query.iter_mut() {
        // Distance culling for visual updates
        let distance_to_camera = (global_transform.translation() - camera_pos).length();
        let max_visual_distance = 30000.0; // Only update visuals for reasonably close objects

        if distance_to_camera > max_visual_distance {
            // Reset scale for distant unselected objects
            transform.scale = Vec3::ONE;
            continue;
        }

        // Keep scale fixed regardless of selection.
        transform.scale = Vec3::ONE;
    }
}

// System to show a navigation bar for quick planet selection
pub fn display_navigation_bar(
    mut contexts: EguiContexts,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable)>,
) {
    let mut name_to_entity = HashMap::new();
    {
        for (entity, selectable) in selectable_query.iter_mut() {
            name_to_entity.insert(selectable.name.clone(), entity);
        }
    }

    let ordered_names = [
        "Sun",
        "Mercury",
        "Venus",
        "Earth",
        "Moon",
        "Mars",
        "Phobos",
        "Deimos",
        "Jupiter",
        "Io",
        "Europa",
        "Ganymede",
        "Callisto",
        "Saturn",
        "Mimas",
        "Enceladus",
        "Tethys",
        "Dione",
        "Rhea",
        "Titan",
        "Hyperion",
        "Iapetus",
        "Uranus",
        "Miranda",
        "Ariel",
        "Umbriel",
        "Titania",
        "Oberon",
        "Neptune",
        "Triton",
        "Proteus",
        "Nereid",
        "Larissa",
    ];

    let ctx = contexts.ctx_mut();
    let mut selection_changed = false;

    egui::TopBottomPanel::top("celestial_nav")
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Navigate:");
                egui::ScrollArea::horizontal()
                    .id_source("celestial_nav_scroll")
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for name in ordered_names {
                                if !name_to_entity.contains_key(name) {
                                    continue;
                                }
                                let selected = selected_planet
                                    .name
                                    .as_ref()
                                    .map(|selected_name| selected_name == name)
                                    .unwrap_or(false);
                                let button = egui::SelectableLabel::new(selected, name);
                                if ui.add(button).clicked() {
                                    if let Some(entity) = name_to_entity.get(name) {
                                        selected_planet.entity = Some(*entity);
                                        selected_planet.name = Some(name.to_string());
                                        selection_changed = true;
                                    }
                                }
                            }
                        });
                    });
            });
        });

    if selection_changed {
        let target_entity = selected_planet.entity;
        for (entity, mut selectable) in selectable_query.iter_mut() {
            selectable.selected = Some(entity) == target_entity;
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
        println!(
            "Orbit visualization: {}",
            if solar_params.show_orbits {
                "ON"
            } else {
                "OFF"
            }
        );
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

        // Handle mouse look for rotation
        let mut mouse_delta = Vec2::ZERO;
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }

        // Apply mouse sensitivity and update rotation only when left mouse is held.
        if mouse_delta != Vec2::ZERO && mouse_buttons.pressed(MouseButton::Left) {
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
        if keyboard.pressed(KeyCode::ControlLeft)
            || keyboard.pressed(KeyCode::ControlRight)
            || keyboard.pressed(KeyCode::KeyE)
        {
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

        // Handle mouse wheel for zooming (direct position change, not velocity-based)
        for wheel_event in mouse_wheel.read() {
            let zoom_multiplier =
                if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                    1.5
                } else {
                    1.0
                };
            let zoom_distance = wheel_event.y * controller.speed * 220.0 * zoom_multiplier;
            let forward = *transform.forward();
            transform.translation += forward * zoom_distance;
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

        // Apply movement to velocity with damping
        controller.velocity += movement * dt;
        controller.velocity *= 0.9; // Velocity damping for smooth movement
    }
}

// System to detect planet hovering for information display
// System to display premium info cards using EGUI (only for selected bodies)
pub fn display_hover_info(mut contexts: EguiContexts, selected_planet: Res<SelectedPlanet>) {
    if let Some(name) = &selected_planet.name {
        let ctx = contexts.ctx_mut();

        // Create a reasonable-sized floating information card like modern UI cards
        egui::Window::new("")
            .title_bar(false) // Remove title bar for clean design
            .resizable(false)
            .default_pos([50.0, 50.0])
            .default_size([420.0, 640.0]) // Increased dimensions for better content accommodation
            .frame(egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(15, 23, 42, 250), // Dark blue-gray background
                stroke: egui::Stroke::new(1.5, egui::Color32::from_rgb(59, 130, 246)), // Subtle blue border
                rounding: egui::Rounding::same(12.0), // Card-like rounded corners
                inner_margin: egui::Margin::symmetric(16.0, 12.0), // Card-appropriate padding
                ..Default::default()
            })
            .show(ctx, |ui| {
                // Header with celestial body name and icon - perfectly centered
                ui.vertical_centered(|ui| {
                    let (icon, header_color) = get_celestial_icon_and_color(name);

                    ui.add_space(6.0); // Compact top spacing
                    ui.label(
                        egui::RichText::new(icon)
                            .size(36.0) // Appropriate size for card format
                            .color(header_color),
                    );
                    ui.add_space(6.0); // Compact spacing between icon and title
                    ui.label(
                        egui::RichText::new(name)
                            .size(24.0) // Card-appropriate title size
                            .color(egui::Color32::WHITE)
                            .strong(),
                    );
                    ui.add_space(12.0); // Reasonable spacing before content
                });

                egui::ScrollArea::vertical()
                    .max_height(540.0) // Increased max height for taller card dimensions
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Main information sections with consistent spacing
                        display_celestial_info(ui, name);

                        ui.add_space(16.0); // Card-appropriate section spacing

                        // Fun facts section with card-appropriate styling
                        ui.group(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("✨ Interesting Facts")
                                        .size(14.0)
                                        .color(egui::Color32::from_rgb(251, 191, 36)) // Amber
                                        .strong(),
                                );
                                ui.add_space(8.0);
                            });

                            display_fun_facts(ui, name);

                            ui.add_space(6.0);
                        });

                        ui.add_space(12.0);

                        // Exploration status with card-appropriate styling
                        ui.group(|ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new("🚀 Exploration Status")
                                        .size(14.0)
                                        .color(egui::Color32::from_rgb(34, 197, 94)) // Green
                                        .strong(),
                                );
                                ui.add_space(8.0);
                            });

                            display_exploration_status(ui, name);

                            ui.add_space(6.0);
                        });

                        ui.add_space(8.0);
                    });

                // Footer with consistent spacing and better visibility
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Click to select • Scroll to zoom • WASD to move")
                            .size(11.0)
                            .color(egui::Color32::from_rgba_premultiplied(148, 163, 184, 200)),
                    ); // Better contrast
                });
            });
    }
}

// Helper function to get celestial body icon and color with universally supported Unicode
fn get_celestial_icon_and_color(name: &str) -> (&'static str, egui::Color32) {
    match name {
        "Sun" => ("☀", egui::Color32::from_rgb(251, 191, 36)), // Bright yellow sun
        "Mercury" => ("☿", egui::Color32::from_rgb(156, 163, 175)), // Mercury symbol
        "Venus" => ("♀", egui::Color32::from_rgb(251, 146, 60)), // Venus symbol
        "Earth" => ("🌍", egui::Color32::from_rgb(59, 130, 246)), // Earth globe
        "Mars" => ("♂", egui::Color32::from_rgb(239, 68, 68)), // Mars symbol
        "Jupiter" => ("♃", egui::Color32::from_rgb(245, 158, 11)), // Jupiter symbol
        "Saturn" => ("♄", egui::Color32::from_rgb(251, 191, 36)), // Saturn symbol
        "Uranus" => ("⛢", egui::Color32::from_rgb(34, 197, 94)), // Uranus symbol
        "Neptune" => ("♆", egui::Color32::from_rgb(59, 130, 246)), // Neptune symbol
        _ => ("🌙", egui::Color32::from_rgb(147, 51, 234)),    // Moon symbol for moons
    }
}

// Display comprehensive celestial information
fn display_celestial_info(ui: &mut egui::Ui, name: &str) {
    match name {
        "Sun" => {
            display_info_section(
                ui,
                "Stellar Classification",
                "G-type main-sequence star (G2V)",
            );
            display_info_section(ui, "Mass", "1.989 × 10³⁰ kg (333,000 Earth masses)");
            display_info_section(ui, "Radius", "696,342 km (109 Earth radii)");
            display_info_section(ui, "Surface Temperature", "5,778 K (5,505°C)");
            display_info_section(ui, "Composition", "About 74% hydrogen, 24% helium (by mass)");
            display_info_section(ui, "Luminosity", "3.83 × 10²⁶ W");
            display_info_section(ui, "Age", "4.6 billion years");
            display_info_section(ui, "Distance from Earth", "149.6 million km (1 AU)");
        }
        "Mercury" => {
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "3.301 × 10²³ kg (0.055 Earth masses)");
            display_info_section(ui, "Radius", "2,439.7 km (0.383 Earth radii)");
            display_info_section(ui, "Distance from Sun", "0.387 AU (57.9 million km)");
            display_info_section(ui, "Orbital Period", "88 Earth days");
            display_info_section(ui, "Day Length", "176 Earth days");
            display_info_section(ui, "Surface Temperature", "-173°C to 427°C");
            display_info_section(ui, "Atmosphere", "Extremely thin exosphere (sodium, oxygen)");
            display_info_section(ui, "Moons", "0");
        }
        "Venus" => {
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "4.867 × 10²⁴ kg (0.815 Earth masses)");
            display_info_section(ui, "Radius", "6,051.8 km (0.949 Earth radii)");
            display_info_section(ui, "Distance from Sun", "0.723 AU (108.2 million km)");
            display_info_section(ui, "Orbital Period", "225 Earth days");
            display_info_section(ui, "Day Length", "243 Earth days (retrograde)");
            display_info_section(ui, "Surface Temperature", "462°C (hottest planet)");
            display_info_section(ui, "Atmosphere", "96.5% CO2, sulfuric acid clouds");
        }
        "Earth" => {
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "5.972 × 10²⁴ kg");
            display_info_section(ui, "Radius", "6,371 km");
            display_info_section(ui, "Distance from Sun", "1.000 AU (149.6 million km)");
            display_info_section(ui, "Orbital Period", "365.25 days");
            display_info_section(ui, "Day Length", "24 hours");
            display_info_section(ui, "Surface Temperature", "-89°C to 58°C");
            display_info_section(ui, "Moons", "1 (The Moon)");
            display_info_section(ui, "Atmosphere", "78% N2, 21% O2, trace gases");
        }
        "Mars" => {
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "6.417 × 10²³ kg (0.107 Earth masses)");
            display_info_section(ui, "Radius", "3,389.5 km (0.532 Earth radii)");
            display_info_section(ui, "Distance from Sun", "1.524 AU (227.9 million km)");
            display_info_section(ui, "Orbital Period", "687 Earth days");
            display_info_section(ui, "Day Length", "24.6 hours");
            display_info_section(ui, "Surface Temperature", "-87°C to -5°C");
            display_info_section(ui, "Moons", "2 (Phobos, Deimos)");
            display_info_section(ui, "Atmosphere", "95% CO2, thin and dusty");
        }
        "Jupiter" => {
            display_info_section(ui, "Type", "Gas giant");
            display_info_section(ui, "Mass", "1.898 × 10²⁷ kg (317.8 Earth masses)");
            display_info_section(ui, "Radius", "69,911 km (10.97 Earth radii)");
            display_info_section(ui, "Distance from Sun", "5.204 AU (778.5 million km)");
            display_info_section(ui, "Orbital Period", "4,333 Earth days (11.86 years)");
            display_info_section(ui, "Day Length", "9.93 hours");
            display_info_section(
                ui,
                "Moons",
                "95+ (4 Galilean: Io, Europa, Ganymede, Callisto)",
            );
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, ammonia clouds");
        }
        "Saturn" => {
            display_info_section(ui, "Type", "Gas giant");
            display_info_section(ui, "Mass", "5.683 × 10²⁶ kg (95.2 Earth masses)");
            display_info_section(ui, "Radius", "58,232 km (9.14 Earth radii)");
            display_info_section(ui, "Distance from Sun", "9.539 AU (1.43 billion km)");
            display_info_section(ui, "Orbital Period", "10,759 Earth days (29.46 years)");
            display_info_section(ui, "Day Length", "10.7 hours");
            display_info_section(ui, "Moons", "146+ (major: Titan, Enceladus, Mimas)");
            display_info_section(ui, "Rings", "Complex ring system of ice and rock");
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, trace methane");
        }
        "Uranus" => {
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "8.681 × 10²⁵ kg (14.5 Earth masses)");
            display_info_section(ui, "Radius", "25,362 km (4.01 Earth radii)");
            display_info_section(ui, "Distance from Sun", "19.191 AU (2.87 billion km)");
            display_info_section(ui, "Orbital Period", "30,687 Earth days (84.01 years)");
            display_info_section(ui, "Day Length", "17.2 hours");
            display_info_section(ui, "Axial Tilt", "98° (rotates on its side)");
            display_info_section(
                ui,
                "Moons",
                "28 (major: Titania, Oberon, Umbriel, Ariel, Miranda)",
            );
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, methane (blue color)");
        }
        "Neptune" => {
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "1.024 × 10²⁶ kg (17.1 Earth masses)");
            display_info_section(ui, "Radius", "24,622 km (3.88 Earth radii)");
            display_info_section(ui, "Distance from Sun", "30.061 AU (4.5 billion km)");
            display_info_section(ui, "Orbital Period", "60,190 Earth days (164.8 years)");
            display_info_section(ui, "Day Length", "16.1 hours");
            display_info_section(
                ui,
                "Wind Speed",
                "Up to 2,100 km/h (fastest in solar system)",
            );
            display_info_section(ui, "Moons", "16 (major: Triton, Proteus, Nereid)");
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, methane");
        }
        "Moon" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Mass", "7.342 × 10²² kg (0.0123 Earth masses)");
            display_info_section(ui, "Radius", "1,737.4 km (0.273 Earth radii)");
            display_info_section(ui, "Distance from Earth", "384,400 km (0.00257 AU)");
            display_info_section(ui, "Orbital Period", "27.3 Earth days");
            display_info_section(ui, "Day Length", "27.3 Earth days (tidal locking)");
            display_info_section(ui, "Surface Gravity", "1.62 m/s² (16.6% of Earth)");
            display_info_section(ui, "Surface", "Regolith with basaltic maria");
        }
        "Phobos" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Mars");
            display_info_section(ui, "Discovery", "1877 (Asaph Hall)");
            display_info_section(ui, "Notable", "Fastest-orbiting moon in the solar system");
        }
        "Deimos" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Mars");
            display_info_section(ui, "Discovery", "1877 (Asaph Hall)");
            display_info_section(ui, "Notable", "Likely captured asteroid");
        }
        "Io" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Most volcanically active body");
        }
        "Europa" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Global subsurface ocean likely");
        }
        "Ganymede" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Largest moon; has magnetic field");
        }
        "Callisto" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Heavily cratered ancient surface");
        }
        "Mimas" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1789 (William Herschel)");
            display_info_section(ui, "Notable", "Herschel crater dominates surface");
        }
        "Enceladus" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1789 (William Herschel)");
            display_info_section(ui, "Notable", "Active geysers, subsurface ocean");
        }
        "Tethys" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1684 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Ithaca Chasma canyon system");
        }
        "Dione" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1684 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Wispy icy cliffs");
        }
        "Rhea" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1672 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Second-largest moon of Saturn");
        }
        "Titan" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1655 (Christiaan Huygens)");
            display_info_section(ui, "Notable", "Thick atmosphere and methane lakes");
        }
        "Hyperion" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1848 (Bond and Lassell)");
            display_info_section(ui, "Notable", "Chaotic rotation and porous interior");
        }
        "Iapetus" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1671 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Two-tone surface and equatorial ridge");
        }
        "Miranda" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1948 (Gerard Kuiper)");
            display_info_section(ui, "Notable", "Patchwork terrain with giant cliffs");
        }
        "Ariel" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1851 (William Lassell)");
            display_info_section(ui, "Notable", "Faulted surface and possible cryovolcanism");
        }
        "Umbriel" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1851 (William Lassell)");
            display_info_section(ui, "Notable", "Darkest of Uranus's major moons");
        }
        "Titania" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1787 (William Herschel)");
            display_info_section(ui, "Notable", "Largest moon of Uranus");
        }
        "Oberon" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1787 (William Herschel)");
            display_info_section(ui, "Notable", "Heavily cratered icy surface");
        }
        "Triton" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1846 (William Lassell)");
            display_info_section(ui, "Notable", "Retrograde orbit and geysers");
        }
        "Proteus" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1989 (Voyager 2)");
            display_info_section(ui, "Notable", "Largest regular moon of Neptune");
        }
        "Nereid" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1949 (Gerard Kuiper)");
            display_info_section(ui, "Notable", "Highly eccentric orbit");
        }
        "Larissa" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1989 (Voyager 2)");
            display_info_section(ui, "Notable", "Irregular moon inside ring system");
        }
        _ => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", get_parent_body(name));
            display_info_section(ui, "Discovery", get_discovery_info(name));
        }
    }
}

// Helper function to display information sections with consistent spacing
fn display_info_section(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        // Label with consistent styling and spacing
        ui.label(
            egui::RichText::new(format!("{}:", label))
                .size(13.0)
                .color(egui::Color32::from_rgb(148, 163, 184)),
        ); // Light gray

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(8.0); // Consistent spacing between label and value
            ui.label(
                egui::RichText::new(value)
                    .size(13.0)
                    .color(egui::Color32::from_rgb(226, 232, 240)),
            ); // Light blue-gray
        });
    });
    ui.add_space(6.0); // Consistent spacing between sections
}

// Display interesting facts with consistent spacing and formatting
fn display_fun_facts(ui: &mut egui::Ui, name: &str) {
    let facts: Vec<String> = match name {
        "Sun" => vec![
            "Contains 99.86% of the solar system's mass".to_string(),
            "Light takes 8 minutes to reach Earth".to_string(),
            "Core temperature reaches about 15 million C".to_string(),
            "Fuses about 600 million tons of hydrogen per second".to_string(),
        ],
        "Mercury" => vec![
            "A year on Mercury is only 88 Earth days".to_string(),
            "Rotates in a 3:2 spin-orbit resonance".to_string(),
            "Has a large iron core for its size".to_string(),
            "No true atmosphere, just a thin exosphere".to_string(),
        ],
        "Venus" => vec![
            "Rotates backward compared to most planets".to_string(),
            "Surface pressure is about 92 times Earth's".to_string(),
            "Runaway greenhouse makes it hotter than Mercury".to_string(),
            "Day is longer than its year".to_string(),
        ],
        "Earth" => vec![
            "Only known planet with life".to_string(),
            "71% of surface covered by water".to_string(),
            "Magnetic field protects from solar radiation".to_string(),
            "Only planet with active plate tectonics".to_string(),
        ],
        "Mars" => vec![
            "Largest volcano: Olympus Mons (3x taller than Everest)".to_string(),
            "Day length: 24 hours and 37 minutes".to_string(),
            "Seasons last twice as long as Earth's".to_string(),
            "Atmosphere is 95% carbon dioxide".to_string(),
        ],
        "Jupiter" => vec![
            "Great Red Spot: storm larger than Earth".to_string(),
            "Faint ring system discovered in 1979".to_string(),
            "Emits more heat than it receives from the Sun".to_string(),
            "Has the strongest planetary magnetic field".to_string(),
        ],
        "Saturn" => vec![
            "Rings made of ice chunks and dust particles".to_string(),
            "Less dense than water - would float in a bathtub!".to_string(),
            "Hexagonal storm at north pole".to_string(),
            "Moon Titan has thicker atmosphere than Earth".to_string(),
        ],
        "Uranus" => vec![
            "Axial tilt is about 98 degrees".to_string(),
            "Coldest average temperatures of any planet".to_string(),
            "Faint rings discovered in 1977".to_string(),
            "Seasons last about 21 Earth years".to_string(),
        ],
        "Neptune" => vec![
            "Fastest winds in the solar system".to_string(),
            "Discovered by mathematical prediction".to_string(),
            "Great Dark Spot observed by Voyager 2".to_string(),
            "Triton orbits backward around Neptune".to_string(),
        ],
        "Moon" => vec![
            "Moving away from Earth at 3.8 cm/year".to_string(),
            "Far side photographed by Luna 3 in 1959".to_string(),
            "Moonquakes are real and can last minutes".to_string(),
            "Apollo footprints will last millions of years".to_string(),
        ],
        "Phobos" => vec![
            "Orbit is slowly decaying toward Mars".to_string(),
            "May break up and form a ring in the future".to_string(),
            "Grooved surface hints at past stress fractures".to_string(),
        ],
        "Deimos" => vec![
            "Likely a captured asteroid".to_string(),
            "Very dark, carbon-rich surface".to_string(),
            "Orbits Mars farther out than Phobos".to_string(),
        ],
        "Io" => vec![
            "Most volcanically active body in the solar system".to_string(),
            "Sulfur plumes can rise hundreds of kilometers".to_string(),
            "Tidal heating from Jupiter powers its volcanism".to_string(),
        ],
        "Europa" => vec![
            "Likely has a global ocean beneath its ice".to_string(),
            "Surface is one of the smoothest in the solar system".to_string(),
            "Possible water vapor plumes observed".to_string(),
        ],
        "Ganymede" => vec![
            "Largest moon in the solar system".to_string(),
            "Only moon known to have its own magnetic field".to_string(),
            "May have a deep subsurface ocean".to_string(),
        ],
        "Callisto" => vec![
            "One of the most heavily cratered moons".to_string(),
            "Surface has changed little for billions of years".to_string(),
            "Likely has a subsurface ocean".to_string(),
        ],
        "Mimas" => vec![
            "Herschel crater makes it look like a Death Star".to_string(),
            "Mostly water ice by composition".to_string(),
            "Smallest of Saturn's major moons".to_string(),
        ],
        "Enceladus" => vec![
            "Geysers vent water vapor from the south pole".to_string(),
            "Evidence for a global subsurface ocean".to_string(),
            "Supplies material to Saturn's E ring".to_string(),
        ],
        "Tethys" => vec![
            "Ithaca Chasma canyon spans much of the moon".to_string(),
            "Very low density suggests mostly ice".to_string(),
            "Odysseus crater is nearly half its diameter".to_string(),
        ],
        "Dione" => vec![
            "Bright wispy terrain are icy cliffs".to_string(),
            "Has a tenuous oxygen exosphere".to_string(),
            "Tidal heating may occur in its interior".to_string(),
        ],
        "Rhea" => vec![
            "Second-largest moon of Saturn".to_string(),
            "Heavily cratered icy surface".to_string(),
            "Has a thin oxygen and CO2 exosphere".to_string(),
        ],
        "Titan" => vec![
            "Thick nitrogen atmosphere with methane clouds".to_string(),
            "Liquid methane and ethane lakes on the surface".to_string(),
            "Methane cycle resembles Earth's water cycle".to_string(),
        ],
        "Hyperion" => vec![
            "Irregular shape with chaotic rotation".to_string(),
            "Very porous, sponge-like interior".to_string(),
            "Surface is extremely dark and cratered".to_string(),
        ],
        "Iapetus" => vec![
            "Two-tone surface: bright and dark hemispheres".to_string(),
            "Has a massive equatorial ridge".to_string(),
            "Dark material may be from outer moons".to_string(),
        ],
        "Miranda" => vec![
            "Patchwork surface from past tectonic activity".to_string(),
            "Giant cliffs are among the tallest in the solar system".to_string(),
            "Distinct coronae regions reshape the surface".to_string(),
        ],
        "Ariel" => vec![
            "Icy moon with long fault valleys".to_string(),
            "Surface shows evidence of resurfacing".to_string(),
            "Possible past cryovolcanism".to_string(),
        ],
        "Umbriel" => vec![
            "Darkest of Uranus's major moons".to_string(),
            "Wunda crater has a bright ring".to_string(),
            "Heavily cratered, ancient surface".to_string(),
        ],
        "Titania" => vec![
            "Largest moon of Uranus".to_string(),
            "Canyons suggest crustal expansion".to_string(),
            "May host a subsurface ocean".to_string(),
        ],
        "Oberon" => vec![
            "Second-largest moon of Uranus".to_string(),
            "Heavily cratered with some icy deposits".to_string(),
            "Surface shows signs of past geology".to_string(),
        ],
        "Triton" => vec![
            "Orbits Neptune in retrograde direction".to_string(),
            "Nitrogen geysers observed by Voyager 2".to_string(),
            "Likely a captured Kuiper Belt object".to_string(),
        ],
        "Proteus" => vec![
            "Largest regular moon of Neptune".to_string(),
            "Irregular shape with dark surface".to_string(),
            "Discovered by Voyager 2 in 1989".to_string(),
        ],
        "Nereid" => vec![
            "Has a highly eccentric, elongated orbit".to_string(),
            "Likely a captured object".to_string(),
            "One of Neptune's most distant moons".to_string(),
        ],
        "Larissa" => vec![
            "Small, irregular moon close to Neptune".to_string(),
            "Discovered by Voyager 2 in 1989".to_string(),
            "Orbits inside Neptune's faint ring system".to_string(),
        ],
        _ => vec![format!(
            "{} has unique and fascinating characteristics",
            name
        )],
    };

    for fact in facts {
        ui.add_space(4.0); // Consistent spacing between facts
        ui.label(
            egui::RichText::new(format!("• {}", fact))
                .size(12.0)
                .color(egui::Color32::from_rgb(226, 232, 240)),
        );
    }
}

// Display exploration status with consistent spacing
fn display_exploration_status(ui: &mut egui::Ui, name: &str) {
    let status = match name {
        "Sun" => "🛰 Studied by SOHO, SDO, and Parker Solar Probe",
        "Mercury" => "🛰 Mariner 10 (1974-1975), MESSENGER (2011-2015), BepiColombo (en route)",
        "Venus" => "🛰 Venera program (1960s-1980s), Magellan (1990-1994), Akatsuki (2015-present)",
        "Earth" => "🏠 Humanity's home - extensively explored and mapped",
        "Mars" => "🤖 Perseverance, Curiosity, Insight (active), Mars Sample Return (planned)",
        "Jupiter" => {
            "🛰 Pioneer 10 (1973), Voyager (1979), Galileo (1995-2003), Juno (2016-present)"
        }
        "Saturn" => "🛰 Pioneer 11 (1979), Voyager (1980-1981), Cassini-Huygens (2004-2017)",
        "Uranus" => "🛰 Voyager 2 flyby (1986) - only spacecraft to visit",
        "Neptune" => "🛰 Voyager 2 flyby (1989) - only spacecraft to visit",
        "Moon" => {
            "🚀 Apollo missions (1969-1972), Luna program, Chang'e missions, Artemis (planned)"
        }
        _ => "🔭 Observed remotely by telescopes and space probes",
    };

    ui.add_space(4.0); // Consistent top spacing
    ui.label(
        egui::RichText::new(status)
            .size(12.0)
            .color(egui::Color32::from_rgb(226, 232, 240)),
    );
}

// Helper functions for moon information
fn get_parent_body(name: &str) -> &'static str {
    match name {
        "Phobos" | "Deimos" => "Mars",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "Jupiter",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Hyperion" | "Iapetus" => {
            "Saturn"
        }
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "Uranus",
        "Triton" | "Proteus" | "Nereid" | "Larissa" => "Neptune",
        "Moon" => "Earth",
        _ => "Unknown",
    }
}

fn get_discovery_info(name: &str) -> &'static str {
    match name {
        "Moon" => "Ancient times",
        "Phobos" | "Deimos" => "1877 (Asaph Hall)",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "1610 (Galileo)",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Iapetus" => {
            "Various 17th-19th century"
        }
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "1787-1851 (William Herschel)",
        "Triton" => "1846 (William Lassell)",
        _ => "Various space missions",
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

// System to auto-inspect a selected planet by smoothly framing it at a readable distance
pub fn auto_inspect_selected_planet(
    time: Res<Time>,
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    mut camera_query: Query<(&CameraController, &mut Transform)>,
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

    let (controller, mut camera_transform) = match camera_query.get_single_mut() {
        Ok(data) => data,
        Err(_) => return,
    };

    if controller.mode != CameraMode::FreeFlight {
        return;
    }

    let planet_radius = if planet_comp.domain_planet.name == "Sun" {
        physics::calculate_sun_visual_radius(&solar_params)
    } else {
        physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
    };

    let min_distance = (planet_radius * 3.0).max(2000.0);
    let max_distance = (planet_radius * 8.0).max(min_distance + 500.0);
    let target_distance = planet_radius * 5.0;

    let planet_pos = planet_transform.translation();
    let camera_pos = camera_transform.translation;
    let current_offset = camera_pos - planet_pos;
    let current_distance = current_offset.length();

    if state.selected != Some(selected_entity) || state.offset == Vec3::ZERO {
        state.selected = Some(selected_entity);
        state.offset = if current_distance > 0.0 {
            current_offset
        } else {
            *camera_transform.forward() * target_distance
        };
    }

    let offset_distance = state.offset.length();
    if offset_distance < min_distance || offset_distance > max_distance {
        let dir = normalized_or_zero(state.offset);
        state.offset = dir * target_distance;
    }

    let target_pos = planet_pos + state.offset;
    let lerp_factor = 1.0 - (-3.5 * time.delta_seconds()).exp();
    camera_transform.translation = camera_transform.translation.lerp(target_pos, lerp_factor);
    camera_transform.look_at(planet_pos, Vec3::Y);
}

#[derive(Default)]
pub struct AutoInspectState {
    selected: Option<Entity>,
    offset: Vec3,
}
