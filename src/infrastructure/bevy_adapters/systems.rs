use super::components::*;
use crate::application::simulation_service::SimulationService;
use crate::domain::services::physics;
use crate::domain::value_objects::simulation_params::SimulationParameters;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::time::Fixed;
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
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    // Get camera position for distance culling (using GlobalTransform to avoid conflicts)
    let camera_global = camera_query.single();
    let camera_pos = camera_global.translation();

    // First pass: collect parent positions and axial tilts
    let mut parent_positions = HashMap::new();
    let mut parent_tilts = HashMap::new();
    for (transform, planet_comp) in query.iter() {
        parent_positions.insert(
            planet_comp.domain_planet.name.clone(),
            transform.translation,
        );
        parent_tilts.insert(
            planet_comp.domain_planet.name.clone(),
            planet_comp.domain_planet.axial_tilt_deg,
        );
    }

    // Second pass: update positions with distance-based optimization
    for (mut transform, planet_comp) in query.iter_mut() {
        // Distance culling: only update objects within reasonable range of camera
        let distance_to_camera = camera_pos.distance(transform.translation);
        // Neptune at ~30 AU * 75000 scale = 2.25M units from Sun
        // Camera can view from up to 10M+ units away to see entire system
        // Set to 15M to ensure all objects update even at farthest zoom
        let max_update_distance = 15000000.0;

        if distance_to_camera > max_update_distance {
            // Skip updating distant objects for performance
            continue;
        }

        // Find parent position
        let (parent_position, parent_tilt) = if let Some(parent_name) =
            &planet_comp.domain_planet.parent_entity
        {
            // This is a moon - get its parent planet's position
            (
                *parent_positions.get(parent_name).unwrap_or(&Vec3::ZERO),
                parent_tilts.get(parent_name).copied(),
            )
        } else {
            // This is a planet orbiting the Sun
            (Vec3::ZERO, None)
        };

        // Adaptive quality: reduce Kepler iterations for distant objects
        let kepler_iterations = physics::get_kepler_iterations_for_distance(distance_to_camera);

        let new_position = physics::calculate_planet_position_with_quality(
            &planet_comp.domain_planet,
            time_days,
            &solar_params,
            parent_position,
            parent_tilt,
            kepler_iterations,
        );
        transform.translation = new_position;
    }
}

// System to update planet rotations
pub fn update_planet_rotations(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut Transform, &PlanetComponent)>,
) {
    let elapsed_seconds = time.elapsed_seconds();
    let time_days = solar_params.time_to_days(elapsed_seconds);

    for (mut transform, planet_comp) in query.iter_mut() {
        let rotation_angle =
            physics::calculate_planet_rotation(&planet_comp.domain_planet, time_days);
        let tilt_rad = planet_comp.domain_planet.axial_tilt_deg.to_radians();

        // Apply axial tilt, then spin around the tilted local Y axis.
        let tilt = Quat::from_rotation_z(tilt_rad);
        let spin = Quat::from_rotation_y(rotation_angle);
        transform.rotation = tilt * spin;
    }
}

// System to animate orbit visuals for a more dynamic presentation
// Optimized to update every 3 frames instead of every frame
pub fn update_orbit_visuals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<&OrbitComponent>,
) {
    // Performance optimization: update orbit visuals only every 3 frames
    // Visual pulsing is slow enough that this is imperceptible
    let frame_number = (time.elapsed_seconds() * 60.0) as u32; // Assume 60 FPS
    if frame_number % 3 != 0 {
        return;
    }

    let elapsed = time.elapsed_seconds();

    for orbit in query.iter() {
        let wobble_phase = elapsed * orbit.wobble_speed + orbit.phase;

        // Don't modify transform - keep the rotation set at spawn time for precision
        // This ensures moons follow their orbital paths exactly

        if let Some(material) = materials.get_mut(&orbit.material) {
            // Smooth, slow pulsing for aesthetic effect
            let pulse = 0.5 + 0.5 * (wobble_phase * 0.4).sin();
            let alpha = (0.25 + 0.15 * pulse).clamp(0.20, 0.45); // More visible, smoother pulse
            material.base_color = orbit.base_color.with_alpha(alpha);
            let linear: LinearRgba = orbit.base_color.into();
            let glow = 0.6 + 0.35 * pulse; // Stronger base glow with subtle pulse
            material.emissive = LinearRgba::new(
                linear.red * glow,
                linear.green * glow,
                linear.blue * glow,
                1.0,
            );
        }
    }
}

// System to update moon orbit positions to follow their parent planets
pub fn update_moon_orbit_positions(
    mut moon_orbit_query: Query<(&mut Transform, &OrbitComponent), With<MoonOrbit>>,
    planet_query: Query<(&Transform, &PlanetComponent), Without<MoonOrbit>>,
) {
    for (mut orbit_transform, orbit_comp) in moon_orbit_query.iter_mut() {
        // Get the parent planet's position
        if let Ok((parent_transform, parent_comp)) = planet_query.get(orbit_comp.planet_entity) {
            // Update orbit position and align the orbit plane with the parent axial tilt
            orbit_transform.translation = parent_transform.translation;
            orbit_transform.rotation =
                Quat::from_rotation_z(parent_comp.domain_planet.axial_tilt_deg.to_radians());
        }
    }
}

// System to toggle orbit visibility based on show_orbits parameter
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    mut orbit_query: Query<&mut Visibility, With<OrbitComponent>>,
) {
    for mut visibility in orbit_query.iter_mut() {
        *visibility = if solar_params.show_orbits {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

// System to add dynamic specular reflection response for planet materials
// Optimized to update every 5 frames (material properties don't change dynamically)
pub fn update_planet_reflections(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    // Performance optimization: skip most updates since values don't change
    let frame_number = (time.elapsed_seconds() * 60.0) as u32;
    if frame_number % 5 != 0 {
        return;
    }

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

    for (_selectable, mut transform, global_transform) in query.iter_mut() {
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
    mut solar_params: ResMut<SolarSystemParameters>,
    performance_stats: Res<PerformanceStats>,
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

    // Minimal, aesthetic bottom panel with transparency
    egui::TopBottomPanel::bottom("celestial_nav")
        .resizable(false)
        .frame(egui::Frame {
            fill: egui::Color32::from_rgba_premultiplied(10, 10, 15, 180), // Dark, semi-transparent
            inner_margin: egui::Margin::symmetric(12.0, 8.0),
            ..Default::default()
        })
        .show(ctx, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // Minimal label with subtle styling
                ui.label(
                    egui::RichText::new("🌌")
                        .size(14.0)
                        .color(egui::Color32::from_rgb(120, 140, 180))
                );

                ui.separator();

                // Orbit visibility toggle button
                let orbit_text = if solar_params.show_orbits { "Hide Orbits" } else { "Show Orbits" };
                let button = egui::Button::new(
                    egui::RichText::new(orbit_text).size(11.0)
                )
                .fill(if solar_params.show_orbits {
                    egui::Color32::from_rgb(40, 60, 100)
                } else {
                    egui::Color32::from_rgba_premultiplied(20, 25, 35, 150)
                })
                .stroke(if solar_params.show_orbits {
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 120, 200))
                } else {
                    egui::Stroke::NONE
                });

                if ui.add(button).clicked() {
                    solar_params.show_orbits = !solar_params.show_orbits;
                }

                ui.separator();

                // Performance display
                let fps_color = if performance_stats.fps >= 50.0 {
                    egui::Color32::from_rgb(100, 200, 100) // Green for good performance
                } else if performance_stats.fps >= 30.0 {
                    egui::Color32::from_rgb(200, 200, 100) // Yellow for acceptable
                } else {
                    egui::Color32::from_rgb(200, 100, 100) // Red for poor performance
                };

                ui.label(
                    egui::RichText::new(format!("FPS: {:.0}", performance_stats.fps))
                        .size(11.0)
                        .color(fps_color)
                );

                ui.separator();

                // Compact horizontal scroll for planet selection
                egui::ScrollArea::horizontal()
                    .id_source("celestial_nav_scroll")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0; // Tighter spacing

                            for name in ordered_names {
                                if !name_to_entity.contains_key(name) {
                                    continue;
                                }
                                let selected = selected_planet
                                    .name
                                    .as_ref()
                                    .map(|selected_name| selected_name == name)
                                    .unwrap_or(false);

                                // Aesthetic button styling
                                let button_text = egui::RichText::new(name).size(11.0);
                                let button = if selected {
                                    egui::Button::new(button_text)
                                        .fill(egui::Color32::from_rgb(40, 60, 100))
                                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(80, 120, 200)))
                                } else {
                                    egui::Button::new(button_text)
                                        .fill(egui::Color32::from_rgba_premultiplied(20, 25, 35, 150))
                                        .stroke(egui::Stroke::NONE)
                                };

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
    mut camera_query: Query<(&mut CameraController, &mut Transform)>,
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<(&PlanetComponent, &GlobalTransform)>,
    mut screenshot_state: ResMut<ScreenshotState>,
    mut notifications: ResMut<NotificationQueue>,
) {
    // Screenshot feature - F12 or P key
    // Request screenshot, will be captured next frame after notifications hide
    if keyboard.just_pressed(KeyCode::F12) || keyboard.just_pressed(KeyCode::KeyP) {
        notifications.hide_for_screenshot = true;
        screenshot_state.pending = true;
    }
    // Time scale controls
    if keyboard.pressed(KeyCode::KeyT) {
        solar_params.time_scale *= 1.1;
        println!("⏩ Time scale: {:.1}x", solar_params.time_scale);
    }
    if keyboard.pressed(KeyCode::KeyR) && solar_params.time_scale > 0.1 {
        solar_params.time_scale /= 1.1;
        println!("⏪ Time scale: {:.1}x", solar_params.time_scale);
    }

    // Reset time scale
    if keyboard.pressed(KeyCode::KeyY) {
        solar_params.time_scale = 1.0;
        println!("⏸️  Time scale reset to: {:.1}x", solar_params.time_scale);
    }

    // Toggle orbit visualization
    if keyboard.just_pressed(KeyCode::KeyO) {
        solar_params.show_orbits = !solar_params.show_orbits;
        println!(
            "🛸 Orbit visualization: {}",
            if solar_params.show_orbits {
                "ON"
            } else {
                "OFF"
            }
        );
    }

    // Quick navigation shortcuts
    if let Ok((mut controller, mut transform)) = camera_query.get_single_mut() {
        // GG (press G twice): Return to overview of entire solar system
        static mut LAST_G_PRESS: Option<std::time::Instant> = None;
        if keyboard.just_pressed(KeyCode::KeyG) {
            unsafe {
                if let Some(last_press) = LAST_G_PRESS {
                    // If pressed within 0.5 seconds, trigger action
                    if last_press.elapsed().as_secs_f32() < 0.5 {
                        transform.translation = Vec3::new(0.0, 120000.0, 1500000.0);
                        transform.look_at(Vec3::ZERO, Vec3::Y);
                        controller.velocity = Vec3::ZERO;
                        controller.speed = 5000.0;
                        println!("🏠 Returned to solar system overview (gg)");
                        LAST_G_PRESS = None;
                    } else {
                        LAST_G_PRESS = Some(std::time::Instant::now());
                    }
                } else {
                    LAST_G_PRESS = Some(std::time::Instant::now());
                }
            }
        }

        // F key: Focus on selected planet
        if keyboard.just_pressed(KeyCode::KeyF) {
            if let Some(entity) = selected_planet.entity {
                if let Ok((planet_comp, planet_transform)) = planet_query.get(entity) {
                    let planet_pos = planet_transform.translation();
                    let radius = physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params);

                    // Position camera to frame the planet nicely
                    let distance = (radius * 10.0).max(5000.0).min(500000.0);
                    let offset = Vec3::new(distance * 0.7, distance * 0.5, distance * 0.7);
                    transform.translation = planet_pos + offset;
                    transform.look_at(planet_pos, Vec3::Y);
                    controller.velocity = Vec3::ZERO;

                    // Adjust speed based on planet size
                    controller.speed = (radius * 2.0).max(50.0).min(50000.0);

                    println!("🎯 Focused on {}", planet_comp.domain_planet.name);
                }
            } else {
                println!("❌ No planet selected. Click on a planet first!");
            }
        }
    }
}

// System to update camera controller based on input
pub fn update_camera_controller(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut mouse_motion: EventReader<MouseMotion>,
    mut mouse_wheel: EventReader<MouseWheel>,
    mut contexts: EguiContexts,
    mut query: Query<(&mut CameraController, &mut Transform)>,
    mut notifications: ResMut<NotificationQueue>,
) {
    // Check if egui is using the mouse (hovering over UI)
    let ui_has_pointer = contexts.ctx_mut().is_pointer_over_area();
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

        // Handle mouse wheel for zooming and speed adjustment (only if not over UI)
        if !ui_has_pointer {
            for wheel_event in mouse_wheel.read() {
                if keyboard.pressed(KeyCode::ControlLeft) || keyboard.pressed(KeyCode::ControlRight) {
                    // Ctrl+Wheel: Adjust base movement speed
                    let speed_change = wheel_event.y * controller.speed * 0.15;
                    controller.speed = (controller.speed + speed_change).clamp(controller.min_speed, controller.max_speed);
                    println!("Camera speed: {:.0} units/s", controller.speed);
                } else if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
                    // Shift+Wheel: Adjust zoom sensitivity
                    let sensitivity_change = wheel_event.y * 5.0;
                    controller.zoom_sensitivity = (controller.zoom_sensitivity + sensitivity_change).clamp(0.1, 500.0);

                    // Show notification with current zoom sensitivity
                    notifications.notifications.push(Notification {
                        message: format!("Zoom Sensitivity: {:.1}", controller.zoom_sensitivity),
                        notification_type: NotificationType::Info,
                        created_at: time.elapsed_seconds(),
                        duration: 2.0,
                    });
                } else {
                    // Normal wheel: Zoom in/out
                    let zoom_distance = wheel_event.y * controller.speed * controller.zoom_sensitivity;
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
    }
}

// Comprehensive yet aesthetic info card for educational purposes
pub fn display_hover_info(mut contexts: EguiContexts, selected_planet: Res<SelectedPlanet>) {
    if let Some(name) = &selected_planet.name {
        let ctx = contexts.ctx_mut();
        let screen_rect = ctx.screen_rect();

        // Elegant card in right side with comprehensive educational info
        let card_width = 380.0; // Fixed width for consistent layout
        let margin = 20.0; // Margin from edges (same for all sides)
        let max_height = screen_rect.height() - (margin * 2.0); // Leave space for top and bottom margins

        egui::Window::new("info_card")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-margin, margin)) // Right margin and top margin
            .fixed_size(egui::vec2(card_width, max_height)) // Fixed width, max height
            .vscroll(false) // Disable window-level vertical scroll
            .frame(egui::Frame {
                fill: egui::Color32::from_rgba_premultiplied(8, 10, 16, 220), // Semi-transparent dark
                stroke: egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(50, 70, 110, 120)),
                rounding: egui::Rounding::same(12.0),
                shadow: egui::Shadow {
                    offset: egui::vec2(0.0, 4.0),
                    blur: 16.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_premultiplied(0, 0, 0, 120),
                },
                inner_margin: egui::Margin::symmetric(16.0, 14.0),
                ..Default::default()
            })
            .show(ctx, |ui| {
                // Elegant header with icon and name
                ui.horizontal(|ui| {
                    let (icon, color) = get_celestial_icon_and_color(name);
                    ui.label(
                        egui::RichText::new(icon)
                            .size(32.0)
                            .color(color),
                    );
                    ui.add_space(10.0);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(name)
                                .size(20.0)
                                .color(egui::Color32::from_rgb(230, 240, 255))
                                .strong(),
                        );
                        let subtitle = get_celestial_type(name);
                        ui.label(
                            egui::RichText::new(subtitle)
                                .size(11.0)
                                .color(egui::Color32::from_rgb(130, 150, 180)),
                        );
                    });
                });

                ui.add_space(12.0);
                ui.separator();
                ui.add_space(10.0);

                // Scrollable content area - auto-sizes to content, scrolls only when needed
                egui::ScrollArea::vertical()
                    .auto_shrink([false, true]) // Shrink vertically to content
                    .max_height(max_height - 100.0) // Reserve space for header
                    .show(ui, |ui| {
                        // Physical characteristics section
                        section_header(ui, "📊 Physical Characteristics");
                        display_celestial_info(ui, name);

                        ui.add_space(12.0);

                        // Interesting facts section
                        section_header(ui, "✨ Interesting Facts");
                        display_fun_facts(ui, name);

                        ui.add_space(12.0);

                        // Exploration status section
                        section_header(ui, "🚀 Exploration");
                        display_exploration_status(ui, name);
                    });
            });
    }
}

// Section header helper
fn section_header(ui: &mut egui::Ui, title: &str) {
    ui.label(
        egui::RichText::new(title)
            .size(13.0)
            .color(egui::Color32::from_rgb(150, 180, 220))
            .strong(),
    );
    ui.add_space(6.0);
}

// Get celestial body type subtitle
fn get_celestial_type(name: &str) -> &'static str {
    match name {
        "Sun" => "G-type Main Sequence Star",
        "Mercury" | "Venus" | "Earth" | "Mars" => "Terrestrial Planet",
        "Jupiter" | "Saturn" => "Gas Giant",
        "Uranus" | "Neptune" => "Ice Giant",
        "Moon" | "Phobos" | "Deimos" | "Io" | "Europa" | "Ganymede" | "Callisto" |
        "Mimas" | "Enceladus" | "Tethys" | "Dione" | "Rhea" | "Titan" | "Hyperion" |
        "Iapetus" | "Miranda" | "Ariel" | "Umbriel" | "Titania" | "Oberon" |
        "Triton" | "Proteus" | "Nereid" | "Larissa" => "Natural Satellite",
        _ => "Celestial Body"
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
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "The solar system's central star, a hot ball of plasma powering planetary climates. Its gravity holds every planet, moon, and comet in orbit.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(
                ui,
                "Stellar Classification",
                "G-type main-sequence star (G2V)",
            );
            display_info_section(ui, "Mass", "1.9885 × 10³⁰ kg (333,000 Earth masses)");
            display_info_section(ui, "Radius", "696,340 km (109 Earth radii)");
            display_info_section(ui, "Surface Temperature", "5,778 K (5,505°C)");
            display_info_section(ui, "Composition", "About 74% hydrogen, 24% helium (by mass)");
            display_info_section(ui, "Luminosity", "3.83 × 10²⁶ W");
            display_info_section(ui, "Age", "4.6 billion years");
            display_info_section(ui, "Distance from Earth", "149.6 million km (1 AU)");
        }
        "Mercury" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "The smallest planet, a cratered rocky world with extreme day-night temperatures. Its surface is heavily scarred by impacts and lacks a true atmosphere.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "3.301 × 10²³ kg (0.055 Earth masses)");
            display_info_section(ui, "Radius", "2,439.7 km (0.383 Earth radii)");
            display_info_section(ui, "Distance from Sun", "0.387 AU (57.9 million km)");
            display_info_section(ui, "Orbital Period", "87.969 Earth days");
            display_info_section(ui, "Rotation Period", "58.646 Earth days");
            display_info_section(ui, "Surface Temperature", "-173°C to 427°C");
            display_info_section(ui, "Atmosphere", "Extremely thin exosphere (sodium, oxygen)");
            display_info_section(ui, "Moons", "0");
        }
        "Venus" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "Earth-sized but shrouded in thick clouds and runaway greenhouse heat. Its surface pressure is crushing and the skies glow with sulfuric acid haze.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "4.867 × 10²⁴ kg (0.815 Earth masses)");
            display_info_section(ui, "Radius", "6,051.8 km (0.949 Earth radii)");
            display_info_section(ui, "Distance from Sun", "0.723 AU (108.2 million km)");
            display_info_section(ui, "Orbital Period", "224.701 Earth days");
            display_info_section(ui, "Rotation Period", "243.025 Earth days (retrograde)");
            display_info_section(ui, "Surface Temperature", "462°C (hottest planet)");
            display_info_section(ui, "Atmosphere", "96.5% CO2, sulfuric acid clouds");
        }
        "Earth" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A water-rich world with life, continents, and a protective atmosphere. Active geology and plate tectonics continually reshape its surface.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "5.972 × 10²⁴ kg");
            display_info_section(ui, "Radius", "6,371 km");
            display_info_section(ui, "Distance from Sun", "1.000 AU (149.6 million km)");
            display_info_section(ui, "Orbital Period", "365.256 days");
            display_info_section(ui, "Rotation Period", "23.934 hours");
            display_info_section(ui, "Surface Temperature", "-89°C to 58°C");
            display_info_section(ui, "Moons", "1 (The Moon)");
            display_info_section(ui, "Atmosphere", "78% N2, 21% O2, trace gases");
        }
        "Mars" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A cold desert planet with ancient rivers, volcanoes, and polar ice caps. Its thin air and dusty surface hint at a wetter past.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Terrestrial planet");
            display_info_section(ui, "Mass", "6.417 × 10²³ kg (0.107 Earth masses)");
            display_info_section(ui, "Radius", "3,389.5 km (0.532 Earth radii)");
            display_info_section(ui, "Distance from Sun", "1.524 AU (227.9 million km)");
            display_info_section(ui, "Orbital Period", "686.980 Earth days");
            display_info_section(ui, "Rotation Period", "24.623 hours");
            display_info_section(ui, "Surface Temperature", "-87°C to -5°C");
            display_info_section(ui, "Moons", "2 (Phobos, Deimos)");
            display_info_section(ui, "Atmosphere", "95% CO2, thin and dusty");
        }
        "Jupiter" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "The largest planet, a swirling gas giant with powerful storms. Its massive gravity shapes the outer solar system and shepherds many moons.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Gas giant");
            display_info_section(ui, "Mass", "1.898 × 10²⁷ kg (317.8 Earth masses)");
            display_info_section(ui, "Radius", "69,911 km (10.97 Earth radii)");
            display_info_section(ui, "Distance from Sun", "5.204 AU (778.6 million km)");
            display_info_section(ui, "Orbital Period", "11.862 years");
            display_info_section(ui, "Rotation Period", "9.925 hours");
            display_info_section(
                ui,
                "Moons",
                "95+ (4 Galilean: Io, Europa, Ganymede, Callisto)",
            );
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, ammonia clouds");
        }
        "Saturn" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A gas giant famous for its bright rings and diverse moon system. The rings are made of countless icy fragments from tiny grains to boulders.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Gas giant");
            display_info_section(ui, "Mass", "5.683 × 10²⁶ kg (95.2 Earth masses)");
            display_info_section(ui, "Radius", "58,232 km (9.14 Earth radii)");
            display_info_section(ui, "Distance from Sun", "9.582 AU (1.433 billion km)");
            display_info_section(ui, "Orbital Period", "29.457 years");
            display_info_section(ui, "Rotation Period", "10.7 hours");
            display_info_section(ui, "Moons", "146+ (major: Titan, Enceladus, Mimas)");
            display_info_section(ui, "Rings", "Complex ring system of ice and rock");
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, trace methane");
        }
        "Uranus" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "An ice giant tipped on its side with faint rings and icy moons. Its tilted spin creates extreme seasons lasting decades.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "8.681 × 10²⁵ kg (14.5 Earth masses)");
            display_info_section(ui, "Radius", "25,362 km (4.01 Earth radii)");
            display_info_section(ui, "Distance from Sun", "19.201 AU (2.871 billion km)");
            display_info_section(ui, "Orbital Period", "84.0205 years");
            display_info_section(ui, "Rotation Period", "17.24 hours (retrograde)");
            display_info_section(ui, "Axial Tilt", "97.77° (rotates on its side)");
            display_info_section(
                ui,
                "Moons",
                "28 (major: Titania, Oberon, Umbriel, Ariel, Miranda)",
            );
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, methane (blue color)");
        }
        "Neptune" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A distant ice giant with deep blue color and supersonic winds. Its storms can rival the scale of Earth itself.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Ice giant");
            display_info_section(ui, "Mass", "1.024 × 10²⁶ kg (17.1 Earth masses)");
            display_info_section(ui, "Radius", "24,622 km (3.88 Earth radii)");
            display_info_section(ui, "Distance from Sun", "30.047 AU (4.495 billion km)");
            display_info_section(ui, "Orbital Period", "164.8 years");
            display_info_section(ui, "Rotation Period", "16.11 hours");
            display_info_section(
                ui,
                "Wind Speed",
                "Up to 2,100 km/h (fastest in solar system)",
            );
            display_info_section(ui, "Moons", "16 (major: Triton, Proteus, Nereid)");
            display_info_section(ui, "Atmosphere", "Hydrogen, helium, methane");
        }
        "Moon" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "Earth's tidally locked companion, shaped by ancient impacts and volcanism. Its near side has dark basaltic plains formed by ancient lava flows.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Mass", "7.342 × 10²² kg (0.0123 Earth masses)");
            display_info_section(ui, "Radius", "1,737.4 km (0.273 Earth radii)");
            display_info_section(ui, "Distance from Earth", "384,400 km (0.00257 AU)");
            display_info_section(ui, "Orbital Period", "27.3217 Earth days");
            display_info_section(ui, "Rotation Period", "27.3217 Earth days (tidal locking)");
            display_info_section(ui, "Surface Gravity", "1.62 m/s² (16.6% of Earth)");
            display_info_section(ui, "Surface", "Regolith with basaltic maria");
        }
        "Phobos" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A small, irregular moon that orbits Mars faster than Mars spins. It is slowly spiraling inward and may one day break apart.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Mars");
            display_info_section(ui, "Discovery", "1877 (Asaph Hall)");
            display_info_section(ui, "Notable", "Fastest-orbiting moon in the solar system");
        }
        "Deimos" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A tiny, dark moon likely captured from the asteroid belt. Its surface is dusty and covered with fine regolith.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Mars");
            display_info_section(ui, "Discovery", "1877 (Asaph Hall)");
            display_info_section(ui, "Notable", "Likely captured asteroid");
        }
        "Io" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A sulfur-rich moon whose surface is constantly reshaped by volcanism. Lava flows and plumes repaint it in yellows, reds, and blacks.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Most volcanically active body");
        }
        "Europa" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "An icy moon with a likely ocean beneath its fractured crust. Its surface cracks may be driven by tidal flexing from Jupiter.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Global subsurface ocean likely");
        }
        "Ganymede" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "The largest moon, a layered ice-rock world with its own magnetic field. It may hide a salty ocean deep below its icy shell.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Largest moon; has magnetic field");
        }
        "Callisto" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A heavily cratered moon preserving one of the oldest surfaces. Its ancient terrain records billions of years of impacts.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Jupiter");
            display_info_section(ui, "Discovery", "1610 (Galileo)");
            display_info_section(ui, "Notable", "Heavily cratered ancient surface");
        }
        "Mimas" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A small icy moon dominated by a giant impact crater. The Herschel crater gives it a distinctive appearance.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1789 (William Herschel)");
            display_info_section(ui, "Notable", "Herschel crater dominates surface");
        }
        "Enceladus" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "An icy moon venting water jets from a subsurface ocean. These plumes feed Saturn's E ring with fresh ice.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1789 (William Herschel)");
            display_info_section(ui, "Notable", "Active geysers, subsurface ocean");
        }
        "Tethys" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A bright, icy moon with a vast canyon system. Its terrain is dominated by ancient fractures and impact basins.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1684 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Ithaca Chasma canyon system");
        }
        "Dione" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "An icy moon with bright fractures and ancient cratered plains. Its surface shows evidence of past internal activity.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1684 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Wispy icy cliffs");
        }
        "Rhea" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "Saturn's second-largest moon, mostly water ice and rock. It has a heavily cratered surface and a thin exosphere.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1672 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Second-largest moon of Saturn");
        }
        "Titan" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A hazy moon with rivers and lakes of liquid methane. Its thick atmosphere hides a complex, Earth-like weather cycle.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1655 (Christiaan Huygens)");
            display_info_section(ui, "Notable", "Thick atmosphere and methane lakes");
        }
        "Hyperion" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A sponge-like moon with an irregular shape and chaotic spin. Its low density suggests a highly porous interior.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1848 (Bond and Lassell)");
            display_info_section(ui, "Notable", "Chaotic rotation and porous interior");
        }
        "Iapetus" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A two-tone moon with a striking equatorial ridge. One hemisphere is dark and the other is bright, creating a stark contrast.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Saturn");
            display_info_section(ui, "Discovery", "1671 (Jean-Dominique Cassini)");
            display_info_section(ui, "Notable", "Two-tone surface and equatorial ridge");
        }
        "Miranda" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A small moon with dramatic cliffs and patchwork terrain. Its fractured surface hints at a complex geological past.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1948 (Gerard Kuiper)");
            display_info_section(ui, "Notable", "Patchwork terrain with giant cliffs");
        }
        "Ariel" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A bright icy moon with fault valleys and resurfaced plains. Smooth areas suggest flows of icy material long ago.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1851 (William Lassell)");
            display_info_section(ui, "Notable", "Faulted surface and possible cryovolcanism");
        }
        "Umbriel" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A dark, ancient moon with a subdued, heavily cratered surface. It reflects little sunlight and appears charcoal colored.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1851 (William Lassell)");
            display_info_section(ui, "Notable", "Darkest of Uranus's major moons");
        }
        "Titania" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "Uranus's largest moon with canyons and evidence of past activity. Its cracks suggest the crust stretched over time.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1787 (William Herschel)");
            display_info_section(ui, "Notable", "Largest moon of Uranus");
        }
        "Oberon" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A distant, cratered moon with icy ejecta deposits. Its surface preserves an ancient record of impacts.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Uranus");
            display_info_section(ui, "Discovery", "1787 (William Herschel)");
            display_info_section(ui, "Notable", "Heavily cratered icy surface");
        }
        "Triton" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A captured Kuiper Belt object with icy geysers and retrograde orbit. Its surface is coated with nitrogen and carbon dioxide frosts.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1846 (William Lassell)");
            display_info_section(ui, "Notable", "Retrograde orbit and geysers");
        }
        "Proteus" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A dark, irregular moon close to Neptune with a battered surface. Large craters suggest a violent impact history.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1989 (Voyager 2)");
            display_info_section(ui, "Notable", "Largest regular moon of Neptune");
        }
        "Nereid" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A small moon on an unusually elongated orbit around Neptune. Its path takes it far from the planet compared to most moons.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1949 (Gerard Kuiper)");
            display_info_section(ui, "Notable", "Highly eccentric orbit");
        }
        "Larissa" => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A small inner moon orbiting within Neptune's faint ring system. It is irregularly shaped and likely heavily cratered.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", "Neptune");
            display_info_section(ui, "Discovery", "1989 (Voyager 2)");
            display_info_section(ui, "Notable", "Irregular moon inside ring system");
        }
        _ => {
            display_section_header(ui, "Overview");
            display_info_section(
                ui,
                "Description",
                "A natural satellite in the solar system. Each moon has its own unique history of impacts and evolution.",
            );
            display_section_header(ui, "Key Data");
            display_info_section(ui, "Type", "Natural satellite");
            display_info_section(ui, "Parent Body", get_parent_body(name));
            display_info_section(ui, "Discovery", get_discovery_info(name));
        }
    }
}

// Helper function to display information sections with consistent spacing
fn display_info_section(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(
        egui::RichText::new(format!("{}:", label))
            .size(13.0)
            .color(egui::Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(2.0);
    ui.indent(format!("{}_value", label), |ui| {
        ui.add(
            egui::Label::new(
                egui::RichText::new(value)
                    .size(13.0)
                    .color(egui::Color32::from_rgb(226, 232, 240)),
            )
            .wrap(),
        );
    });
    ui.add_space(6.0); // Consistent spacing between sections
}

fn display_section_header(ui: &mut egui::Ui, title: &str) {
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(title)
            .size(13.0)
            .color(egui::Color32::from_rgb(251, 191, 36)),
    );
    ui.add_space(4.0);
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
    mut camera_query: Query<(&CameraController, &mut Transform, &Projection)>,
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

    let (controller, mut camera_transform, projection) = match camera_query.get_single_mut() {
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
        state.orbit_angle += time.delta_seconds() * 0.15; // Slow orbit
    }

    if let (Some(axis_dir), Some(up)) = (moon_axis, moon_up) {
        // Frame the moon large in the foreground with the parent in the background.
        let distance = moon_distance.unwrap_or(target_distance);
        let smooth_lerp = 1.0 - (-3.0 * time.delta_seconds()).exp();
        state.smooth_axis = if state.smooth_axis.length_squared() > 0.0 {
            state.smooth_axis.lerp(axis_dir, smooth_lerp).normalize_or_zero()
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

    let smooth_factor = if is_moon {
        1.0 - (-2.5 * time.delta_seconds()).exp()
    } else {
        1.0 - (-4.0 * time.delta_seconds()).exp()
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
    let lerp_factor = 1.0 - (-2.5 * time.delta_seconds()).exp(); // Smoother transitions
    camera_transform.translation = camera_transform.translation.lerp(target_pos, lerp_factor);

    // Look at the focus point to frame moon + parent when applicable
    camera_transform.look_at(state.smooth_focus, Vec3::Y);
}

// Get aesthetic viewing parameters for different celestial bodies
fn get_aesthetic_view_params(name: &str) -> (f32, f32) {
    // Returns (distance_multiplier, elevation_offset)
    match name {
        "Sun" => (1.2, 0.2),          // Further back, slightly elevated for the massive sun
        "Saturn" => (1.15, 0.3),      // Extra elevation to showcase rings
        "Jupiter" => (1.1, 0.25),     // Show off the gas giant with nice elevation
        "Earth" | "Mars" => (0.95, 0.35), // Closer, higher angle for detail
        "Moon" | "Io" | "Europa" | "Titan" | "Triton" => (0.95, 0.35), // Keep close while showing parent
        "Neptune" | "Uranus" => (1.0, 0.3), // Ice giants with good elevation
        _ => (1.0, 0.3), // Default: standard distance, nice 3/4 view
    }
}

// Performance monitoring and automatic quality adjustment system
pub fn update_performance_stats(
    time: Res<Time>,
    mut performance_stats: ResMut<PerformanceStats>,
    mut solar_params: ResMut<SolarSystemParameters>,
) {
    // Update frame time and FPS
    let frame_time_ms = time.delta_seconds() * 1000.0;
    performance_stats.frame_time = frame_time_ms;
    performance_stats.fps = if frame_time_ms > 0.0 { 1000.0 / frame_time_ms } else { 0.0 };
    performance_stats.frame_count += 1;

    // Update rolling average frame time (exponential moving average)
    const ALPHA: f32 = 0.1; // Smoothing factor
    performance_stats.average_frame_time = performance_stats.average_frame_time * (1.0 - ALPHA) + frame_time_ms * ALPHA;

    // Automatic quality adjustment based on frame time
    if performance_stats.adaptive_enabled {
        adjust_quality_based_on_performance(&mut performance_stats, &mut solar_params);
    }
}

// Automatic quality adjustment based on performance metrics
fn adjust_quality_based_on_performance(
    performance_stats: &mut PerformanceStats,
    solar_params: &mut SolarSystemParameters,
) {
    let target_frame_time = 1000.0 / performance_stats.target_fps;
    let current_avg_time = performance_stats.average_frame_time;

    // Quality adjustment thresholds
    let ultra_threshold = target_frame_time * 0.8;    // 80% of target = excellent performance
    let high_threshold = target_frame_time * 1.0;     // At target = good performance
    let medium_threshold = target_frame_time * 1.3;   // 30% over target = acceptable
    let low_threshold = target_frame_time * 1.6;      // 60% over target = poor performance

    let new_quality_level = if current_avg_time < ultra_threshold {
        QualityLevel::Ultra
    } else if current_avg_time < high_threshold {
        QualityLevel::High
    } else if current_avg_time < medium_threshold {
        QualityLevel::Medium
    } else if current_avg_time < low_threshold {
        QualityLevel::Low
    } else {
        QualityLevel::Minimal
    };

    // Only change quality if we've moved significantly
    if new_quality_level != performance_stats.quality_level {
        performance_stats.quality_level = new_quality_level;
        apply_quality_settings(new_quality_level, solar_params);
    }
}

// Apply quality settings based on the quality level
fn apply_quality_settings(quality_level: QualityLevel, solar_params: &mut SolarSystemParameters) {
    match quality_level {
        QualityLevel::Ultra => {
            // Maximum quality - no performance optimizations
            solar_params.time_scale = 1.0;
            solar_params.show_orbits = true;
            println!("🎯 Performance excellent - Quality set to Ultra");
        }
        QualityLevel::High => {
            // High quality with minimal optimizations
            solar_params.time_scale = 1.0;
            solar_params.show_orbits = true;
            println!("✅ Performance good - Quality set to High");
        }
        QualityLevel::Medium => {
            // Balanced quality and performance
            solar_params.time_scale = 0.8;
            solar_params.show_orbits = true;
            println!("⚖️ Performance moderate - Quality set to Medium");
        }
        QualityLevel::Low => {
            // Lower quality for better performance
            solar_params.time_scale = 0.5;
            solar_params.show_orbits = false;
            println!("⚡ Performance low - Quality set to Low (reduced orbits, slower time)");
        }
        QualityLevel::Minimal => {
            // Minimum quality for maximum performance
            solar_params.time_scale = 0.2;
            solar_params.show_orbits = false;
            println!("🚀 Performance critical - Quality set to Minimal (maximum optimizations)");
        }
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
}

// System to display notifications as toast-style messages
pub fn display_notifications(
    mut contexts: EguiContexts,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    use bevy_egui::egui;

    // Don't display notifications during screenshot capture
    if notifications.hide_for_screenshot {
        notifications.hide_for_screenshot = false; // Reset flag after one frame
        return;
    }

    let current_time = time.elapsed_seconds();
    let ctx = contexts.ctx_mut();

    // Remove expired notifications
    notifications.notifications.retain(|n| {
        current_time - n.created_at < n.duration
    });

    // Display each notification
    let mut y_offset = 80.0; // Start from bottom with some padding
    for (idx, notification) in notifications.notifications.iter().enumerate() {
        let remaining_time = notification.duration - (current_time - notification.created_at);
        let alpha = if remaining_time < 0.5 {
            // Fade out in the last 0.5 seconds
            (remaining_time / 0.5 * 255.0) as u8
        } else {
            255
        };

        // Choose colors based on notification type
        let bg_color = match notification.notification_type {
            NotificationType::Success => egui::Color32::from_rgba_premultiplied(16, 64, 16, alpha.min(220)),
            NotificationType::Error => egui::Color32::from_rgba_premultiplied(64, 16, 16, alpha.min(220)),
            NotificationType::Info => egui::Color32::from_rgba_premultiplied(16, 32, 64, alpha.min(220)),
        };

        egui::Window::new(format!("notification_{}", idx))
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -y_offset))
            .frame(egui::Frame {
                fill: bg_color,
                rounding: egui::Rounding::same(8.0),
                stroke: egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, alpha / 3)),
                inner_margin: egui::Margin::symmetric(16.0, 12.0),
                outer_margin: egui::Margin::ZERO,
                shadow: egui::epaint::Shadow {
                    offset: egui::vec2(0.0, 4.0),
                    blur: 12.0,
                    spread: 0.0,
                    color: egui::Color32::from_rgba_premultiplied(0, 0, 0, (alpha / 2).min(100)),
                },
            })
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(&notification.message)
                        .size(14.0)
                        .color(egui::Color32::from_rgba_premultiplied(255, 255, 255, alpha))
                );
            });

        y_offset += 60.0; // Stack notifications vertically
    }
}

// System to capture screenshot on next frame after notifications are hidden
pub fn take_pending_screenshot(
    mut screenshot_state: ResMut<ScreenshotState>,
    mut screenshot_manager: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    main_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    if !screenshot_state.pending {
        return;
    }

    screenshot_state.pending = false;

    let window_entity = main_window.single();

    // Create screenshots directory in home folder
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let screenshots_dir = format!("{}/cosmic_systems_images", home_dir);

    if let Err(e) = std::fs::create_dir_all(&screenshots_dir) {
        notifications.notifications.push(Notification {
            message: format!("Failed to create screenshots directory: {}", e),
            notification_type: NotificationType::Error,
            created_at: time.elapsed_seconds(),
            duration: 5.0,
        });
        return;
    }

    // Generate filename with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let filename = format!("{}/cosmic_systems_{}.png", screenshots_dir, timestamp);

    // Take screenshot using Bevy's screenshot API
    match screenshot_manager.save_screenshot_to_disk(window_entity, filename.clone()) {
        Ok(_) => {
            notifications.notifications.push(Notification {
                message: format!("Screenshot saved to: {}", filename),
                notification_type: NotificationType::Success,
                created_at: time.elapsed_seconds(),
                duration: 4.0,
            });
        }
        Err(e) => {
            notifications.notifications.push(Notification {
                message: format!("Failed to save screenshot: {}", e),
                notification_type: NotificationType::Error,
                created_at: time.elapsed_seconds(),
                duration: 5.0,
            });
        }
    }
}
