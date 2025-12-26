use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy_egui::{egui, EguiContexts};
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use super::components::{*, HoveredPlanet};

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
        parent_positions.insert(planet_comp.domain_planet.name.clone(), transform.translation);
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

// System to update visual feedback for selected planets (optimized)
pub fn update_planet_selection_visuals(
    time: Res<Time>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(&Selectable, &mut Transform, &GlobalTransform)>,
) {
    let pulse = (time.elapsed_seconds() * 3.0).sin() * 0.1 + 1.0; // Gentle pulsing effect
    let camera_pos = camera_query.single().translation();

    for (selectable, mut transform, global_transform) in query.iter_mut() {
        // Distance culling for visual updates
        let distance_to_camera = (global_transform.translation() - camera_pos).length();
        let max_visual_distance = 30000.0; // Only update visuals for reasonably close objects

        if distance_to_camera > max_visual_distance {
            // Reset scale for distant unselected objects
            if !selectable.selected {
                transform.scale = Vec3::ONE;
            }
            continue;
        }

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

        // Handle mouse look for rotation (only when left mouse button is held)
        let mut mouse_delta = Vec2::ZERO;
        for motion in mouse_motion.read() {
            mouse_delta += motion.delta;
        }

        // Apply mouse sensitivity and update rotation only when left mouse button is held
        // This provides precise control for 3D navigation
        if mouse_delta != Vec2::ZERO && mouse_buttons.pressed(MouseButton::Left) {
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
            transform.rotate_axis(bevy::math::Dir3::new(right).unwrap_or(bevy::math::Dir3::X), pitch);

            // Prevent camera from flipping upside down
            let euler = transform.rotation.to_euler(EulerRot::YXZ);
            let clamped_pitch = euler.1.clamp(-std::f32::consts::PI / 2.1, std::f32::consts::PI / 2.1);
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
        if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight) || keyboard.pressed(KeyCode::KeyE) {
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
        let zoom_speed = controller.speed * 75.0; // Maximum fast zoom with keyboard for astronomical navigation
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

        // Handle mouse wheel for zooming (direct position change, not velocity-based) - MAXIMUM SENSITIVE
        for wheel_event in mouse_wheel.read() {
            let zoom_distance = wheel_event.y * controller.speed * 500.0; // Maximum sensitive zoom for lightning-fast navigation across astronomical distances
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
pub fn detect_planet_hover(
    camera_query: Query<(&bevy::prelude::Camera, &GlobalTransform)>,
    planet_query: Query<(&GlobalTransform, &Selectable)>,
    windows: Query<&Window>,
    mut hovered_planet: ResMut<HoveredPlanet>,
) {
    // Reset hover state
    hovered_planet.name = None;
    hovered_planet.info = None;

    // Get camera and cursor position
    if let Ok((_camera, camera_transform)) = camera_query.get_single() {
        // Use a simpler distance-based approach for hover detection
        // Check if any planet is close to the camera's forward direction

        let camera_pos = camera_transform.translation();
        let camera_forward = camera_transform.forward();

        let mut closest_planet: Option<(String, f32)> = None;

        for (transform, selectable) in planet_query.iter() {
            let planet_pos = transform.translation();
            let distance_to_camera = (planet_pos - camera_pos).length();

            // Check if planet is in front of camera (dot product > 0)
            let to_planet = (planet_pos - camera_pos).normalize();
            let dot_product = camera_forward.dot(to_planet);

            // Only consider planets in front of camera and within reasonable distance
            if dot_product > 0.8 && distance_to_camera < 5000.0 { // Wide viewing angle, reasonable distance
                if let Some((_, current_dist)) = closest_planet {
                    if distance_to_camera < current_dist {
                        closest_planet = Some((selectable.name.clone(), distance_to_camera));
                    }
                } else {
                    closest_planet = Some((selectable.name.clone(), distance_to_camera));
                }
            }
        }

        // Set hover information for the closest planet
        if let Some((planet_name, _)) = closest_planet {
            hovered_planet.name = Some(planet_name.clone());
            hovered_planet.info = Some(get_planet_info(&planet_name));
        }
    }
}

// System to display premium hover information cards using EGUI
pub fn display_hover_info(
    mut contexts: EguiContexts,
    hovered_planet: Res<HoveredPlanet>,
) {
    if let (Some(name), Some(_info)) = (&hovered_planet.name, &hovered_planet.info) {
        let ctx = contexts.ctx_mut();

        // Create a beautiful floating information card
        egui::Window::new("")
            .title_bar(false) // Remove title bar for clean design
            .resizable(false)
            .default_pos([50.0, 50.0])
            .default_size([450.0, 600.0])
            .frame(egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(15, 23, 42, 240), // Dark blue-gray background
                stroke: egui::Stroke::new(2.0, egui::Color32::from_rgb(59, 130, 246)), // Blue border
                rounding: egui::Rounding::same(12.0),
                ..Default::default()
            })
            .show(ctx, |ui| {
                // Header with celestial body name and icon
                ui.vertical_centered(|ui| {
                    let (icon, header_color) = get_celestial_icon_and_color(name);

                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(icon)
                        .size(48.0)
                        .color(header_color));
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new(name)
                        .size(28.0)
                        .color(egui::Color32::WHITE)
                        .strong());
                    ui.add_space(15.0);
                });

                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(10.0);

                    // Main information sections
                    display_celestial_info(ui, name);

                    ui.add_space(20.0);

                    // Fun facts section
                    ui.group(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("✨ Interesting Facts")
                                .size(18.0)
                                .color(egui::Color32::from_rgb(251, 191, 36)) // Amber
                                .strong());
                        });
                        ui.add_space(8.0);
                        display_fun_facts(ui, name);
                    });

                    ui.add_space(15.0);

                    // Exploration status
                    ui.group(|ui| {
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("🚀 Exploration Status")
                                .size(16.0)
                                .color(egui::Color32::from_rgb(34, 197, 94)) // Green
                                .strong());
                        });
                        ui.add_space(5.0);
                        display_exploration_status(ui, name);
                    });
                });

                // Footer hint
                ui.add_space(10.0);
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("Click to select • Scroll to zoom • WASD to move")
                        .size(10.0)
                        .color(egui::Color32::from_rgba_premultiplied(148, 163, 184, 180))); // Muted gray
                });
            });
    }
}

// Helper function to get celestial body icon and color
fn get_celestial_icon_and_color(name: &str) -> (&'static str, egui::Color32) {
    match name {
        "Sun" => ("☀️", egui::Color32::from_rgb(251, 191, 36)), // Bright yellow
        "Mercury" => ("☿", egui::Color32::from_rgb(156, 163, 175)), // Gray
        "Venus" => ("♀", egui::Color32::from_rgb(251, 146, 60)), // Orange
        "Earth" => ("🌍", egui::Color32::from_rgb(59, 130, 246)), // Blue
        "Mars" => ("♂", egui::Color32::from_rgb(239, 68, 68)), // Red
        "Jupiter" => ("♃", egui::Color32::from_rgb(245, 158, 11)), // Amber
        "Saturn" => ("♄", egui::Color32::from_rgb(251, 191, 36)), // Gold
        "Uranus" => ("⛢", egui::Color32::from_rgb(34, 197, 94)), // Green
        "Neptune" => ("♆", egui::Color32::from_rgb(59, 130, 246)), // Blue
        _ => ("🌌", egui::Color32::from_rgb(147, 51, 234)), // Purple for moons
    }
}

// Display comprehensive celestial information
fn display_celestial_info(ui: &mut egui::Ui, name: &str) {
    match name {
        "Sun" => {
            display_info_section(ui, "Stellar Classification", "G-type main-sequence star (G2V)");
            display_info_section(ui, "Mass", "1.989 × 10³⁰ kg (333,000 Earth masses)");
            display_info_section(ui, "Radius", "696,342 km (109 Earth radii)");
            display_info_section(ui, "Surface Temperature", "5,778 K (5,505°C)");
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
        }
        "Venus" => {
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "4.867 × 10²⁴ kg (0.815 Earth masses)");
            display_info_section(ui, "Radius", "6,051.8 km (0.949 Earth radii)");
            display_info_section(ui, "Distance from Sun", "0.723 AU (108.2 million km)");
            display_info_section(ui, "Orbital Period", "225 Earth days");
            display_info_section(ui, "Day Length", "243 Earth days (retrograde)");
            display_info_section(ui, "Surface Temperature", "462°C (hottest planet)");
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
        }
        "Jupiter" => {
            display_info_section(ui, "Type", "Gas giant");
            display_info_section(ui, "Mass", "1.898 × 10²⁷ kg (317.8 Earth masses)");
            display_info_section(ui, "Radius", "69,911 km (10.97 Earth radii)");
            display_info_section(ui, "Distance from Sun", "5.204 AU (778.5 million km)");
            display_info_section(ui, "Orbital Period", "4,333 Earth days (11.86 years)");
            display_info_section(ui, "Day Length", "9.93 hours");
            display_info_section(ui, "Moons", "95+ (4 Galilean: Io, Europa, Ganymede, Callisto)");
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
        }
        "Uranus" => {
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "8.681 × 10²⁵ kg (14.5 Earth masses)");
            display_info_section(ui, "Radius", "25,362 km (4.01 Earth radii)");
            display_info_section(ui, "Distance from Sun", "19.191 AU (2.87 billion km)");
            display_info_section(ui, "Orbital Period", "30,687 Earth days (84.01 years)");
            display_info_section(ui, "Day Length", "17.2 hours");
            display_info_section(ui, "Axial Tilt", "98° (rotates on its side)");
            display_info_section(ui, "Moons", "28 (major: Titania, Oberon, Umbriel, Ariel, Miranda)");
        }
        "Neptune" => {
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "1.024 × 10²⁶ kg (17.1 Earth masses)");
            display_info_section(ui, "Radius", "24,622 km (3.88 Earth radii)");
            display_info_section(ui, "Distance from Sun", "30.061 AU (4.5 billion km)");
            display_info_section(ui, "Orbital Period", "60,190 Earth days (164.8 years)");
            display_info_section(ui, "Day Length", "16.1 hours");
            display_info_section(ui, "Wind Speed", "Up to 2,100 km/h (fastest in solar system)");
            display_info_section(ui, "Moons", "16 (major: Triton, Proteus, Nereid)");
        }
        "Moon" => {
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Mass", "7.342 × 10²² kg (0.0123 Earth masses)");
            display_info_section(ui, "Radius", "1,737.4 km (0.273 Earth radii)");
            display_info_section(ui, "Distance from Earth", "384,400 km (0.00257 AU)");
            display_info_section(ui, "Orbital Period", "27.3 Earth days");
            display_info_section(ui, "Day Length", "27.3 Earth days (tidal locking)");
            display_info_section(ui, "Surface Gravity", "1.62 m/s² (16.6% of Earth)");
        }
        _ => {
            // Generic information for other moons
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", get_parent_body(name));
            display_info_section(ui, "Discovery", get_discovery_info(name));
        }
    }
}

// Helper function to display information sections
fn display_info_section(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{}:", label))
            .size(12.0)
            .color(egui::Color32::from_rgb(148, 163, 184))); // Light gray
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(value)
                .size(12.0)
                .color(egui::Color32::from_rgb(226, 232, 240))); // Light blue-gray
        });
    });
    ui.add_space(2.0);
}

// Display interesting facts
fn display_fun_facts(ui: &mut egui::Ui, name: &str) {
    let facts: Vec<String> = match name {
        "Sun" => vec![
            "The Sun contains 99.86% of the solar system's mass".to_string(),
            "Light takes 8 minutes to reach Earth from the Sun".to_string(),
            "The Sun's core temperature reaches 15 million °C".to_string(),
            "The Sun loses 4 million tons of mass per second through fusion".to_string()
        ],
        "Earth" => vec![
            "Earth is the only known planet with life".to_string(),
            "71% of Earth's surface is covered by water".to_string(),
            "Earth's magnetic field protects us from solar radiation".to_string(),
            "Earth has been habitable for about 3.5 billion years".to_string()
        ],
        "Mars" => vec![
            "Mars has the largest volcano in the solar system (Olympus Mons)".to_string(),
            "A day on Mars is 24 hours and 37 minutes".to_string(),
            "Mars has seasons like Earth but each lasts twice as long".to_string(),
            "Mars' atmosphere is 95% carbon dioxide".to_string()
        ],
        "Jupiter" => vec![
            "Jupiter's Great Red Spot is a storm larger than Earth".to_string(),
            "Jupiter has a faint ring system discovered in 1979".to_string(),
            "Jupiter acts as a cosmic vacuum cleaner, protecting inner planets".to_string(),
            "Jupiter's magnetic field is 20,000 times stronger than Earth's".to_string()
        ],
        "Saturn" => vec![
            "Saturn's rings are made mostly of ice chunks and dust".to_string(),
            "Saturn is less dense than water - it would float!".to_string(),
            "Saturn has a hexagonal storm at its north pole".to_string(),
            "Saturn's moon Titan has a thicker atmosphere than Earth".to_string()
        ],
        "Moon" => vec![
            "The Moon is slowly moving away from Earth (3.8 cm/year)".to_string(),
            "The Moon's far side was first photographed by Luna 3 in 1959".to_string(),
            "The Moon has quakes caused by Earth's gravitational pull".to_string(),
            "Apollo astronauts' footprints will last millions of years".to_string()
        ],
        _ => vec![format!("{} is a fascinating celestial body with unique characteristics", name)]
    };

    for fact in facts {
        ui.label(egui::RichText::new(format!("• {}", fact))
            .size(11.0)
            .color(egui::Color32::from_rgb(226, 232, 240)));
    }
}

// Display exploration status
fn display_exploration_status(ui: &mut egui::Ui, name: &str) {
    let status = match name {
        "Sun" => "🛰️ Studied remotely by SOHO, SDO, and Parker Solar Probe",
        "Mercury" => "🛰️ Mariner 10 (1974-1975), MESSENGER (2011-2015), BepiColombo (en route)",
        "Venus" => "🛰️ Venera program (1960s-1980s), Magellan (1990-1994), Akatsuki (2015-present)",
        "Earth" => "🏠 Humanity's home - extensively explored and mapped",
        "Mars" => "🤖 Perseverance, Curiosity, Insight (active), Mars Sample Return (planned)",
        "Jupiter" => "🛰️ Pioneer 10 (1973), Voyager (1979), Galileo (1995-2003), Juno (2016-present)",
        "Saturn" => "🛰️ Pioneer 11 (1979), Voyager (1980-1981), Cassini-Huygens (2004-2017)",
        "Uranus" => "🛰️ Voyager 2 flyby (1986) - only spacecraft to visit",
        "Neptune" => "🛰️ Voyager 2 flyby (1989) - only spacecraft to visit",
        "Moon" => "🚀 Apollo missions (1969-1972), Luna program, Chang'e missions, Artemis (planned)",
        _ => "🔭 Observed remotely by telescopes and space probes"
    };

    ui.label(egui::RichText::new(status)
        .size(11.0)
        .color(egui::Color32::from_rgb(226, 232, 240)));
}

// Helper functions for moon information
fn get_parent_body(name: &str) -> &'static str {
    match name {
        "Phobos" | "Deimos" => "Mars",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "Jupiter",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Hyperion" | "Iapetus" => "Saturn",
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "Uranus",
        "Triton" | "Proteus" | "Nereid" | "Larissa" => "Neptune",
        "Moon" => "Earth",
        _ => "Unknown"
    }
}

fn get_discovery_info(name: &str) -> &'static str {
    match name {
        "Moon" => "Ancient times",
        "Phobos" | "Deimos" => "1877 (Asaph Hall)",
        "Io" | "Europa" | "Ganymede" | "Callisto" => "1610 (Galileo)",
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Iapetus" => "Various 17th-19th century",
        "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" => "1787-1851 (William Herschel)",
        "Triton" => "1846 (William Lassell)",
        _ => "Various space missions"
    }
}

// Function to get educational information about planets and moons
fn get_planet_info(name: &str) -> String {
    match name {
        "Sun" => "🌞 The Sun: A yellow dwarf star at the center of our solar system. \
                  Mass: 333,000 Earths. Surface temperature: 5,778°C. \
                  Provides light and heat to all planets.".to_string(),

        "Mercury" => "☿ Mercury: The smallest planet and closest to the Sun. \
                     Extreme temperatures: 427°C (day) to -173°C (night). \
                     No atmosphere, heavily cratered surface. \
                     Completes orbit in just 88 Earth days.".to_string(),

        "Venus" => "♀ Venus: Earth's 'sister planet' with similar size but very different conditions. \
                   Hottest planet with surface temperature of 462°C. \
                   Thick toxic atmosphere of CO2. Rotates backwards! \
                   Often called Earth's twin due to size.".to_string(),

        "Earth" => "🌍 Earth: Our home planet, the only known world with life. \
                   71% surface water, protective atmosphere. \
                   Average temperature: 15°C. One large moon. \
                   Supports millions of species including humans.".to_string(),

        "Mars" => "♂ Mars: The Red Planet, named after the Roman god of war. \
                  Cold desert world with polar ice caps. \
                  Two small moons: Phobos and Deimos. \
                  Evidence of ancient water and potential past life.".to_string(),

        "Jupiter" => "♃ Jupiter: Largest planet, a gas giant 2.5x more massive than all other planets combined. \
                     Great Red Spot: A giant storm larger than Earth. \
                     95 moons including the 4 large Galilean moons. \
                     Strong magnetic field, faint rings.".to_string(),

        "Saturn" => "♄ Saturn: Famous for its spectacular ring system made of ice and rock. \
                    A gas giant less dense than water (would float!). \
                    146 moons, including Titan (larger than Mercury). \
                    Hexagonal storm at north pole.".to_string(),

        "Uranus" => "⛢ Uranus: Ice giant that rotates on its side (98° axial tilt). \
                    Coldest planetary atmosphere at -224°C. \
                    27 moons, faint rings. Unique blue-green color from methane. \
                    Discovered in 1781 by William Herschel.".to_string(),

        "Neptune" => "♆ Neptune: Deep blue ice giant with the strongest winds in the solar system (2,100 km/h). \
                     16 moons including Triton (retrograde orbit). \
                     Great Dark Spot: A giant storm similar to Jupiter's. \
                     Discovered in 1846 through mathematical predictions.".to_string(),

        "Moon" => "🌕 Earth's Moon: Our natural satellite, formed 4.5 billion years ago. \
                  Synchronous rotation (same face always visible). \
                  Influences tides and stabilizes Earth's axial tilt. \
                  No atmosphere, heavily cratered surface.".to_string(),

        "Phobos" => "👽 Phobos: Larger of Mars' two moons. Irregular potato-shaped rock. \
                    Orbiting closer to Mars than any other moon to its planet. \
                    Slowly spiraling inward, will eventually crash into Mars.".to_string(),

        "Deimos" => "👽 Deimos: Smaller of Mars' two moons. Very dark, carbon-rich surface. \
                    More distant orbit than Phobos. \
                    Both moons likely captured asteroids.".to_string(),

        "Io" => "🌋 Io: Most volcanically active body in the solar system. \
                Over 400 volcanoes, surface constantly resurfaced. \
                Orbiting inside Jupiter's radiation belts. \
                Discovered by Galileo in 1610.".to_string(),

        "Europa" => "🧊 Europa: Ice-covered moon with subsurface ocean. \
                    Smoothest surface in the solar system. \
                    Possible habitable environment beneath the ice. \
                    Shows evidence of recent geological activity.".to_string(),

        "Ganymede" => "🌕 Ganymede: Largest moon in the solar system, bigger than Mercury. \
                      Only moon with its own magnetic field. \
                      Surface shows light and dark terrains. \
                      Thin oxygen atmosphere.".to_string(),

        "Callisto" => "🧊 Callisto: Heavily cratered ice moon. \
                      Oldest, most cratered surface in the solar system. \
                      Possible subsurface ocean. \
                      Weakest magnetic field influence of the Galilean moons.".to_string(),

        "Titan" => "🌌 Titan: Saturn's largest moon, bigger than Mercury. \
                   Thick atmosphere richer than Earth's. \
                   Lakes and rivers of liquid methane and ethane. \
                   Only other body besides Earth with stable liquids on surface.".to_string(),

        "Enceladus" => "🌊 Enceladus: Brightest object in the solar system due to fresh ice. \
                       Water vapor geysers from south pole. \
                       Possible subsurface ocean. \
                       One of the most reflective bodies known.".to_string(),

        "Mimas" => "👁️ Mimas: Saturn's 'Death Star' moon with giant crater. \
                   Herschel crater is 130km wide (1/3 of moon's diameter). \
                   Low density suggests high water ice content. \
                   Synchronous rotation with Saturn.".to_string(),

        "Tethys" => "🧊 Tethys: Saturn's icy moon with large crater. \
                    Odysseus crater is 400km wide. \
                    Possible evidence of cryovolcanism. \
                    One of the larger mid-sized moons.".to_string(),

        "Dione" => "🧊 Dione: Saturn's moon with bright wispy streaks. \
                   Surface shows tectonic fractures. \
                   Possible thin atmosphere. \
                   Companion to Tethys in orbit.".to_string(),

        "Rhea" => "🧊 Rhea: Saturn's second-largest moon. \
                  Bright, reflective surface. \
                  Possible faint rings or ring system. \
                  Second-brightest moon after Enceladus.".to_string(),

        "Iapetus" => "🏮 Iapetus: Saturn's 'yin-yang' moon with two-tone surface. \
                     Leading hemisphere dark as coal, trailing bright as snow. \
                     Extreme equatorial ridge (20km high). \
                     One of Saturn's outermost major moons.".to_string(),

        "Hyperion" => "🧽 Hyperion: Saturn's spongy, chaotic moon. \
                      Highly porous and irregular shape. \
                      Tumbling rotation, not tidally locked. \
                      One of the largest irregularly shaped moons.".to_string(),

        "Miranda" => "🧊 Miranda: Uranus' strange, varied moon. \
                     Surface shows coronae, faults, and valleys. \
                     Greatest surface variation of any moon. \
                     Smallest of Uranus' round moons.".to_string(),

        "Ariel" => "🧊 Ariel: Uranus' brightest moon. \
                   Young surface with few craters. \
                   Possible past cryovolcanic activity. \
                   One of Uranus' 'classic' moons.".to_string(),

        "Umbriel" => "🧊 Umbriel: Uranus' darkest moon. \
                     Very dark, ancient surface. \
                     Large bright crater. \
                     Orbits between Ariel and Titania.".to_string(),

        "Titania" => "🧊 Titania: Uranus' largest moon. \
                     Heavily cratered surface. \
                     Possible subsurface ocean. \
                     Second-largest Uranian moon.".to_string(),

        "Oberon" => "🧊 Oberon: Uranus' outermost major moon. \
                    Large cratered surface. \
                    Canyon system discovered. \
                    Faintest of Uranus' five major moons.".to_string(),

        "Triton" => "🧊 Triton: Neptune's largest moon, retrograde orbit. \
                    Geysers of nitrogen gas. \
                    Thin nitrogen atmosphere. \
                    Possibly captured Kuiper Belt object.".to_string(),

        "Proteus" => "🧊 Proteus: Neptune's second-largest moon. \
                     Irregular, potato-shaped. \
                     Very dark surface. \
                     Orbits just outside Triton's orbit.".to_string(),

        "Nereid" => "🧊 Nereid: Neptune's third-largest moon. \
                    Highly eccentric orbit. \
                    One of the most eccentric orbits in the solar system. \
                    Possibly captured object.".to_string(),

        "Larissa" => "🧊 Larissa: Neptune's inner moon. \
                     Small, irregular shape. \
                     Orbits within Neptune's rings. \
                     Discovered by Voyager 2.".to_string(),

        _ => format!("🌌 {}: A fascinating celestial body in our solar system.", name),
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