use super::components::*;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::PerformanceStats;
use bevy::input::mouse::MouseButton;
use bevy::prelude::*;

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

    // Deselect with Escape (idempotent - only deselect if something is selected)
    if keyboard.just_pressed(KeyCode::Escape) && selected_planet.entity.is_some() {
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
    ui_state: Res<UiPointerState>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut selectable_query: Query<(Entity, &mut Selectable, &PlanetComponent, &GlobalTransform)>,
) {
    // Only handle left mouse button clicks
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if ui_state.is_over_ui {
        return;
    }

    let (camera, camera_transform) = match camera_query.single() {
        Ok(result) => result,
        Err(_) => return,
    };
    let window = match windows.single() {
        Ok(window) => window,
        Err(_) => return,
    };
    let cursor_pos = match window.cursor_position() {
        Some(pos) => pos,
        None => return,
    };
    let ray = match camera.viewport_to_world(camera_transform, cursor_pos) {
        Ok(ray) => ray,
        Err(_) => return,
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

    // Update selection (idempotent - clicking selected planet deselects it)
    if let Some(selected_entity) = closest_entity {
        // Check if this planet is already selected
        if selected_planet.entity == Some(selected_entity) {
            // Clicking on already selected planet - deselect it
            selected_planet.entity = None;
            selected_planet.name = None;
            println!("Deselected planet (clicked on selected)");
        } else {
            // Clicking on different planet - select it
            if let Ok((_, selectable, _, _)) = selectable_query.get(selected_entity) {
                selected_planet.entity = Some(selected_entity);
                selected_planet.name = Some(selectable.name.clone());
                println!("Selected planet: {}", selectable.name);
            }
        }
    } else {
        // Clicked on empty space - only deselect if something is selected
        if selected_planet.entity.is_some() {
            selected_planet.entity = None;
            selected_planet.name = None;
            println!("Deselected planet (clicked on empty space)");
        }
        // If nothing is selected, clicking empty space does nothing (idempotent)
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
    let camera_pos = camera_query.single().unwrap().translation();

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

// System to handle solar system controls (time scale, etc.)
pub fn handle_solar_system_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut solar_params: ResMut<SolarSystemParameters>,
    mut perf_stats: ResMut<PerformanceStats>,
    mut camera_query: Query<(&mut CameraController, &mut Transform)>,
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<(&PlanetComponent, &GlobalTransform)>,
    mut screenshot_state: ResMut<ScreenshotState>,
    mut notifications: ResMut<NotificationQueue>,
    mut zen_mode: ResMut<crate::infrastructure::bevy_adapters::components::ZenMode>,
) {
    // Screenshot feature - F12 or P key
    // Request screenshot, will be captured next frame after notifications hide
    if keyboard.just_pressed(KeyCode::F12) || keyboard.just_pressed(KeyCode::KeyP) {
        notifications.hide_for_screenshot = true;
        screenshot_state.pending = true;
    }
    if keyboard.just_pressed(KeyCode::KeyZ) {
        zen_mode.enabled = !zen_mode.enabled;
        println!("🧘 Zen mode: {}", if zen_mode.enabled { "ON" } else { "OFF" });
    }
    // Time scale controls (require Ctrl key)
    if keyboard.just_pressed(KeyCode::KeyT) && keyboard.pressed(KeyCode::ControlLeft) && keyboard.pressed(KeyCode::ShiftLeft) {
        // Exponential increase: 10x
        solar_params.time_scale = (solar_params.time_scale * 10.0).min(10000000.0);
        println!("🚀 Time scale: {:.0}x (10x increase)", solar_params.time_scale);
    } else if keyboard.just_pressed(KeyCode::KeyT) && keyboard.pressed(KeyCode::ControlLeft) {
        // Gradual increase: 10%
        solar_params.time_scale = (solar_params.time_scale * 1.1).max(0.0001);
        println!("⏩ Time scale: {:.1}x", solar_params.time_scale);
    }

    if keyboard.just_pressed(KeyCode::KeyR) && keyboard.pressed(KeyCode::ControlLeft) && keyboard.pressed(KeyCode::ShiftLeft) && solar_params.time_scale > 0.1 {
        // Exponential decrease: 10x
        solar_params.time_scale = (solar_params.time_scale / 10.0).max(0.0001);
        println!("🐌 Time scale: {:.0}x (10x decrease)", solar_params.time_scale);
    } else if keyboard.just_pressed(KeyCode::KeyR) && keyboard.pressed(KeyCode::ControlLeft) && solar_params.time_scale > 0.1 {
        // Gradual decrease: 10%
        solar_params.time_scale = (solar_params.time_scale / 1.1).max(0.0001);
        println!("⏪ Time scale: {:.1}x", solar_params.time_scale);
    }

    // Reset time scale
    if keyboard.just_pressed(KeyCode::KeyY) && keyboard.pressed(KeyCode::ControlLeft) {
        solar_params.time_scale = 1.0;
        println!("⏸️ Time scale reset to: {:.1}x", solar_params.time_scale);
    }

    // Toggle automatic quality adaptation
    if keyboard.just_pressed(KeyCode::KeyA) && keyboard.pressed(KeyCode::ControlLeft) && keyboard.pressed(KeyCode::ShiftLeft) {
        perf_stats.adaptive_enabled = !perf_stats.adaptive_enabled;
        println!("🎛️ Automatic quality adaptation: {}", if perf_stats.adaptive_enabled { "ENABLED" } else { "DISABLED (manual control)" });
        if !perf_stats.adaptive_enabled {
            println!("💡 Use Ctrl+T/Ctrl+R to manually adjust time scale");
        }
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
    if let Ok((mut controller, mut transform)) = camera_query.single_mut() {
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

        // F key: Focus on selected planet (or terrain view for Earth with Ctrl)
        if keyboard.just_pressed(KeyCode::KeyF) {
            if let Some(entity) = selected_planet.entity {
                if let Ok((planet_comp, planet_transform)) = planet_query.get(entity) {
                    // Check if Earth is selected and Ctrl is pressed - switch to terrain view
                    if planet_comp.domain_planet.name == "Earth" && keyboard.pressed(KeyCode::ControlLeft) {
                        controller.mode = CameraMode::TerrainView;
                        // Position camera at terrain level
                        transform.translation = Vec3::new(0.0, 100.0, 0.0); // Above Kennedy Space Center
                        transform.look_at(Vec3::ZERO, Vec3::Y);
                        controller.velocity = Vec3::ZERO;
                        println!("🌍 Switched to terrain view for Earth (Ctrl+F)");
                    } else {
                        // Normal focus on planet
                        let planet_pos = planet_transform.translation();
                        let radius =
                            physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params);

                        // Position camera to frame the planet nicely
                        let distance = (radius * 10.0).clamp(5000.0, 500000.0);
                        let offset = Vec3::new(distance * 0.7, distance * 0.5, distance * 0.7);
                        transform.translation = planet_pos + offset;
                        transform.look_at(planet_pos, Vec3::Y);
                        controller.velocity = Vec3::ZERO;

                        // Adjust speed based on planet size
                        controller.speed = (radius * 2.0).clamp(50.0, 50000.0);

                        println!("🎯 Focused on {}", planet_comp.domain_planet.name);
                    }
                }
            } else {
                println!("❌ No planet selected. Click on a planet first!");
            }
        }
    }
}