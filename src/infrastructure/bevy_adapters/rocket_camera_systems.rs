// Rocket camera mode systems for different viewing perspectives.

use crate::components::rocket::*;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::rocket_presentation::render_dynamics_state;
use crate::infrastructure::bevy_adapters::terrain_render::RenderOrigin;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::math::{Quat, Vec3};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;

/// Adds RocketCameraController to the existing camera entity so the rocket
/// camera systems can drive it. The camera is spawned by setup_space with a
/// solar CameraController. We keep the solar CameraController marker so shared
/// systems (e.g. update_planet_positions) can still locate the camera via
/// `.single()`, but since SolarSystemModePlugin is not composed into Rocket
/// Mode, its free-flight camera system never runs — Rocket Mode owns the camera.
pub fn setup_rocket_camera_controller(
    mut commands: Commands,
    camera_query: Query<Entity, With<Camera3d>>,
) {
    for entity in camera_query.iter() {
        commands
            .entity(entity)
            .insert(RocketCameraController::default());
    }
}

/// Initializes the camera position and render origin for rocket mode.
/// The camera starts at the solar system position; this system repositions it
/// to the rocket's location on the launch pad and sets the render origin to
/// the rocket's physics position so the rocket renders near the origin.
pub fn setup_rocket_camera_and_origin(
    mut commands: Commands,
    mut camera_query: Query<(Entity, &mut Transform, &mut Projection), With<Camera3d>>,
    mut render_origin: ResMut<RenderOrigin>,
    config: Res<RocketCameraConfig>,
    rocket_query: Query<(&RocketPhysicsState, &RocketGeometry)>,
) {
    let Some((rocket, geometry)) = rocket_query.iter().next() else {
        bevy::log::warn!("setup_rocket_camera_and_origin: no rocket entity found");
        return;
    };

    // Rocket physics position in planet-centered inertial frame (meters)
    let rocket_pos_m = rocket.dynamics.position_m;

    // Set render origin to the rocket's physics position so the rocket
    // renders near the origin (flight units = meters, 1:1 scale).
    render_origin.origin = rocket_pos_m;
    render_origin.last_camera_pos = rocket_pos_m;

    // The render origin is the rocket, so its initial presentation position is
    // zero. Use the same rotated-body chase pose as the per-frame system;
    // looking at world +Y here points away from launch sites whose local up is
    // not render +Y and leaves the streamed launch terrain out of frame on the
    // first presentation frames.
    let rocket_pos_flight = Vec3::ZERO;
    let up_dir = rocket_pos_m.normalize().as_vec3();
    let (camera_pos_flight, camera_rotation) = compute_chase_camera(
        rocket_pos_flight,
        rocket.dynamics.orientation.as_quat(),
        up_dir,
        geometry,
        &config,
    );
    let camera_transform =
        Transform::from_translation(camera_pos_flight).with_rotation(camera_rotation);

    // The launch pad horizon is tens of kilometers away. Start at the chase
    // camera range so the curved Earth proxy is visible on the first frame;
    // the regular projection system maintains this range afterwards.
    for (entity, mut cam_transform, projection) in camera_query.iter_mut() {
        *cam_transform = camera_transform;
        if let Projection::Perspective(proj) = projection.into_inner() {
            proj.near = 0.5;
            proj.far = 100_000.0;
        }
        // Workaround for bevyengine/bevy#18904: with GPU preprocessing on,
        // meshes that pass CPU visibility (rocket, pad primitives) silently
        // drop out of the indirect-draw path on this driver. Drawing directly
        // keeps every visible mesh on screen.
        commands.entity(entity).insert((
            bevy::render::view::NoIndirectDrawing,
            // Presentation-only low-altitude haze makes the distant
            // terrain recede naturally without changing surface lighting.
            DistanceFog {
                color: Color::srgba(0.52, 0.68, 0.9, 1.0),
                directional_light_color: Color::srgb(1.0, 0.94, 0.8),
                directional_light_exponent: 8.0,
                falloff: FogFalloff::from_visibility(25_000.0),
            },
        ));
    }
}

/// System to handle rocket camera mode input and transitions.
pub fn handle_rocket_camera_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_mode: ResMut<RocketCameraMode>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    let requested_mode =
    // Cycle camera modes with number keys or C key.
    if keyboard.just_pressed(KeyCode::Digit1) {
        Some(RocketCameraMode::Chase)
    } else if keyboard.just_pressed(KeyCode::Digit2) {
        Some(RocketCameraMode::Cockpit)
    } else if keyboard.just_pressed(KeyCode::Digit3) {
        Some(RocketCameraMode::Orbital)
    } else if keyboard.just_pressed(KeyCode::Digit4) {
        Some(RocketCameraMode::Surface)
    } else if keyboard.just_pressed(KeyCode::Digit5) {
        Some(RocketCameraMode::Free)
    } else if keyboard.just_pressed(KeyCode::KeyC) {
        // Cycle through modes (Free now included).
        Some(match *camera_mode {
            RocketCameraMode::Chase => RocketCameraMode::Cockpit,
            RocketCameraMode::Cockpit => RocketCameraMode::Orbital,
            RocketCameraMode::Orbital => RocketCameraMode::Surface,
            RocketCameraMode::Surface => RocketCameraMode::Free,
            RocketCameraMode::Free => RocketCameraMode::Chase,
        })
    } else {
        None
    };

    let Some(requested_mode) = requested_mode else {
        return;
    };
    *camera_mode = requested_mode;

    // Reset from the currently rendered pose when a transition is retargeted.
    for mut controller in controller_query.iter_mut() {
        controller.request_mode(requested_mode);
    }
}

/// Free-fly (space) camera input: left-drag orbits the rocket, scroll zooms.
/// Stores orbit angles/distance on each controller so `update_rocket_camera`
/// can position the camera deterministically from the current rocket state.
pub fn handle_free_camera_input(
    mouse: Res<ButtonInput<MouseButton>>,
    mut scroll: MessageReader<MouseWheel>,
    mut motion: MessageReader<MouseMotion>,
    mut controller_query: Query<&mut RocketCameraController>,
) {
    let mut dragging = false;
    let mut dx = 0.0f32;
    let mut dy = 0.0f32;
    let mut scroll_y = 0.0f32;

    if mouse.pressed(MouseButton::Left) {
        dragging = true;
    }
    for e in motion.read() {
        dx += e.delta.x;
        dy += e.delta.y;
    }
    for e in scroll.read() {
        scroll_y += e.y;
    }

    if !dragging && scroll_y.abs() < 1e-6 {
        return;
    }

    for mut controller in controller_query.iter_mut() {
        if controller.current_mode != RocketCameraMode::Free {
            continue;
        }
        if dragging {
            controller.free_orbit_yaw -= dx * 0.005;
            controller.free_orbit_pitch =
                (controller.free_orbit_pitch + dy * 0.005).clamp(0.05, 1.45);
        }
        if scroll_y.abs() > 1e-6 {
            controller.free_orbit_distance =
                (controller.free_orbit_distance * (1.0 - scroll_y * 0.15)).clamp(20.0, 40_000.0);
        }
    }
}

/// System to update rocket camera based on current mode.
/// Operates in flight units (1 unit = 1 meter) using the rocket's Transform,
/// which `interpolate_render_transform` refreshes every frame from the
/// interpolated physics state. The radial frame and velocity use that same
/// interpolated f64 state, so camera inputs do not mix render-frame position
/// with a newer fixed-tick physics sample at liftoff.
#[expect(
    clippy::type_complexity,
    reason = "The camera reads the cohesive presentation state for the primary vehicle."
)]
pub fn update_rocket_camera(
    time: Res<Time>,
    fixed_time: Res<Time<Fixed>>,
    config: Res<RocketCameraConfig>,
    render_origin: Res<RenderOrigin>,
    planet_query: Query<(&PlanetComponent, &Transform), Without<Camera3d>>,
    rocket_query: Query<
        (
            &RocketPlanetBinding,
            &RocketRenderState,
            &RocketGeometry,
            &Transform,
        ),
        (Without<Camera3d>, Without<SpentStage>),
    >,
    mut camera_query: Query<(&mut Transform, &mut RocketCameraController), With<Camera3d>>,
) {
    let dt = time.delta_secs();

    // Get the rocket entity (assume single rocket for now)
    let Some((binding, render, geometry, rocket_transform)) = rocket_query.iter().next() else {
        return;
    };

    let Some((_planet, _planet_transform)) = planet_query
        .iter()
        .find(|(p, _)| p.matches_body(&binding.planet_name))
    else {
        return;
    };

    // Rocket position in flight units (meters) from its Transform.
    let rocket_pos_flight = rocket_transform.translation;
    let rendered_dynamics = render_dynamics_state(*render, fixed_time.overstep_fraction() as f64);

    // Planet center in flight units: -render_origin.origin converted to flight units
    let planet_center_flight = -render_origin.origin.as_vec3();

    // Compute radial up in the authoritative planet-centered f64 frame before
    // crossing the render boundary. Subtracting two f32 flight coordinates at
    // an Earth-radius scale loses precision near liftoff.
    let up_dir = rendered_dynamics.position_m.normalize_or_zero().as_vec3();
    if up_dir.length_squared() < 0.5 {
        return; // degenerate frame (rocket at planet center): keep last camera
    }

    // Rocket orientation in flight frame (interpolated render state).
    let rocket_rot = rocket_transform.rotation;

    for (mut camera_transform, mut controller) in camera_query.iter_mut() {
        // Compute target camera transform in flight units (meters)
        let (target_pos, target_rot) = match controller.target_mode {
            RocketCameraMode::Chase => {
                compute_chase_camera(rocket_pos_flight, rocket_rot, up_dir, geometry, &config)
            }
            RocketCameraMode::Cockpit => {
                compute_cockpit_camera(rocket_pos_flight, rocket_rot, &config)
            }
            RocketCameraMode::Orbital => compute_orbital_camera(
                rocket_pos_flight,
                planet_center_flight,
                up_dir,
                rendered_dynamics.velocity_mps.as_vec3(),
                &config,
            ),
            RocketCameraMode::Surface => compute_surface_camera(rocket_pos_flight, up_dir, &config),
            RocketCameraMode::Free => compute_free_camera(rocket_pos_flight, up_dir, &controller),
        };

        let target_pose = camera_reference_pose(
            rocket_pos_flight,
            rocket_rot,
            Transform::from_translation(target_pos).with_rotation(target_rot),
            controller.target_mode,
        );
        let transitioning = controller.current_mode != controller.target_mode;
        let current_pose = if transitioning {
            // Changing reference frames means the previous stored pose may be
            // body-relative while the new target is render-relative. Convert
            // the rendered camera pose instead of interpolating mismatched data.
            camera_reference_pose(
                rocket_pos_flight,
                rocket_rot,
                *camera_transform,
                controller.target_mode,
            )
        } else {
            controller.smoothed_pose.unwrap_or_else(|| {
                camera_reference_pose(
                    rocket_pos_flight,
                    rocket_rot,
                    *camera_transform,
                    controller.target_mode,
                )
            })
        };

        // The pose frame is selected by the target camera mode. Both supported
        // frames are relative to the rocket position, so a RenderOrigin rebase
        // shifts neither stored offset nor produces a camera cut.
        let mode_pose = if !transitioning {
            controller.cancel_transition();
            target_pose
        } else {
            let start = controller.begin_transition(current_pose);
            controller.transition_progress =
                (controller.transition_progress + dt * config.transition_speed).min(1.0);
            let linear_t = controller.transition_progress;
            let t = linear_t * linear_t * (3.0 - 2.0 * linear_t);
            let pose = interpolate_camera_pose(start, target_pose, t);

            if controller.transition_progress >= 1.0 {
                controller.complete_transition();
            }
            pose
        };
        let smoothing = camera_smoothing_alpha(config.smooth_factor, dt);
        let smoothed_pose = if transitioning {
            interpolate_camera_pose(current_pose, mode_pose, smoothing)
        } else {
            controller
                .smoothed_pose
                .map(|previous| interpolate_camera_pose(previous, mode_pose, smoothing))
                .unwrap_or(current_pose)
        };
        controller.smoothed_pose = Some(smoothed_pose);
        *camera_transform = camera_pose_from_reference(
            rocket_pos_flight,
            rocket_rot,
            smoothed_pose,
            controller.target_mode,
        );
    }
}

/// Vehicle-attached views follow the body frame. Planet-relative and free views
/// retain a render-frame orientation so landing attitude changes cannot shake
/// their horizon or orbit direction.
fn camera_uses_rocket_body_frame(mode: RocketCameraMode) -> bool {
    matches!(mode, RocketCameraMode::Chase | RocketCameraMode::Cockpit)
}

/// Convert a render-frame camera pose into the frame owned by `mode`.
fn camera_reference_pose(
    rocket_position: Vec3,
    rocket_rotation: Quat,
    camera_pose: Transform,
    mode: RocketCameraMode,
) -> Transform {
    let reference_rotation = if camera_uses_rocket_body_frame(mode) {
        rocket_rotation
    } else {
        Quat::IDENTITY
    };
    let inverse_reference_rotation = reference_rotation.inverse();
    Transform::from_translation(
        inverse_reference_rotation * (camera_pose.translation - rocket_position),
    )
    .with_rotation(inverse_reference_rotation * camera_pose.rotation)
}

/// Reapply a stored mode-reference pose to the current flight frame.
fn camera_pose_from_reference(
    rocket_position: Vec3,
    rocket_rotation: Quat,
    pose: Transform,
    mode: RocketCameraMode,
) -> Transform {
    let reference_rotation = if camera_uses_rocket_body_frame(mode) {
        rocket_rotation
    } else {
        Quat::IDENTITY
    };
    Transform::from_translation(rocket_position + reference_rotation * pose.translation)
        .with_rotation(reference_rotation * pose.rotation)
}

fn interpolate_camera_pose(from: Transform, to: Transform, amount: f32) -> Transform {
    Transform::from_translation(from.translation.lerp(to.translation, amount))
        .with_rotation(from.rotation.slerp(to.rotation, amount))
}

/// Convert the existing 60 Hz smoothing fraction into a frame-rate-independent
/// response. For the default 0.1 factor, one 1/60 s frame retains its existing
/// behavior while the same wall-clock duration converges identically at any FPS.
fn camera_smoothing_alpha(smooth_factor: f32, delta_seconds: f32) -> f32 {
    let smooth_factor = smooth_factor.clamp(0.0, 1.0);
    if smooth_factor >= 1.0 {
        return 1.0;
    }
    1.0 - (1.0 - smooth_factor).powf((delta_seconds.max(0.0)) * 60.0)
}

/// Camera rotation looking from `eye` at `target`, with an up reference that is
/// guaranteed non-parallel to the view direction. `Transform::looking_at`
/// produces a degenerate (NaN/garbage) rotation when `up ∥ view`, which showed
/// up as the camera flipping to a "random view" during vertical ascent.
fn safe_look_rotation(eye: Vec3, target: Vec3, candidates: &[Vec3; 3]) -> Quat {
    let view = (target - eye).normalize_or_zero();
    if view.length_squared() < 0.5 {
        return Quat::IDENTITY;
    }
    let mut up_ref = candidates[0];
    for &candidate in candidates {
        if candidate.length_squared() > 0.5 && view.dot(candidate).abs() < 0.95 {
            up_ref = candidate;
            break;
        }
    }
    Transform::from_translation(eye)
        .looking_at(target, up_ref)
        .rotation
}

/// Chase camera: positioned behind and above the rocket, framing the whole
/// vehicle.
///
/// The offset basis is built from the RADIAL up and the horizontal component
/// of the vehicle's body axis — never from `cross(body_axis, up)`, which is
/// degenerate while the rocket is vertical (launch). The look target is the
/// vehicle's MID-BODY point along its actual (pitched) axis, so the rocket
/// stays framed through the gravity turn instead of sliding out of frame.
fn compute_chase_camera(
    rocket_pos: Vec3,
    rocket_rot: Quat,
    up: Vec3,
    geometry: &RocketGeometry,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    let body_fwd = (rocket_rot * Vec3::Y).normalize_or_zero();
    if body_fwd.length_squared() < 0.5 {
        // Degenerate orientation: hold position.
        return (rocket_pos + up * config.chase_height, Quat::IDENTITY);
    }

    // Horizontal component of the body axis (the direction the vehicle is
    // pitched toward). While vertical this is near-zero, so fall back to a
    // stable horizontal reference perpendicular to up.
    let mut horiz = body_fwd - body_fwd.dot(up) * up;
    if horiz.length_squared() < 1.0e-4 {
        horiz = if up.z.abs() < 0.9 {
            up.cross(Vec3::Z)
        } else {
            up.cross(Vec3::X)
        };
    }
    let horiz = horiz.normalize_or_zero();
    // A stable side direction perpendicular to both up and the flight heading.
    let side = up.cross(horiz).normalize_or_zero();

    // Verticality blend: near-vertical flight uses a side view (a rear view
    // would look into the ground); pitched flight uses a rear chase view. The
    // smoothstep band prevents any per-frame flip (AGENTS.md section 48).
    let verticality = body_fwd.dot(up).abs();
    let t = ((verticality - 0.90) / (0.99 - 0.90)).clamp(0.0, 1.0);
    let vert_t = t * t * (3.0 - 2.0 * t);

    let behind_dir = -horiz;
    let offset_dir = behind_dir.lerp(side, vert_t).normalize_or_zero();
    let target_pos = rocket_pos + offset_dir * config.chase_distance + up * config.chase_height;

    // Frame the mid-body point along the vehicle's ACTUAL axis, so a pitched
    // rocket stays centered instead of drifting out of the viewport.
    let half_len = geometry.height_m * 0.5;
    let look_point = rocket_pos + body_fwd * half_len;

    let rotation = safe_look_rotation(target_pos, look_point, &[up, body_fwd, horiz]);
    (target_pos, rotation)
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
    velocity_mps: Vec3,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    // Position camera at an angle to show the orbital plane
    let orbit_radius = config.orbital_distance;
    let elevation = config.orbital_elevation;

    // Compute a position that shows the orbit from an inclined perspective
    let mut tangent = velocity_mps - up_dir * velocity_mps.dot(up_dir);
    if tangent.length_squared() < 1.0e-4 {
        tangent = if up_dir.z.abs() < 0.9 {
            up_dir.cross(Vec3::Z)
        } else {
            up_dir.cross(Vec3::X)
        };
    }
    let tangent = tangent.normalize_or_zero();
    let right = up_dir.cross(tangent).normalize_or_zero();

    // Orbital camera position: offset from rocket in orbital plane
    let offset = right * orbit_radius * 0.5 + up_dir * orbit_radius * elevation;
    let target_pos = rocket_pos + offset;

    // Look toward the planet/rocket
    let target_rot = safe_look_rotation(target_pos, planet_pos, &[up_dir, tangent, right]);

    (target_pos, target_rot)
}

/// Surface camera: planet-relative for landing.
fn compute_surface_camera(
    rocket_pos: Vec3,
    up_dir: Vec3,
    config: &RocketCameraConfig,
) -> (Vec3, Quat) {
    // Position camera above and ahead of landing trajectory
    let distance = config.surface_distance;
    let height = config.surface_height;

    // Compute forward along velocity projected to horizontal plane
    // For now, use a fixed forward direction relative to the planet
    let mut forward = up_dir.cross(Vec3::Z).normalize_or_zero();
    if forward.length_squared() < 1e-6 {
        forward = up_dir.cross(Vec3::X).normalize_or_zero();
    }

    let offset = -forward * distance + up_dir * height;
    let target_pos = rocket_pos + offset;

    // Look down at the landing area
    let look_dir = (rocket_pos - target_pos).normalize_or_zero();
    let target_rot = Quat::from_rotation_arc(-Vec3::Z, look_dir);

    (target_pos, target_rot)
}

/// Free-fly space camera: orbits the rocket at a user-controlled yaw/pitch and
/// distance (see [`handle_free_camera_input`]). The horizon is kept upright by
/// `looking_at` with the radial up, so the space view stays readable.
fn compute_free_camera(
    rocket_pos: Vec3,
    up: Vec3,
    controller: &RocketCameraController,
) -> (Vec3, Quat) {
    let yaw = controller.free_orbit_yaw;
    let pitch = controller.free_orbit_pitch.clamp(0.05, 1.45); // above horizon
    let distance = controller.free_orbit_distance.max(5.0);

    // Horizon-stable basis around the rocket.
    let mut forward = up.cross(Vec3::Z).normalize_or_zero();
    if forward.length_squared() < 1e-6 {
        forward = up.cross(Vec3::X).normalize_or_zero();
    }
    let right = forward.cross(up).normalize_or_zero();

    let horizontal = (forward * yaw.cos() + right * yaw.sin()) * pitch.cos();
    let dir = horizontal + up * pitch.sin();
    let target_pos = rocket_pos + dir * distance;
    let target_rot = Transform::from_translation(target_pos)
        .looking_at(rocket_pos, up)
        .rotation;
    (target_pos, target_rot)
}

/// System to update the camera near/far planes for rocket mode.
pub fn update_rocket_camera_projection(
    camera_mode: Res<RocketCameraMode>,
    rocket_query: Query<&RocketPhysicsState>,
    rocket_binding_query: Query<&RocketPlanetBinding>,
    planet_query: Query<&PlanetComponent>,
    mut camera_query: Query<&mut Projection, With<Camera3d>>,
) {
    let Some(rocket) = rocket_query.iter().next() else {
        return;
    };
    let Some(binding) = rocket_binding_query.iter().next() else {
        return;
    };

    let Some(planet_radius) = planet_query
        .iter()
        .find(|p| p.matches_body(&binding.planet_name))
        .map(|p| p.domain_planet.radius_km as f64 * 1000.0)
    else {
        return;
    };
    let altitude = rocket.dynamics.position_m.length();
    let height_above_surface = (altitude - planet_radius).max(0.0);
    // A planet fills the view out to its geometric horizon. Clipping before
    // that intersection turns the curved globe into a flat far-plane slice.
    let horizon_distance_m =
        (height_above_surface * (2.0 * planet_radius + height_above_surface)).sqrt();

    for projection in camera_query.iter_mut() {
        if let Projection::Perspective(proj) = projection.into_inner() {
            // Adjust near/far planes based on altitude and mode.
            // Far stays small enough to exclude the solar-system's giant
            // spheres (Sun shell at ~22,835 units) from the flight frame.
            let (near, far) = match *camera_mode {
                RocketCameraMode::Cockpit => {
                    // Retain the curved Earth horizon at altitude. Limiting the
                    // cockpit view to altitude + 1 km slices the globe at the
                    // far plane and makes it appear flat.
                    (0.01, (horizon_distance_m + 50_000.0).max(5_000.0) as f32)
                }
                RocketCameraMode::Surface => {
                    (0.1, (horizon_distance_m + 50_000.0).max(5_000.0) as f32)
                }
                RocketCameraMode::Chase => {
                    (0.5, (horizon_distance_m + 100_000.0).max(100_000.0) as f32)
                }
                RocketCameraMode::Orbital => {
                    // Earth and the predicted trajectory are centered roughly a
                    // planet radius away in the flight frame. Keep the range
                    // tied to physical altitude, not an arbitrary camera zoom.
                    (10.0, ((planet_radius + height_above_surface) * 3.0) as f32)
                }
                RocketCameraMode::Free => {
                    // Frame the whole visible Earth disk at altitude while
                    // retaining enough range for the local orbit prediction.
                    (0.5, (horizon_distance_m + 200_000.0).max(300_000.0) as f32)
                }
            };

            let lerp = 1.0 - (-2.0_f32 * 0.016).exp(); // ~60Hz
            proj.near = proj.near.lerp(near, lerp);
            proj.far = proj.far.lerp(far.clamp(proj.near * 1.1, far), lerp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_relative_camera_pose_rebases_without_a_camera_cut() {
        let rocket_rotation = Quat::from_rotation_y(0.4);
        let relative = Transform::from_translation(Vec3::new(-220.0, 50.0, 0.0))
            .with_rotation(Quat::from_rotation_x(-0.2));
        let before_rocket = Vec3::new(1_000.0, -200.0, 50.0);
        let rebase_delta = Vec3::new(10_000.0, -4_000.0, 1_000.0);

        let before = camera_pose_from_reference(
            before_rocket,
            rocket_rotation,
            relative,
            RocketCameraMode::Free,
        );
        let after = camera_pose_from_reference(
            before_rocket - rebase_delta,
            rocket_rotation,
            relative,
            RocketCameraMode::Free,
        );

        assert!((after.translation + rebase_delta - before.translation).length() < 1e-3);
        assert_eq!(after.rotation, before.rotation);
    }

    #[test]
    fn free_camera_pose_does_not_follow_a_touchdown_rotation() {
        let pose = Transform::from_translation(Vec3::new(-220.0, 50.0, 0.0))
            .with_rotation(Quat::from_rotation_x(-0.2));
        let rocket_position = Vec3::new(12.0, 4.0, -8.0);
        let before = camera_pose_from_reference(
            rocket_position,
            Quat::IDENTITY,
            pose,
            RocketCameraMode::Free,
        );
        let after = camera_pose_from_reference(
            rocket_position,
            Quat::from_rotation_z(1.2),
            pose,
            RocketCameraMode::Free,
        );

        assert_eq!(after, before);
    }

    #[test]
    fn chase_camera_pose_follows_the_rocket_body_rotation() {
        let pose = Transform::from_translation(Vec3::new(-220.0, 50.0, 0.0));
        let rocket_position = Vec3::ZERO;
        let before = camera_pose_from_reference(
            rocket_position,
            Quat::IDENTITY,
            pose,
            RocketCameraMode::Chase,
        );
        let after = camera_pose_from_reference(
            rocket_position,
            Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
            pose,
            RocketCameraMode::Chase,
        );

        assert!((after.translation - before.translation).length() > 1.0);
    }

    #[test]
    fn smoothing_response_is_independent_of_render_frame_rate() {
        let advance = |delta_seconds: f32, frames: usize| {
            (0..frames).fold(0.0, |value, _| {
                value + (1.0 - value) * camera_smoothing_alpha(0.1, delta_seconds)
            })
        };

        let at_60_fps = advance(1.0 / 60.0, 60);
        let at_120_fps = advance(1.0 / 120.0, 120);
        assert!((at_60_fps - at_120_fps).abs() < 1e-5);
    }

    #[test]
    fn surface_camera_remains_finite_at_radial_basis_singularities() {
        for up_dir in [Vec3::Z, -Vec3::Z] {
            let (position, rotation) = compute_surface_camera(
                Vec3::new(10.0, 20.0, 30.0),
                up_dir,
                &RocketCameraConfig::default(),
            );
            assert!(position.is_finite());
            assert!(rotation.is_finite());
        }
    }
}
