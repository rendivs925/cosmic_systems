// Rocket debug visualization plugin (observer of the simulation, not a
// second simulation). Reads authoritative state and draws it with gizmos.
//
// Origins reuse the same planet-translation + PhysicalScale conversion as
// `sync_render_transform` so debug geometry lands where the vehicle renders.
// Vector directions/magnitudes come from the authoritative components
// (`GravityAcceleration`, dynamics state) — nothing here re-simulates physics.

use super::components::*;
use crate::domain::services::cube_sphere::patch_world_size_m;
use crate::domain::services::reference_frames::body_fixed_to_planet_inertial_rotation;
use crate::infrastructure::bevy_adapters::entity_components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use crate::infrastructure::bevy_adapters::terrain::streaming::TerrainStreamingResource;
use bevy::math::{DQuat, DVec3, Isometry3d};
use bevy::prelude::*;

mod primitives;
mod trajectory;

use primitives::{draw_vector, find_bound_planet, offset_to_units, rocket_render_origin};
use trajectory::{draw_orbital_trajectory, update_trajectory_cache, TrajectoryCache};

/// Gizmo configuration group so rocket debugging has independent visibility,
/// line style, and depth settings (isolated from DefaultGizmoConfigGroup).
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
struct RocketDebugGizmos {}

/// Per-category visual scales separate physical magnitude from visual length.
#[derive(Resource, Debug, Clone)]
pub struct RocketDebugConfig {
    pub enabled: bool,
    pub gravity_scale: f32,
    pub velocity_scale: f32,
    pub thrust_scale: f32,
    pub aero_scale: f32,
    pub frame_scale: f32,
    pub max_visual_length: f32,
    pub show_gravity: bool,
    pub show_velocity: bool,
    pub show_thrust: bool,
    pub show_aero: bool,
    pub show_frames: bool,
    pub show_trajectory: bool,
    pub show_com_cop: bool,
    pub show_collision: bool,
    pub show_lod: bool,
}

impl Default for RocketDebugConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gravity_scale: 2.0,
            // Velocity magnitudes are 0-8000 m/s; ~0.05 gives a useful arrow.
            velocity_scale: 0.05,
            // Thrust/aero reach millions of N; compress to a visible range.
            thrust_scale: 1e-4,
            aero_scale: 1e-3,
            frame_scale: 20.0,
            max_visual_length: 500.0,
            show_gravity: true,
            show_velocity: true,
            show_thrust: true,
            show_aero: true,
            show_frames: true,
            show_trajectory: true,
            show_com_cop: true,
            show_collision: true,
            show_lod: false,
        }
    }
}

/// Identity of the orbital state that produced the cached trajectory.
/// Rocket debug visualization plugin. Composed only by RocketModePlugin;
/// GizmoPlugin itself is registered once at the app level.
pub struct RocketDebugPlugin;

impl Plugin for RocketDebugPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RocketDebugConfig>()
            .init_resource::<TrajectoryCache>()
            .init_gizmo_group::<RocketDebugGizmos>()
            .add_systems(Update, handle_debug_input)
            .add_systems(
                Update,
                (
                    update_trajectory_cache,
                    draw_gravity_vectors,
                    draw_velocity_vectors,
                    draw_thrust_vectors,
                    draw_aero_forces,
                    draw_coordinate_frames,
                    draw_com_cop,
                    draw_orbital_trajectory,
                    draw_terrain_collision,
                    draw_terrain_lod,
                )
                    .chain()
                    .run_if(debug_enabled),
            );
    }
}

fn debug_enabled(config: Res<RocketDebugConfig>) -> bool {
    config.enabled
}

fn terrain_lod_enabled(config: &RocketDebugConfig) -> bool {
    config.enabled && config.show_lod
}

/// Debug category toggles. F1 = master; F2..F10 = categories.
fn handle_debug_input(keyboard: Res<ButtonInput<KeyCode>>, mut config: ResMut<RocketDebugConfig>) {
    type DebugToggle = (KeyCode, fn(&mut RocketDebugConfig));
    let toggles: [DebugToggle; 10] = [
        (KeyCode::F1, |c: &mut RocketDebugConfig| {
            c.enabled = !c.enabled
        }),
        (KeyCode::F2, |c: &mut RocketDebugConfig| {
            c.show_gravity = !c.show_gravity
        }),
        (KeyCode::F3, |c: &mut RocketDebugConfig| {
            c.show_velocity = !c.show_velocity
        }),
        (KeyCode::F4, |c: &mut RocketDebugConfig| {
            c.show_thrust = !c.show_thrust
        }),
        (KeyCode::F5, |c: &mut RocketDebugConfig| {
            c.show_aero = !c.show_aero
        }),
        (KeyCode::F6, |c: &mut RocketDebugConfig| {
            c.show_frames = !c.show_frames
        }),
        (KeyCode::F7, |c: &mut RocketDebugConfig| {
            c.show_trajectory = !c.show_trajectory
        }),
        (KeyCode::F8, |c: &mut RocketDebugConfig| {
            c.show_com_cop = !c.show_com_cop
        }),
        (KeyCode::F9, |c: &mut RocketDebugConfig| {
            c.show_collision = !c.show_collision
        }),
        (KeyCode::F10, |c: &mut RocketDebugConfig| {
            c.show_lod = !c.show_lod
        }),
    ];

    if keyboard.just_pressed(KeyCode::F1) {
        bevy::log::info!(
            "Debug visualization: {}",
            if config.enabled { "OFF" } else { "ON" }
        );
    }

    for (key, toggle) in toggles {
        if keyboard.just_pressed(key) {
            toggle(&mut config);
        }
    }
}

/// Gravity vector from the single authoritative `GravityAcceleration`
/// component (computed by `update_rocket_gravity`). No physics re-computed.
fn draw_gravity_vectors(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &GravityAcceleration,
    )>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_gravity {
        return;
    }

    for (binding, rocket, gravity) in rocket_query.iter() {
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );

        // Direction toward the body; length driven by configured scale only
        // (acceleration magnitude ~9.81 would otherwise vanish or dominate).
        let accel_dir = gravity.value.normalize_or_zero();
        if accel_dir.length_squared() < 1e-9 {
            continue;
        }

        draw_vector(
            &mut gizmos,
            origin,
            accel_dir.as_vec3(),
            Color::srgb(1.0, 0.3, 0.3),
            config.gravity_scale,
            config.max_visual_length,
        );
    }
}

/// Velocity vector from authoritative dynamics state.
fn draw_velocity_vectors(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_velocity {
        return;
    }

    for (binding, rocket) in rocket_query.iter() {
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );

        draw_vector(
            &mut gizmos,
            origin,
            rocket.dynamics.velocity_mps.as_vec3(),
            Color::srgb(0.3, 1.0, 0.3),
            config.velocity_scale,
            config.max_visual_length,
        );
    }
}

/// Thrust vector from the same `stage_thrust_body` call the physics uses.
fn draw_thrust_vectors(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketPropulsion,
        &RocketFlightConditions,
    )>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_thrust {
        return;
    }

    for (binding, rocket, propulsion, atmosphere) in rocket_query.iter() {
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };
        let Some((active_core_stage, throttle)) = propulsion.running_core_stage() else {
            continue;
        };

        let (thrust_body, _) = crate::domain::services::rocket_propulsion::stage_thrust_body(
            &active_core_stage.stage().engines,
            throttle,
            atmosphere.ambient_pressure_pa,
        );
        let thrust_inertial = rocket.dynamics.orientation * thrust_body;

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );

        draw_vector(
            &mut gizmos,
            origin,
            thrust_inertial.as_vec3(),
            Color::srgb(1.0, 0.8, 0.2),
            config.thrust_scale,
            config.max_visual_length,
        );
    }
}

/// Aerodynamic force vector from the authoritative `AerodynamicForces`.
fn draw_aero_forces(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &AerodynamicForces,
    )>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_aero {
        return;
    }

    for (binding, rocket, aero) in rocket_query.iter() {
        if aero.force_body.length_squared() < 1e-9 {
            continue;
        }
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };

        let aero_inertial = rocket.dynamics.orientation * aero.force_body;
        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );

        draw_vector(
            &mut gizmos,
            origin,
            aero_inertial.as_vec3(),
            Color::srgb(0.2, 0.8, 1.0),
            config.aero_scale,
            config.max_visual_length,
        );
    }
}

/// Coordinate frames at the vehicle: body (RGB), LVLH (offset +X), ENU
/// (offset -X). Derived read-only from authoritative orientation/position.
fn draw_coordinate_frames(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(&RocketPlanetBinding, &RocketPhysicsState)>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_frames {
        return;
    }

    const BODY_X: Color = Color::srgb(1.0, 0.0, 0.0);
    const BODY_Y: Color = Color::srgb(0.0, 1.0, 0.0);
    const BODY_Z: Color = Color::srgb(0.0, 0.0, 1.0);

    for (binding, rocket) in rocket_query.iter() {
        let Some((planet, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };
        let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );
        let s = config.frame_scale;

        // Body frame.
        let axes = [
            (rocket.dynamics.orientation * DVec3::X, BODY_X),
            (rocket.dynamics.orientation * DVec3::Y, BODY_Y),
            (rocket.dynamics.orientation * DVec3::Z, BODY_Z),
        ];
        for (axis, color) in axes {
            gizmos.ray(origin, offset_to_units(&physical_scale, axis) * s, color);
        }

        // LVLH: up = radial, forward = velocity projected on local horizontal.
        let position = rocket.dynamics.position_m;
        let up = position / position.length().max(1.0);
        let velocity = rocket.dynamics.velocity_mps;
        let east = up.cross(DVec3::Z).normalize_or_zero();
        if east.length_squared() < 1e-9 {
            continue;
        }
        let north = up.cross(east).normalize_or_zero();
        let forward = (velocity - up * velocity.dot(up)).normalize_or_zero();

        let lvlh_offset = origin + offset_to_units(&physical_scale, up) * s * 3.0;
        gizmos.ray(
            lvlh_offset,
            offset_to_units(&physical_scale, forward) * s,
            Color::srgb(0.0, 1.0, 1.0),
        );
        gizmos.ray(
            lvlh_offset,
            offset_to_units(&physical_scale, up) * s,
            Color::srgb(1.0, 1.0, 0.0),
        );
        gizmos.ray(
            lvlh_offset,
            offset_to_units(&physical_scale, north) * s,
            Color::srgb(1.0, 0.0, 1.0),
        );

        // ENU at the sub-vehicle surface point.
        let surface_radius_units = physical_scale.solar_meters_to_units(radius_m.min(1e7)) as f32;
        let enu_origin = origin - up.as_vec3() * surface_radius_units;
        gizmos.ray(
            enu_origin,
            offset_to_units(&physical_scale, east) * s,
            Color::srgb(0.5, 0.5, 1.0),
        );
        gizmos.ray(
            enu_origin,
            offset_to_units(&physical_scale, north) * s,
            Color::srgb(0.5, 1.0, 0.5),
        );
        gizmos.ray(
            enu_origin,
            offset_to_units(&physical_scale, up) * s,
            Color::srgb(1.0, 0.5, 0.5),
        );
    }
}

/// COM/COP markers. COM comes from dynamics state; COP from the aero system.
fn draw_com_cop(
    config: Res<RocketDebugConfig>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &AerodynamicForces,
    )>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !config.show_com_cop {
        return;
    }

    fn cross(gizmos: &mut Gizmos<RocketDebugGizmos>, at: Vec3, half: f32, color: Color) {
        gizmos.line(at - Vec3::X * half, at + Vec3::X * half, color);
        gizmos.line(at - Vec3::Y * half, at + Vec3::Y * half, color);
        gizmos.line(at - Vec3::Z * half, at + Vec3::Z * half, color);
    }

    for (binding, rocket, aero) in rocket_query.iter() {
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );
        let orientation = rocket.dynamics.orientation;

        let com_world = origin
            + offset_to_units(
                &physical_scale,
                orientation * rocket.dynamics.center_of_mass_m,
            );
        cross(&mut gizmos, com_world, 1.0, Color::srgb(1.0, 1.0, 0.0));

        if aero.force_body.length_squared() > 1e-9 {
            let cop_world = origin
                + offset_to_units(&physical_scale, orientation * aero.center_of_pressure_body);
            cross(&mut gizmos, cop_world, 1.0, Color::srgb(0.0, 1.0, 1.0));
            gizmos.line(com_world, cop_world, Color::srgb(1.0, 0.5, 0.0));
        }
    }
}

/// Radar altitude line and ground-contact indicator from the authoritative
/// `TerrainCollisionState` (no terrain re-sampling).
fn draw_terrain_collision(
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &TerrainCollisionState,
    )>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    for (binding, rocket, collision) in rocket_query.iter() {
        let Some((_, planet_transform)) = find_bound_planet(&planet_query, binding) else {
            continue;
        };

        let origin = rocket_render_origin(
            planet_transform.translation.as_dvec3(),
            &physical_scale,
            rocket.dynamics.position_m,
        );
        let up = rocket.dynamics.position_m.normalize_or_zero();
        if up.length_squared() < 1e-9 {
            continue;
        }

        // Radar altitude converted with the same PhysicalScale as everything
        // else so the surface marker sits on the rendered terrain.
        let radar_units = physical_scale.solar_meters_to_units(collision.radar_altitude_m) as f32;
        let surface = origin - up.as_vec3() * radar_units;

        gizmos.line(origin, surface, Color::srgb(1.0, 1.0, 1.0));
        gizmos.ray(
            surface,
            up.as_vec3() * radar_units * 0.02,
            Color::srgb(0.5, 1.0, 0.5),
        );

        let contact_color = match collision.ground_contact {
            crate::domain::services::terrain_collision::GroundContact::Landed => {
                Color::srgb(0.0, 1.0, 0.0)
            }
            crate::domain::services::terrain_collision::GroundContact::Crash => {
                Color::srgb(1.0, 0.0, 0.0)
            }
            _ => Color::srgb(0.5, 0.5, 0.5),
        };
        gizmos.cuboid(
            Transform::from_translation(surface).with_scale(Vec3::ONE),
            contact_color,
        );
    }
}

/// LOD wireframes for patches the streaming system actually generated.
/// Centers come from generated geometry; size from `patch_world_size_m` —
/// the same inputs the renderer consumes. No second coordinate pipeline.
fn draw_terrain_lod(
    config: Res<RocketDebugConfig>,
    streaming: Res<TerrainStreamingResource>,
    render_origin: Res<RenderOrigin>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if !terrain_lod_enabled(&config) || streaming.generated.is_empty() {
        return;
    }
    let Some(planet) = planet_query.iter().next() else {
        return;
    };
    let Some(orientation) =
        ephemeris_snapshot.orientation_for_catalog_body(&planet.domain_planet.name)
    else {
        return;
    };
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
    let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;

    for (patch, cached_geometry) in streaming.generated.iter() {
        let geometry = &cached_geometry.geometry;
        let vertex_count = geometry.positions.len();
        if vertex_count == 0 {
            continue;
        }

        // Patch center from sampled generated vertices (planet-centered m).
        let mid = vertex_count / 2;
        let last = vertex_count - 1;
        let mut acc = DVec3::ZERO;
        for idx in [0, mid, last] {
            acc += DVec3::from_array(geometry.positions[idx]);
        }
        let center_planet = acc / 3.0;

        let center_world = terrain_lod_center_in_flight_frame(
            center_planet,
            body_to_inertial,
            render_origin.origin,
        )
        .as_vec3();
        let normal = center_planet.normalize_or_zero();
        if normal.length_squared() < 1e-9 {
            continue;
        }

        let size_m = patch_world_size_m(patch.level, radius_m) as f32;
        if !size_m.is_finite() || size_m < 1e-3 {
            continue;
        }

        let color = match patch.level {
            0 => Color::srgb(1.0, 0.0, 0.0),
            1 => Color::srgb(1.0, 0.5, 0.0),
            2 => Color::srgb(1.0, 1.0, 0.0),
            3 => Color::srgb(0.0, 1.0, 0.0),
            4 => Color::srgb(0.0, 1.0, 1.0),
            _ => Color::srgb(0.0, 0.5, 1.0),
        };

        // Square tangent to the surface, facing along the radial direction.
        let rotation = Quat::from_rotation_arc(Vec3::Z, normal.as_vec3());
        gizmos.rect(
            Isometry3d::new(center_world, rotation),
            Vec2::splat(size_m),
            color,
        );
    }
}

fn terrain_lod_center_in_flight_frame(
    center_body_fixed_m: DVec3,
    body_to_inertial: DQuat,
    render_origin_m: DVec3,
) -> DVec3 {
    body_to_inertial * center_body_fixed_m - render_origin_m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terrain_lod_requires_the_master_and_category_toggles() {
        let mut config = RocketDebugConfig::default();
        assert!(!terrain_lod_enabled(&config));

        config.enabled = true;
        assert!(!terrain_lod_enabled(&config));

        config.show_lod = true;
        assert!(terrain_lod_enabled(&config));
    }

    #[test]
    fn terrain_lod_center_uses_the_flight_render_origin() {
        let center = DVec3::new(10.0, 0.0, 0.0);
        let rotation = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2);
        let origin = DVec3::new(0.0, 7.0, 0.0);

        assert!(terrain_lod_center_in_flight_frame(center, rotation, origin)
            .abs_diff_eq(DVec3::new(0.0, 3.0, 0.0), 1.0e-12));
    }
}
