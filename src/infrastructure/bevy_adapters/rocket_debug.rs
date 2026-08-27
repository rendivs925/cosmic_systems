// Rocket debug visualization plugin (observer of the simulation, not a
// second simulation). Reads authoritative state and draws it with gizmos.
//
// Origins reuse the same planet-translation + PhysicalScale conversion as
// `sync_render_transform` so debug geometry lands where the vehicle renders.
// Vector directions/magnitudes come from the authoritative components
// (`GravityAcceleration`, dynamics state) — nothing here re-simulates physics.

use crate::components::rocket::*;
use crate::domain::services::cube_sphere::patch_world_size_m;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::terrain_streaming::TerrainStreamingResource;
use bevy::math::{DVec3, Isometry3d};
use bevy::prelude::*;

/// Gizmo configuration group so rocket debugging has independent visibility,
/// line style, and depth settings (isolated from DefaultGizmoConfigGroup).
#[derive(Default, Reflect, GizmoConfigGroup)]
#[reflect(Default)]
struct RocketDebugGizmos {}

/// Samples along one full orbit for the trajectory polyline.
const TRAJECTORY_SAMPLE_COUNT: usize = 128;

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
/// Recompute only when this changes (stage switch, thrust toggle, or km-level
/// apoapsis/periapsis drift), not every frame.
#[derive(Debug, Clone, PartialEq)]
struct TrajectoryStateKey {
    active_stage: usize,
    thrusting: bool,
    apoapsis_km: i64,
    periapsis_km: i64,
}

impl TrajectoryStateKey {
    fn new(propulsion: &RocketPropulsion, orbital: &OrbitalElements) -> Self {
        Self {
            active_stage: propulsion.active_stage,
            thrusting: propulsion.throttle.clamp(0.0, 1.0) > 0.0,
            apoapsis_km: (orbital.apoapsis_m / 1000.0) as i64,
            periapsis_km: (orbital.periapsis_m / 1000.0) as i64,
        }
    }
}

/// Cached trajectory points in planet-centered meters, plus the state key that
/// produced them. Drawn offset by the live planet transform each frame.
#[derive(Resource, Default)]
struct TrajectoryCache {
    key: Option<TrajectoryStateKey>,
    /// Planet-centered sample positions (meters) forming the orbit polyline.
    points_planet_frame: Vec<DVec3>,
}

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

/// Shared vector-with-arrowhead primitive. All force/velocity vectors route
/// through here so there is exactly one arrow implementation.
fn draw_vector(
    gizmos: &mut Gizmos<RocketDebugGizmos>,
    origin: Vec3,
    vector: Vec3,
    color: Color,
    visual_scale: f32,
    max_visual_length: f32,
) {
    let magnitude = vector.length();
    if magnitude < 1e-9 || !magnitude.is_finite() {
        return;
    }
    let dir = vector / magnitude;
    let visual_len = (visual_scale * magnitude).min(max_visual_length).max(0.5);

    // Shaft
    gizmos.ray(origin, dir * visual_len, color);

    // Arrowhead: four barbs around the tip.
    let tip = origin + dir * visual_len;
    let reference = if dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let side_a = dir.cross(reference).normalize_or_zero();
    let side_b = dir.cross(side_a).normalize_or_zero();
    let base = tip - dir * (visual_len * 0.12);
    let barb = visual_len * 0.06;

    gizmos.line(tip, base + side_a * barb, color);
    gizmos.line(tip, base - side_a * barb, color);
    gizmos.line(tip, base + side_b * barb, color);
    gizmos.line(tip, base - side_b * barb, color);
}

/// World-space render origin of the rocket, matching `sync_render_transform`:
/// planet translation + PhysicalScale-converted planet-centered meters.
fn rocket_render_origin(
    planet_translation: DVec3,
    scale: &PhysicalScale,
    position_m: DVec3,
) -> Vec3 {
    (planet_translation
        + DVec3::new(
            scale.solar_meters_to_units(position_m.x),
            scale.solar_meters_to_units(position_m.y),
            scale.solar_meters_to_units(position_m.z),
        ))
    .as_vec3()
}

/// Convert a planet-centered meter offset to render-unit offset.
fn offset_to_units(scale: &PhysicalScale, offset_m: DVec3) -> Vec3 {
    DVec3::new(
        scale.solar_meters_to_units(offset_m.x),
        scale.solar_meters_to_units(offset_m.y),
        scale.solar_meters_to_units(offset_m.z),
    )
    .as_vec3()
}

fn find_bound_planet<'a>(
    planet_query: &'a Query<(&PlanetComponent, &Transform)>,
    binding: &RocketPlanetBinding,
) -> Option<(&'a PlanetComponent, &'a Transform)> {
    planet_query
        .iter()
        .find(|(planet, _)| planet.matches_body(&binding.planet_name))
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
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        if throttle <= 0.0 || remaining <= 0.0 {
            continue;
        }

        let (thrust_body, _) = crate::domain::services::rocket_propulsion::stage_thrust_body(
            &stage.engines,
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

/// Rebuild the trajectory polyline only when orbital state meaningfully
/// changes; otherwise reuse cached samples (drawn against the live planet
/// transform). Avoids per-frame Kepler propagation during coast flight.
fn update_trajectory_cache(
    config: Res<RocketDebugConfig>,
    rocket_query: Query<(&RocketPropulsion, &OrbitalElements)>,
    mut cache: ResMut<TrajectoryCache>,
) {
    if !config.show_trajectory {
        return;
    }

    let Some((propulsion, orbital)) = rocket_query.iter().next() else {
        return;
    };

    let period = orbital.orbital_period_s;
    // Skip hyperbolic/escape or degenerate orbits.
    if !period.is_finite() || period <= 0.0 || period > 86400.0 * 365.0 {
        cache.points_planet_frame.clear();
        return;
    }

    let key = TrajectoryStateKey::new(propulsion, orbital);
    if cache.key.as_ref() == Some(&key) && !cache.points_planet_frame.is_empty() {
        return;
    }

    let steps = TRAJECTORY_SAMPLE_COUNT;
    let e = orbital.eccentricity.clamp(0.0, 0.999);
    let a = orbital.semi_major_axis_m;

    // Precompute the perifocal-to-inertial rotation terms once.
    let cos_raan = orbital.longitude_ascending_node_rad.cos();
    let sin_raan = orbital.longitude_ascending_node_rad.sin();
    let cos_inc = orbital.inclination_rad.cos();
    let sin_inc = orbital.inclination_rad.sin();
    let cos_arg = orbital.argument_of_periapsis_rad.cos();
    let sin_arg = orbital.argument_of_periapsis_rad.sin();

    cache.points_planet_frame.clear();
    cache.points_planet_frame.reserve(steps + 1);

    for i in 0..=steps {
        let mean_anomaly =
            orbital.mean_anomaly_rad + 2.0 * std::f64::consts::PI * i as f64 / steps as f64;

        // Newton iteration for Kepler's equation: M = E - e·sin(E).
        let mut eccentric = mean_anomaly;
        for _ in 0..8 {
            eccentric -=
                (eccentric - e * eccentric.sin() - mean_anomaly) / (1.0 - e * eccentric.cos());
        }

        // True anomaly and radius from the conic equation.
        let cos_e = eccentric.cos();
        let sin_e = eccentric.sin();
        let cos_nu = (cos_e - e) / (1.0 - e * cos_e);
        let sin_nu = (1.0 - e * e).sqrt() * sin_e / (1.0 - e * cos_e);
        let r = a * (1.0 - e * cos_e);

        // Perifocal coordinates.
        let u = r * cos_nu;
        let v = r * sin_nu;
        let angle = orbital.argument_of_periapsis_rad;
        let p_x = u * angle.cos() - v * angle.sin();
        let p_y = u * angle.sin() + v * angle.cos();

        let x = (cos_raan * cos_arg - sin_raan * sin_arg * cos_inc) * p_x
            + (-cos_raan * sin_arg - sin_raan * cos_arg * cos_inc) * p_y;
        let y = (sin_raan * cos_arg + cos_raan * sin_arg * cos_inc) * p_x
            + (-sin_raan * sin_arg + cos_raan * cos_arg * cos_inc) * p_y;
        let z = sin_arg * sin_inc * p_x + cos_arg * sin_inc * p_y;

        cache.points_planet_frame.push(DVec3::new(x, y, z));
    }

    cache.key = Some(key);
}

/// Cached orbital polyline + apoapsis/periapsis markers. Pure drawing; the
/// expensive propagation lives in `update_trajectory_cache`.
fn draw_orbital_trajectory(
    planet_query: Query<(&PlanetComponent, &Transform)>,
    cache: Res<TrajectoryCache>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if cache.points_planet_frame.len() < 2 {
        return;
    }

    let Some((_, planet_transform)) = planet_query.iter().next() else {
        return;
    };
    let planet_pos = planet_transform.translation.as_dvec3();

    let to_world = |p: DVec3| (planet_pos + p).as_vec3();

    // Polyline via linestrip (one primitive for the whole orbit).
    gizmos.linestrip(
        cache.points_planet_frame.iter().map(|p| to_world(*p)),
        Color::srgb(0.5, 0.5, 1.0),
    );

    // Apoapsis (green) / periapsis (red) from cached extremes.
    if let (Some(ap), Some(pe)) = (
        cache
            .points_planet_frame
            .iter()
            .max_by(|a, b| a.length().total_cmp(&b.length())),
        cache
            .points_planet_frame
            .iter()
            .min_by(|a, b| a.length().total_cmp(&b.length())),
    ) {
        gizmos.sphere(to_world(*ap), 3.0, Color::srgb(0.0, 1.0, 0.0));
        gizmos.sphere(to_world(*pe), 3.0, Color::srgb(1.0, 0.0, 0.0));
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
    streaming: Res<TerrainStreamingResource>,
    physical_scale: Res<PhysicalScale>,
    planet_query: Query<(&PlanetComponent, &Transform)>,
    mut gizmos: Gizmos<RocketDebugGizmos>,
) {
    if streaming.generated.is_empty() {
        return;
    }
    let Some((planet, planet_transform)) = planet_query.iter().next() else {
        return;
    };
    let planet_pos = planet_transform.translation.as_dvec3();
    let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;

    for (patch, geometry) in streaming.generated.iter() {
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

        let center_world = (planet_pos + center_planet).as_vec3();
        let normal = center_planet.normalize_or_zero();
        if normal.length_squared() < 1e-9 {
            continue;
        }

        let size_m = patch_world_size_m(patch.level, radius_m) as f32;
        let size_units = physical_scale.solar_meters_to_units(size_m as f64) as f32;
        if !size_units.is_finite() || size_units < 1e-3 {
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
            Vec2::splat(size_units),
            color,
        );
    }
}
