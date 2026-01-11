use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;

// System to update rocket physics
pub fn update_rocket_physics(
    time: Res<Time>,
    mut rocket_query: Query<(&mut RocketComponent, &mut Transform)>,
) {
    let dt = time.delta_secs();

    for (mut rocket, mut transform) in rocket_query.iter_mut() {
        // Basic physics integration
        // F = ma, so a = F/m
        let acceleration = rocket.thrust / rocket.mass;
        let current_velocity = rocket.velocity;

        // Update velocity and position
        rocket.velocity += acceleration * dt;
        rocket.position += current_velocity * dt;

        // Update angular velocity
        rocket.orientation = rocket.orientation * Quat::from_vec4(rocket.angular_velocity.extend(0.0)) * dt;

        // Update transform
        transform.translation = rocket.position;
        transform.rotation = rocket.orientation;

        // Fuel consumption (simplified)
        if rocket.fuel_mass > 0.0 && rocket.thrust.length() > 0.0 {
            let mass_flow_rate = 100.0; // kg/s - simplified
            rocket.fuel_mass = (rocket.fuel_mass - mass_flow_rate * dt).max(0.0);
            rocket.mass = rocket.dry_mass_kg + rocket.fuel_mass;
        }
    }
}

// System to handle rocket controls (placeholder)
pub fn update_rocket_controls(
    time: Res<Time>,
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<&mut RocketComponent>,
) {
    let dt = time.delta_secs();

    for mut rocket in rocket_query.iter_mut() {
        // Simple thrust control
        let mut thrust = Vec3::ZERO;

        if keyboard_input.pressed(KeyCode::Space) {
            thrust.y = 100000.0; // Upward thrust
        }

        // Basic attitude control
        let mut torque = Vec3::ZERO;

        if keyboard_input.pressed(KeyCode::KeyW) {
            torque.x = 10.0; // Pitch up
        }
        if keyboard_input.pressed(KeyCode::KeyS) {
            torque.x = -10.0; // Pitch down
        }
        if keyboard_input.pressed(KeyCode::KeyA) {
            torque.z = 10.0; // Roll left
        }
        if keyboard_input.pressed(KeyCode::KeyD) {
            torque.z = -10.0; // Roll right
        }

        rocket.thrust = thrust;
        rocket.angular_velocity += torque * dt;
    }
}

/// System to handle rocket interaction with terrain
/// Enables rockets to launch from and land on terrain surfaces
pub fn update_rocket_terrain_interaction(
    mut rocket_query: Query<(&mut RocketComponent, &Transform)>,
    terrain_query: Query<(&TerrainComponent, &Transform)>,
    images: Res<Assets<Image>>,
) {
    for (mut rocket, rocket_transform) in rocket_query.iter_mut() {
        // Find nearest terrain within influence distance
        let mut nearest_terrain: Option<(&TerrainComponent, &Transform, f32)> = None;

        for (terrain, terrain_transform) in terrain_query.iter() {
            let distance_to_terrain = rocket_transform.translation.distance(terrain_transform.translation);
            let influence_distance = terrain.size_km * 500.0; // Within terrain influence range

            if distance_to_terrain < influence_distance {
                if let Some((_, _, current_min_distance)) = nearest_terrain {
                    if distance_to_terrain < current_min_distance {
                        nearest_terrain = Some((terrain, terrain_transform, distance_to_terrain));
                    }
                } else {
                    nearest_terrain = Some((terrain, terrain_transform, distance_to_terrain));
                }
            }
        }

        if let Some((terrain, terrain_transform, _)) = nearest_terrain {
            // Sample terrain height at rocket position relative to terrain
            let terrain_height = sample_terrain_height(
                rocket_transform.translation - terrain_transform.translation,
                terrain,
                &images
            );

            // Apply terrain effects
            apply_terrain_effects(&mut rocket, rocket_transform, terrain_height, terrain_transform);
        }
    }
}

/// Sample terrain height at a given position relative to terrain center
fn sample_terrain_height(
    relative_position: Vec3,
    terrain: &TerrainComponent,
    images: &Assets<Image>,
) -> f32 {
    // Convert world position to terrain local coordinates
    let terrain_size_m = terrain.size_km * 1000.0;
    let half_size = terrain_size_m / 2.0;

    // Check if position is within terrain bounds
    if relative_position.x < -half_size || relative_position.x > half_size ||
       relative_position.z < -half_size || relative_position.z > half_size {
        return 0.0; // Outside terrain, assume sea level
    }

    // Convert to texture coordinates (0-1 range)
    let u = (relative_position.x + half_size) / terrain_size_m;
    let v = (relative_position.z + half_size) / terrain_size_m;

    // Sample heightmap
    if let Some(heightmap_image) = images.get(&terrain.heightmap) {
        if let Some(data) = &heightmap_image.data {
            let width = heightmap_image.width() as usize;
            let height = heightmap_image.height() as usize;

            // Convert UV to pixel coordinates
            let x = (u * (width - 1) as f32) as usize;
            let y = (v * (height - 1) as f32) as usize;

            let pixel_index = y * width + x;
            if pixel_index < data.len() {
                let height_normalized = data[pixel_index] as f32 / 255.0;

                // Get height range based on terrain type
                let (height_min, height_max) = match terrain.launch_site_type {
                    crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType::KennedySpaceCenter => (-10.0, 10.0),
                    crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType::RtlsLandingPad => (-8.0, 8.0),
                    crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType::DroneShip => (-2.0, 2.0),
                    crate::infrastructure::bevy_adapters::entity_components::LaunchSiteType::LunarLanding => (-10.0, 10.0),
                };

                return height_min + height_normalized * (height_max - height_min);
            }
        }
    }

    0.0 // Default height if sampling fails
}

/// Apply terrain effects to rocket (collision detection, ground effect, etc.)
fn apply_terrain_effects(
    rocket: &mut RocketComponent,
    rocket_transform: &Transform,
    terrain_height: f32,
    terrain_transform: &Transform,
) {
    // Calculate rocket's height above terrain
    let rocket_world_height = rocket_transform.translation.y;
    let terrain_world_height = terrain_transform.translation.y + terrain_height;

    let height_above_terrain = rocket_world_height - terrain_world_height;

    // Simple collision detection and ground effect
    if height_above_terrain < 5.0 && rocket.velocity.y < 0.0 {
        // Rocket is close to ground and descending - apply ground effect
        let ground_effect_strength = (5.0 - height_above_terrain).max(0.0) / 5.0;
        let ground_effect_force = Vec3::new(0.0, ground_effect_strength * 50000.0, 0.0);

        // Add ground effect to thrust (simplified)
        rocket.thrust += ground_effect_force;

        // If very close to ground and moving slowly, simulate landing
        if height_above_terrain < 1.0 && rocket.velocity.length() < 10.0 {
            rocket.velocity.y = rocket.velocity.y.max(0.0); // Stop downward motion
            rocket.mission_state = crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::Landed;
        }
    }

    // Update mission state based on height
    if height_above_terrain > 100.0 && rocket.mission_state == crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::PreLaunch {
        rocket.mission_state = crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::Launch;
    }
}