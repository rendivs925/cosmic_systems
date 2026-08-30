use super::components::*;
use super::craft_components::{CraftCameraTag, CraftTravelTarget};
use crate::domain::services::physics;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::planet_systems::solar_map_render_translation;
use bevy::input::mouse::MouseButton;
use bevy::math::DVec3;
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
#[expect(
    clippy::too_many_arguments,
    reason = "This selection system receives independent ECS resources and queries."
)]
#[expect(
    clippy::type_complexity,
    reason = "The camera filter supports the shared solar and craft camera infrastructure."
)]
pub fn handle_mouse_planet_selection(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    camera_query: Query<(&Camera, &Transform), Or<(With<CameraController>, With<CraftCameraTag>)>>,
    windows: Query<&Window>,
    solar_params: Res<SolarSystemParameters>,
    origin: Res<SolarMapRenderOrigin>,
    ui_state: Res<UiPointerState>,
    mut selected_planet: ResMut<SelectedPlanet>,
    mut craft_target: Option<ResMut<CraftTravelTarget>>,
    mut selectable_query: Query<(Entity, &mut Selectable, &PlanetComponent, &SolarMapPosition)>,
) {
    // Only handle left mouse button clicks
    if !mouse_buttons.just_pressed(MouseButton::Left) {
        return;
    }
    if ui_state.is_over_ui {
        return;
    }

    let Some((camera, camera_transform)) = camera_query.iter().find(|(camera, _)| camera.is_active)
    else {
        return;
    };
    let window = match windows.single() {
        Ok(window) => window,
        Err(_) => return,
    };
    let cursor_pos = match window.cursor_position() {
        Some(pos) => pos,
        None => return,
    };
    let camera_global = GlobalTransform::from(*camera_transform);
    let ray = match camera.viewport_to_world(&camera_global, cursor_pos) {
        Ok(ray) => ray,
        Err(_) => return,
    };

    // Raycast against planet spheres to find the clicked body.
    let mut closest_entity: Option<Entity> = None;
    let mut closest_t = f32::INFINITY;

    for (entity, _selectable, planet_comp, position) in selectable_query.iter() {
        let radius = if planet_comp.domain_planet.name == "Sun" {
            physics::calculate_sun_visual_radius(&solar_params)
        } else {
            physics::calculate_visual_radius(&planet_comp.domain_planet, &solar_params)
        };
        let center = solar_map_render_translation(position.0, origin.position_units);
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
            if let Some(target) = craft_target.as_mut() {
                target.entity = None;
                target.name = None;
            }
            println!("Deselected planet (clicked on selected)");
        } else {
            // Clicking on different planet - select it
            if let Ok((_, selectable, _, _)) = selectable_query.get(selected_entity) {
                selected_planet.entity = Some(selected_entity);
                selected_planet.name = Some(selectable.name.clone());
                if let Some(target) = craft_target.as_mut() {
                    target.entity = Some(selected_entity);
                    target.name = Some(selectable.name.clone());
                    println!("Craft traveling to {}", selectable.name);
                }
                println!("Selected planet: {}", selectable.name);
            }
        }
    } else {
        // Clicked on empty space - only deselect if something is selected
        if selected_planet.entity.is_some() {
            selected_planet.entity = None;
            selected_planet.name = None;
            if let Some(target) = craft_target.as_mut() {
                target.entity = None;
                target.name = None;
            }
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

// System to handle solar system controls (time scale, etc.)
#[expect(
    clippy::too_many_arguments,
    reason = "This input system mutates independent shared simulation resources."
)]
pub fn handle_solar_system_input(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut simulation_time: ResMut<SimulationTime>,
    mut solar_params: ResMut<SolarSystemParameters>,
    mut camera_query: Query<(&mut CameraController, &mut Transform)>,
    selected_planet: Res<SelectedPlanet>,
    planet_query: Query<(&PlanetComponent, &SolarMapPosition)>,
    origin: Res<SolarMapRenderOrigin>,
    mut camera_command: ResMut<SolarMapCameraCommand>,
    mut zen_mode: ResMut<crate::infrastructure::bevy_adapters::components::ZenMode>,
    mut camera_input_state: ResMut<CameraInputState>,
) {
    if keyboard.just_pressed(KeyCode::KeyZ) {
        zen_mode.enabled = !zen_mode.enabled;
        println!("Zen mode: {}", if zen_mode.enabled { "ON" } else { "OFF" });
    }
    // Time scale controls (require Ctrl key)
    if keyboard.just_pressed(KeyCode::KeyT)
        && keyboard.pressed(KeyCode::ControlLeft)
        && keyboard.pressed(KeyCode::ShiftLeft)
    {
        // Exponential increase: 10x
        let target_scale =
            (simulation_time.time_acceleration * 10.0).min(SimulationTime::ACCEL_10000X);
        simulation_time.set_time_acceleration(target_scale);
        println!(
            "Time scale: {:.0}x (10x increase)",
            simulation_time.time_acceleration
        );
    } else if keyboard.just_pressed(KeyCode::KeyT) && keyboard.pressed(KeyCode::ControlLeft) {
        // Gradual increase: 10%
        let target_scale =
            (simulation_time.time_acceleration * 1.1).min(SimulationTime::ACCEL_10000X);
        simulation_time.set_time_acceleration(target_scale);
        println!("Time scale: {:.1}x", simulation_time.time_acceleration);
    }

    if keyboard.just_pressed(KeyCode::KeyR)
        && keyboard.pressed(KeyCode::ControlLeft)
        && keyboard.pressed(KeyCode::ShiftLeft)
        && simulation_time.time_acceleration > 0.1
    {
        // Exponential decrease: 10x
        let target_scale = (simulation_time.time_acceleration / 10.0).max(0.1);
        simulation_time.set_time_acceleration(target_scale);
        println!(
            "Time scale: {:.0}x (10x decrease)",
            simulation_time.time_acceleration
        );
    } else if keyboard.just_pressed(KeyCode::KeyR)
        && keyboard.pressed(KeyCode::ControlLeft)
        && simulation_time.time_acceleration > 0.1
    {
        // Gradual decrease: 10%
        let target_scale = (simulation_time.time_acceleration / 1.1).max(0.1);
        simulation_time.set_time_acceleration(target_scale);
        println!("Time scale: {:.1}x", simulation_time.time_acceleration);
    }

    // Reset time scale
    if keyboard.just_pressed(KeyCode::KeyY) && keyboard.pressed(KeyCode::ControlLeft) {
        simulation_time.set_time_acceleration(SimulationTime::REALTIME);
        println!(
            "Time scale reset to: {:.1}x",
            simulation_time.time_acceleration
        );
    }

    // Toggle orbit visualization
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

    // Quick navigation shortcuts
    if let Ok((mut controller, mut transform)) = camera_query.single_mut() {
        // GG (press G twice): Return to overview of entire solar system.
        if keyboard.just_pressed(KeyCode::KeyG) {
            let is_double_press = camera_input_state
                .last_overview_key_press_s
                .is_some_and(|last_press_s| time.elapsed_secs() - last_press_s < 0.5);
            camera_input_state.last_overview_key_press_s = Some(time.elapsed_secs());
            if is_double_press {
                camera_command.position_units = Some(DVec3::new(0.0, 120000.0, 1500000.0));
                camera_command.look_at_units = Some(DVec3::ZERO);
                controller.velocity = Vec3::ZERO;
                controller.speed = 5000.0;
                camera_input_state.last_overview_key_press_s = None;
                println!("Returned to solar system overview (gg)");
            }
        }

        // F key: Focus on selected planet (or terrain view for Earth with Ctrl)
        if keyboard.just_pressed(KeyCode::KeyF) {
            println!("F key pressed!");
            if let Some(entity) = selected_planet.entity {
                if let Ok((planet_comp, planet_transform)) = planet_query.get(entity) {
                    // Check if Earth is selected and Ctrl is pressed - toggle terrain view
                    if planet_comp.domain_planet.name == "Earth"
                        && keyboard.pressed(KeyCode::ControlLeft)
                    {
                        if camera_input_state.earth_terrain_active {
                            // Deactivate terrain view
                            println!(
                                "Ctrl+F detected on Earth - deactivating terrain flag and mode"
                            );
                            camera_input_state.earth_terrain_active = false;
                            controller.mode = CameraMode::FreeFlight;
                            println!(
                                "Terrain view deactivated for Earth (flag: {})",
                                camera_input_state.earth_terrain_active
                            );
                        } else {
                            // Activate terrain view
                            println!("Ctrl+F detected on Earth - setting terrain flag and mode");
                            camera_input_state.earth_terrain_active = true;
                            controller.mode = CameraMode::TerrainView;
                            // Position camera above the current Earth position for terrain view
                            let earth_pos = solar_map_render_translation(
                                planet_transform.0,
                                origin.position_units,
                            );
                            let terrain_height_above_earth = 6371.0 + 100.0; // Earth radius + 100m above surface
                            transform.translation =
                                earth_pos + Vec3::new(0.0, terrain_height_above_earth, 0.0);
                            transform.look_at(earth_pos, Vec3::Y);
                            controller.velocity = Vec3::ZERO;
                            println!(
                                "Terrain view activated for Earth (flag: {})",
                                camera_input_state.earth_terrain_active
                            );
                        }
                    } else {
                        // Normal focus on planet
                        let radius = physics::calculate_visual_radius(
                            &planet_comp.domain_planet,
                            &solar_params,
                        );

                        // The selected-body presentation system owns framing.
                        controller.velocity = Vec3::ZERO;

                        // Adjust speed based on planet size
                        controller.speed = (radius * 2.0).clamp(50.0, 50000.0);

                        println!("Focused on {}", planet_comp.domain_planet.name);
                    }
                }
            } else {
                println!("No planet selected. Click on a planet first!");
            }
        }
    }
}
