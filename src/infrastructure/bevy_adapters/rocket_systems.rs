use crate::domain::services::gravity::gravitational_acceleration;
use crate::domain::services::rocket_dynamics::rocket_inertia_tensor;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::*;
use bevy::math::DVec3;
use bevy::prelude::*;

/// Compute authoritative gravitational acceleration for each rocket from its
/// dominant body (see [`RocketPlanetBinding`]) and store it for the force
/// accumulation stage. Gravity uses the rocket's f64 planet-centered inertial
/// position directly and the single gravity implementation in
/// `domain::services::gravity`.
pub fn update_rocket_gravity(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketComponent,
        &mut GravityAcceleration,
    )>,
) {
    for (binding, rocket, mut gravity) in rocket_query.iter_mut() {
        let Some((planet, _)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        gravity.value = gravitational_acceleration(
            planet.domain_planet.mass_kg,
            rocket.dynamics.position_m,
            DVec3::ZERO,
        );
    }
}

/// Accumulate the net force acting on each rocket: gravity (from
/// [`GravityAcceleration`]) plus commanded thrust. Forces are in the
/// planet-centered inertial meter frame.
pub fn accumulate_forces(mut rocket_query: Query<(&mut RocketComponent, &GravityAcceleration)>) {
    for (mut rocket, gravity) in rocket_query.iter_mut() {
        let gravity_force = gravity.value * rocket.dynamics.mass_kg;
        rocket.force_accum_n = gravity_force + rocket.thrust.as_dvec3();
    }
}

/// Accumulate the net torque acting on each rocket (body frame). Commanded
/// control torque is the only input until propulsion/gimbal lands.
pub fn accumulate_torques(mut rocket_query: Query<&mut RocketComponent>) {
    for mut rocket in rocket_query.iter_mut() {
        rocket.torque_accum_nm = rocket.control_torque_nm.as_dvec3();
    }
}

/// Integrate the authoritative 6-DOF dynamics (semi-implicit Euler in f64),
/// apply simplified propellant depletion, and refresh the inertia tensor /
/// center of mass as mass changes. Resets the force and torque accumulators.
pub fn integrate_6dof(time: Res<Time>, mut rocket_query: Query<&mut RocketComponent>) {
    let dt = time.delta_secs() as f64;

    for mut rocket in rocket_query.iter_mut() {
        let force = rocket.force_accum_n;
        let torque = rocket.torque_accum_nm;
        rocket.dynamics.integrate_translation(force, dt);
        rocket.dynamics.integrate_rotation(torque, dt);

        // Simplified propellant depletion (kept from the previous behavior);
        // the propulsion change will replace this with an authoritative model.
        if rocket.fuel_mass > 0.0 && rocket.thrust.length() > 0.0 {
            const MASS_FLOW_RATE_KG_S: f32 = 100.0;
            rocket.fuel_mass = (rocket.fuel_mass - MASS_FLOW_RATE_KG_S * dt as f32).max(0.0);
            let dry = rocket.dry_mass_kg as f64;
            let fuel = rocket.fuel_mass as f64;
            rocket.dynamics.mass_kg = dry + fuel;
            let (inertia, com) =
                rocket_inertia_tensor(dry, fuel, rocket.radius_m as f64, rocket.height_m as f64);
            rocket.dynamics.inertia_body = inertia;
            rocket.dynamics.center_of_mass_m = com;
        }
        rocket.mass = rocket.dynamics.mass_kg as f32;

        rocket.force_accum_n = DVec3::ZERO;
        rocket.torque_accum_nm = DVec3::ZERO;
    }
}

/// Sync the rocket's rendered [`Transform`] and the f32 facade fields from the
/// authoritative f64 dynamics state. This is the only system that writes the
/// rocket's `Transform`.
pub fn sync_render_transform(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    physical_scale: Res<PhysicalScale>,
    mut rocket_query: Query<
        (&RocketPlanetBinding, &mut RocketComponent, &mut Transform),
        Without<PlanetComponent>,
    >,
) {
    for (binding, mut rocket, mut transform) in rocket_query.iter_mut() {
        let Some((_, planet_transform)) = planet_query
            .iter()
            .find(|(planet, _)| planet.domain_planet.name == binding.planet_name)
        else {
            continue;
        };

        let solar_display = planet_transform.translation.as_dvec3()
            + DVec3::new(
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.x),
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.y),
                physical_scale.solar_meters_to_units(rocket.dynamics.position_m.z),
            );

        transform.translation = solar_display.as_vec3();
        transform.rotation = rocket.dynamics.orientation.as_quat();

        // Refresh the compatible facade fields from the authoritative state.
        rocket.position = transform.translation;
        rocket.velocity = rocket.dynamics.velocity_mps.as_vec3();
        rocket.orientation = rocket.dynamics.orientation.as_quat();
        rocket.angular_velocity = rocket.dynamics.angular_velocity_radps.as_vec3();
    }
}

// System to handle rocket controls (placeholder)
// Issues commanded thrust and control torque; physics (not this system)
// integrates motion. Replaced by the guidance/control change later.
pub fn update_rocket_controls(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<&mut RocketComponent>,
) {
    for mut rocket in rocket_query.iter_mut() {
        // Simple thrust command (world frame, newtons)
        let mut thrust = Vec3::ZERO;

        if keyboard_input.pressed(KeyCode::Space) {
            thrust.y = 100000.0; // Upward thrust
        }

        // Basic attitude torque commands (body frame, N·m)
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
        rocket.control_torque_nm = torque;
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
            let distance_to_terrain = rocket_transform
                .translation
                .distance(terrain_transform.translation);
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
                &images,
            );

            // Apply terrain effects
            apply_terrain_effects(
                &mut rocket,
                rocket_transform,
                terrain_height,
                terrain_transform,
            );
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
    if relative_position.x < -half_size
        || relative_position.x > half_size
        || relative_position.z < -half_size
        || relative_position.z > half_size
    {
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
            rocket.mission_state =
                crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::Landed;
        }
    }

    // Update mission state based on height
    if height_above_terrain > 100.0 && rocket.mission_state == crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::PreLaunch {
        rocket.mission_state = crate::infrastructure::bevy_adapters::entity_components::RocketMissionState::Launch;
    }
}
