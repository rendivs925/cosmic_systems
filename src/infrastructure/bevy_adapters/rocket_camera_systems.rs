// Rocket camera mode systems for different viewing perspectives.

use crate::components::rocket::*;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use bevy::math::{DVec3, Quat, Vec3};
use bevy::prelude::*;

/// System to handle rocket camera mode input and transitions.
pub fn handle_rocket_camera_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_mode: ResMut<RocketCameraMode>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    // Cycle camera modes with number keys or C key
    if keyboard.just_pressed(KeyCode::Digit1) {
        *camera_mode = RocketCameraMode::Chase;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        *camera_mode = RocketCameraMode::Cockpit;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        *camera_mode = RocketCameraMode::Orbital;
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        *camera_mode = RocketCameraMode::Surface;
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        *camera_mode = RocketCameraMode::Free;
    } else if keyboard.just_pressed(KeyCode::KeyC) {
        // Cycle through modes
        *camera_mode = match *camera_mode {
            RocketCameraMode::Chase => RocketCameraMode::Cockpit,
            RocketCameraMode::Cockpit => RocketCameraMode::Orbital,
            RocketCameraMode::Orbital => RocketCameraMode::Surface,
            RocketCameraMode::Surface => RocketCameraMode::Free,
            RocketCameraMode::Free => RocketCameraMode::Chase,
        };
    }

    // Update controller target mode
    for mut controller in controller_query.iter_mut() {
        controller.target_mode = *camera_mode;
    }
}

/// System to update rocket camera based on current mode.
pub fn update_rocket_camera(
    time: Res<Time>,
    camera_mode: Res<RocketCameraMode>,
    config: Res<RocketCameraConfig>,
    physical_scale: Res<PhysicalScale>,
    // `Without<Camera>` proves disjointness from the mutable camera query
    // below (B0001): planets/rockets never carry a Camera component.
    planet_query: Query<(&PlanetComponent, &Transform), Without<Camera>>,
    rocket_query: Query<
        (
            &RocketPlanetBinding,
            &RocketPhysicsState,
            &RocketGeometry,
            &RocketFacade,
            &Transform,
            &RocketMissionState,
        ),
        Without<Camera>,
    >,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera>>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    let dt = time.delta_secs();

    // Get the rocket entity (assume single rocket for now)
    let Some((binding, rocket_physics, _geometry, _facade, rocket_transform, mission_state)) =
        rocket_query.iter().next()
    else {
        return;
    };

    let Some((planet, planet_transform)) = planet_query
        .iter()
        .find(|(p, _)| p.domain_planet.name == binding.planet_name)
    else {
        return;
    };

    let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
    let position_m = rocket_physics.dynamics.position_m;
    let rocket_pos_solar = planet_transform.translation.as_dvec3()
        + DVec3::new(
            physical_scale.solar_meters_to_units(position_m.x),
            physical_scale.solar_meters_to_units(position_m.y),
            physical_scale.solar_meters_to_units(position_m.z),
        );
    let rocket_rot = rocket_transform.rotation;

    let planet_radius_solar = physical_scale.solar_meters_to_units(radius_m);
    let planet_pos_solar = planet_transform.translation.as_dvec3();

    // Compute up direction (from planet center to rocket)
    let up_dir = (rocket_pos_solar - planet_pos_solar)
        .normalize_or_zero()
        .as_vec3();

    // Compute forward direction (rocket's forward in solar frame)
    let forward_dir = (rocket_rot * Vec3::Y).normalize();

    // Compute right direction
    let right_dir = forward_dir.cross(up_dir).normalize();

    for mut controller in controller_query.iter_mut() {
        let smooth = config.smooth_factor * dt * 60.0; // Frame-rate independent

        // Handle mode transitions
        if controller.current_mode != controller.target_mode {
            controller.transition_progress += dt * config.transition_speed;
            if controller.transition_progress >= 1.0 {
                controller.current_mode = controller.target_mode;
                controller.transition_progress = 0.0;
            }
        } else {
            controller.transition_progress = 0.0;
        }

        // Compute target camera transform based on mode
        let (target_pos, target_rot) = match controller.current_mode {
            RocketCameraMode::Chase => compute_chase_camera(
                rocket_pos_solar.as_vec3(),
                forward_dir,
                up_dir,
                right_dir,
                &config,
            ),
            RocketCameraMode::Cockpit => {
                compute_cockpit_camera(rocket_pos_solar.as_vec3(), rocket_rot, &config)
            }
            RocketCameraMode::Orbital => compute_orbital_camera(
                rocket_pos_solar.as_vec3(),
                planet_pos_solar,
                up_dir,
                &config,
            ),
            RocketCameraMode::Surface => compute_surface_camera(
                rocket_pos_solar.as_vec3(),
                planet_pos_solar,
                up_dir,
                rocket_physics.dynamics.velocity_mps.length() as f32,
                &config,
            ),
            RocketCameraMode::Free => {
                // Free camera - don't auto-update position
                continue;
            }
        };

        // Smooth interpolation
        for (mut cam_transform, mut projection) in camera_query.iter_mut() {
            if controller.transition_progress > 0.0 {
                // Interpolate during transition
                let prev_pos = controller
                    .last_rocket_transform
                    .map(|t| t.translation)
                    .unwrap_or(cam_transform.translation);
                let prev_rot = controller
                    .last_rocket_transform
                    .map(|t| t.rotation)
                    .unwrap_or(cam_transform.rotation);

                cam_transform.translation =
                    prev_pos.lerp(target_pos, controller.transition_progress);
                cam_transform.rotation = prev_rot.slerp(target_rot, controller.transition_progress);
            } else {
                cam_transform.translation = cam_transform.translation.lerp(target_pos, smooth);
                cam_transform.rotation = cam_transform.rotation.slerp(target_rot, smooth);
            }

            // Store current transform for next transition
            controller.last_rocket_transform =
                Some(Transform::from_translation(target_pos).with_rotation(target_rot));
        }
    }
}

/// Chase camera: positioned behind and above the rocket.
fn compute_chase_camera(
    rocket_pos: Vec3,
    forward: Vec3,
    up: Vec3,
    right: Vec3,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    let offset = -forward * config.chase_distance + up * config.chase_height;
    let target_pos = rocket_pos + offset;
    let target_rot = Quat::from_rotation_arc(-Vec3::Z, -offset.normalize());
    (target_pos, target_rot)
}

/// Cockpit camera: first-person from rocket body.
fn compute_cockpit_camera(
    rocket_pos: Vec3,
    rocket_rot: Quat,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    let target_pos = rocket_pos + rocket_rot * config.cockpit_offset;
    let target_rot = rocket_rot;
    (target_pos, target_rot)
}

/// Orbital camera: inertial frame showing orbital trajectory.
fn compute_orbital_camera(
    rocket_pos: Vec3,
    planet_pos: DVec3,
    up_dir: Vec3,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    // Position camera at an angle to show the orbital plane
    let orbit_radius = config.orbital_distance;
    let elevation = config.orbital_elevation;

    // Compute a position that shows the orbit from an inclined perspective
    let forward = (rocket_pos - planet_pos.as_vec3()).normalize();
    let right = up_dir.cross(forward).normalize();

    // Orbital camera position: offset from rocket in orbital plane
    let offset = right * orbit_radius * 0.5 + up_dir * orbit_radius * elevation;
    let target_pos = rocket_pos + offset;

    // Look toward the planet/rocket
    let look_dir = (planet_pos.as_vec3() - target_pos).normalize();
    let target_rot = Quat::from_rotation_arc(-Vec3::Z, look_dir);

    (target_pos, target_rot)
}

/// Surface camera: planet-relative for landing.
fn compute_surface_camera(
    rocket_pos: Vec3,
    planet_pos: DVec3,
    up_dir: Vec3,
    speed: f32,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    // Position camera above and ahead of landing trajectory
    let distance = config.surface_distance;
    let height = config.surface_height;

    // Compute forward along velocity projected to horizontal plane
    // For now, use a fixed forward direction relative to the planet
    let forward = up_dir.cross(Vec3::Z).normalize(); // Eastward

    let offset = -forward * distance + up_dir * height;
    let target_pos = rocket_pos + offset;

    // Look down at the landing area
    let look_dir = (rocket_pos - target_pos).normalize();
    let target_rot = Quat::from_rotation_arc(-Vec3::Z, look_dir);

    (target_pos, target_rot)
}

/// System to update the camera near/far planes for rocket mode.
pub fn update_rocket_camera_projection(
    camera_mode: Res<RocketCameraMode>,
    config: Res<RocketCameraConfig>,
    rocket_query: Query<&RocketPhysicsState>,
    planet_query: Query<&PlanetComponent>,
    mut camera_query: Query<&mut Projection, With<Camera>>,
) {
    if *camera_mode == RocketCameraMode::Free {
        return;
    }

    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };

    let altitude = rocket.dynamics.position_m.length();
    let planet_radius = planet_query
        .iter()
        .next()
        .map(|p| p.domain_planet.radius_km as f64 * 1000.0)
        .unwrap_or(6_371_000.0);
    let height_above_surface = (altitude - planet_radius).max(0.0);

    for mut projection in camera_query.iter_mut() {
        if let Projection::Perspective(proj) = projection.into_inner() {
            // Adjust near/far planes based on altitude and mode
            let (near, far) = match *camera_mode {
                RocketCameraMode::Cockpit => {
                    // Very close near plane for cockpit view
                    (0.01, (height_above_surface + 1000.0) as f32)
                }
                RocketCameraMode::Surface => {
                    // Close for landing
                    (0.1, (height_above_surface + 5000.0) as f32)
                }
                RocketCameraMode::Chase => {
                    // Medium range
                    (1.0, (config.chase_distance * 3.0) as f32)
                }
                RocketCameraMode::Orbital => {
                    // Far for orbital view
                    (10.0, (config.orbital_distance * 5.0) as f32)
                }
                RocketCameraMode::Free => continue,
            };

            let lerp = 1.0 - (-2.0_f32 * 0.016).exp(); // ~60Hz
            proj.near = proj.near.lerp(near, lerp);
            proj.far = proj.far.lerp(far.clamp(proj.near * 1.1, far), lerp);
        }
    }
}
