// Rocket camera mode systems for different viewing perspectives.

use crate::components::rocket::*;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::math::{Quat, Vec3};
use bevy::prelude::*;

/// System to handle rocket camera mode input and transitions.
pub fn handle_rocket_camera_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_mode: ResMut<RocketCameraMode>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    // Cycle camera modes with number keys or C key.
    // Free mode is deliberately NOT offered: Rocket Mode keeps the camera
    // locked to the vehicle (no free spectator flight).
    if keyboard.just_pressed(KeyCode::Digit1) {
        *camera_mode = RocketCameraMode::Chase;
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        *camera_mode = RocketCameraMode::Cockpit;
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        *camera_mode = RocketCameraMode::Orbital;
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        *camera_mode = RocketCameraMode::Surface;
    } else if keyboard.just_pressed(KeyCode::KeyC) {
        // Cycle through modes (Free excluded)
        *camera_mode = match *camera_mode {
            RocketCameraMode::Chase => RocketCameraMode::Cockpit,
            RocketCameraMode::Cockpit => RocketCameraMode::Orbital,
            RocketCameraMode::Orbital => RocketCameraMode::Surface,
            RocketCameraMode::Surface => RocketCameraMode::Chase,
            RocketCameraMode::Free => RocketCameraMode::Chase,
        };
    }

    // Update controller target mode
    for mut controller in controller_query.iter_mut() {
        controller.target_mode = *camera_mode;
    }
}

/// System to update rocket camera based on current mode.
/// Operates in flight units (1 unit = 1 meter) using the rocket's Transform
/// which is already in flight units via sync_render_transform.
pub fn update_rocket_camera(
    time: Res<Time>,
    camera_mode: Res<RocketCameraMode>,
    config: Res<RocketCameraConfig>,
    render_origin: Res<RenderOrigin>,
    planet_query: Query<(&PlanetComponent, &Transform), Without<Camera3d>>,
    rocket_query: Query<
        (
            &RocketPlanetBinding,
            &RocketPhysicsState,
            &RocketGeometry,
            &RocketFacade,
            &Transform,
            &RocketMissionState,
        ),
        Without<Camera3d>,
    >,
    mut camera_query: Query<(&mut Transform, &mut Projection), With<Camera3d>>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    let dt = time.delta_secs();

    // Get the rocket entity (assume single rocket for now)
    let Some((binding, rocket_physics, _geometry, _facade, rocket_transform, _mission_state)) =
        rocket_query.iter().next()
    else {
        return;
    };

    let Some((_planet, _planet_transform)) = planet_query
        .iter()
        .find(|(p, _)| p.domain_planet.name == binding.planet_name)
    else {
        return;
    };

    // Rocket position in flight units (meters) from its Transform.
    let rocket_pos_flight = rocket_transform.translation;

    // Planet center in flight units: -render_origin.origin converted to flight units
    let planet_center_flight = -render_origin.origin.as_vec3();

    // Up direction in flight frame: radial from planet center to rocket.
    let up_dir = (rocket_pos_flight - planet_center_flight).normalize_or_zero();

    // Rocket orientation in flight frame (already correct from sync_render_transform)
    let rocket_rot = rocket_transform.rotation;

    // Compute forward direction (rocket's forward in flight frame)
    let forward_dir = (rocket_rot * Vec3::Y).normalize();

    // Compute right direction: cross of forward and up.
    // At spawn, forward (body +Y) aligns with up (radial), so cross is zero.
    // Fall back to a horizontal reference (e.g., planet north or arbitrary perpendicular).
    let right_dir = forward_dir.cross(up_dir);
    let right_dir = if right_dir.length_squared() < 1e-6 {
        // Forward and up are parallel; use an arbitrary perpendicular vector.
        // Choose a vector perpendicular to up_dir.
        if up_dir.z.abs() < 0.9 {
            up_dir.cross(Vec3::Z).normalize()
        } else {
            up_dir.cross(Vec3::X).normalize()
        }
    } else {
        right_dir.normalize()
    };

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

        // Compute target camera transform in flight units (meters)
        let (target_pos, target_rot) = match controller.current_mode {
            RocketCameraMode::Chase => {
                compute_chase_camera(rocket_pos_flight, forward_dir, up_dir, right_dir, &config)
            }
            RocketCameraMode::Cockpit => {
                compute_cockpit_camera(rocket_pos_flight, rocket_rot, &config)
            }
            RocketCameraMode::Orbital => {
                compute_orbital_camera(rocket_pos_flight, planet_center_flight, up_dir, &config)
            }
            RocketCameraMode::Surface => compute_surface_camera(
                rocket_pos_flight,
                planet_center_flight,
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
        for (mut cam_transform, _projection) in camera_query.iter_mut() {
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
    // If rocket is nearly vertical (forward ≈ up), "behind" would be underground.
    // Instead, position camera to the side and above for a clear pad view.
    let vertical_alignment = forward.dot(up).abs();
    let offset = if vertical_alignment > 0.95 {
        // Rocket is vertical: use side offset (right vector) + up offset
        right * config.chase_distance + up * config.chase_height
    } else {
        // Rocket is tilted: traditional chase behind and above
        -forward * config.chase_distance + up * config.chase_height
    };
    let target_pos = rocket_pos + offset;
    // Look at the rocket's center (half height up) so the entire vehicle is
    // framed, including engines at the base and nose at the top. The rocket
    // geometry is 70m tall; center is at ~35m in the flight frame.
    let rocket_center = rocket_pos + up * 35.0;
    // Frame the rocket upright against the terrain: `looking_at` keeps the
    // camera's up on the radial direction so the ground sits at the bottom of
    // the view. `from_rotation_arc` (previously used) rolls the view
    // arbitrarily, which put the terrain on the left/right of the screen.
    let target_rot = Transform::from_translation(target_pos)
        .looking_at(rocket_center, up)
        .rotation;
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
    planet_pos: Vec3,
    up_dir: Vec3,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    // Position camera at an angle to show the orbital plane
    let orbit_radius = config.orbital_distance;
    let elevation = config.orbital_elevation;

    // Compute a position that shows the orbit from an inclined perspective
    let forward = (rocket_pos - planet_pos).normalize();
    let right = up_dir.cross(forward).normalize();

    // Orbital camera position: offset from rocket in orbital plane
    let offset = right * orbit_radius * 0.5 + up_dir * orbit_radius * elevation;
    let target_pos = rocket_pos + offset;

    // Look toward the planet/rocket
    let look_dir = (planet_pos - target_pos).normalize();
    let target_rot = Quat::from_rotation_arc(-Vec3::Z, look_dir);

    (target_pos, target_rot)
}

/// Surface camera: planet-relative for landing.
fn compute_surface_camera(
    rocket_pos: Vec3,
    planet_pos: Vec3,
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
    rocket_binding_query: Query<&RocketPlanetBinding>,
    planet_query: Query<&PlanetComponent>,
    mut camera_query: Query<&mut Projection, With<Camera3d>>,
) {
    if *camera_mode == RocketCameraMode::Free {
        return;
    }

    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };
    let Some(binding) = rocket_binding_query.iter().next() else {
        return;
    };

    let altitude = rocket.dynamics.position_m.length();
    let planet_radius = planet_query
        .iter()
        .find(|p| p.domain_planet.name == binding.planet_name)
        .map(|p| p.domain_planet.radius_km as f64 * 1000.0)
        .unwrap_or(6_371_000.0);
    let height_above_surface = (altitude - planet_radius).max(0.0);

    for projection in camera_query.iter_mut() {
        if let Projection::Perspective(proj) = projection.into_inner() {
            // Adjust near/far planes based on altitude and mode.
            // Far stays small enough to exclude the solar-system's giant
            // spheres (Sun shell at ~22,835 units) from the flight frame.
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
                    // Launch-pad view: far plane large enough to show Earth curvature
                    // and horizon. From 100m altitude, horizon is ~36km away.
                    (0.5, 100_000.0)
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
