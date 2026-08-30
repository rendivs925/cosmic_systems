use super::components::*;
use crate::application::material_factory::ORBIT_LINE_COLOR;
use crate::application::mesh_factory::{
    create_orbit_ribbon_mesh, create_sampled_orbit_ribbon_mesh, ORBIT_RIBBON_NEAR_WIDTH_UNITS,
};
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

const ORBIT_LINE_PIXELS: f32 = 1.5;
const ORBIT_BODY_WIDTH_FRACTION: f32 = 0.008;
const ORBIT_MAX_RELATIVE_WIDTH: f32 = 0.003;
const ORBIT_MAX_VIEWPORT_WIDTH_MULTIPLIER: f32 = 4.0;
const ORBIT_OVERVIEW_DISTANCE_AU: f32 = 10.0;
const ORBIT_RIBBON_REBUILD_RATIO: f32 = 0.05;
const NEAR_ORBIT_OPACITY: f32 = 0.16;
const DISTANT_ORBIT_OPACITY: f32 = 0.28;

// System to update stable orbit opacity from the camera's distance to the path.
pub(crate) fn update_orbit_visuals(
    camera_query: Query<&Transform, With<CameraController>>,
    solar_params: Res<SolarSystemParameters>,
    origin: Res<SolarMapRenderOrigin>,
    selected_planet: Res<SelectedPlanet>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    planet_positions: Query<&SolarMapPosition>,
    orbit_query: Query<(&OrbitComponent, &Transform, Has<MoonOrbit>)>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let camera_pos = origin.position_units + DVec3::from(camera.translation);

    for (orbit_comp, orbit_transform, is_moon) in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let linear_color: LinearRgba = ORBIT_LINE_COLOR.into();

            let orbit_center =
                orbit_center_units(is_moon, orbit_comp.planet_entity, &planet_positions);
            let distance_to_path = orbit_comp.sampled_path_units.as_deref().map_or_else(
                || {
                    camera_distance_to_orbit_path(
                        camera_pos,
                        &orbit_comp.orbit_shape,
                        orbit_center,
                        orbit_transform.rotation,
                        orbit_comp.segments,
                    )
                },
                |path| {
                    camera_distance_to_sampled_path(
                        camera_pos,
                        path,
                        orbit_comp.sampled_path_closed,
                    )
                },
            );
            let overview_progress = (distance_to_path
                / (solar_params.scale_factor as f64 * ORBIT_OVERVIEW_DISTANCE_AU as f64))
                .clamp(0.0, 1.0);
            let base_opacity = NEAR_ORBIT_OPACITY
                + (DISTANT_ORBIT_OPACITY - NEAR_ORBIT_OPACITY) * overview_progress as f32;
            let final_base_opacity = if is_selected {
                (base_opacity * 2.0).min(0.55)
            } else {
                base_opacity
            };

            material.base_color = ORBIT_LINE_COLOR.with_alpha(final_base_opacity);

            let emissive_intensity = if is_selected { 0.12 } else { 0.06 };
            material.emissive = LinearRgba::new(
                linear_color.red * emissive_intensity,
                linear_color.green * emissive_intensity,
                linear_color.blue * emissive_intensity,
                1.0,
            );
        }
    }
}

// System to toggle orbit visibility based on show_orbits parameter
// Orbits are now visible at all distances for better navigation
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    mut orbit_query: Query<(&OrbitComponent, &mut Visibility)>,
) {
    if !solar_params.show_orbits {
        // Hide all orbits if disabled
        for (_, mut visibility) in orbit_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Show all orbits regardless of distance - always visible for navigation
    for (_, mut visibility) in orbit_query.iter_mut() {
        *visibility = Visibility::Visible;
    }
}

// System to add dynamic specular reflection response for planet materials
// Optimized to update every 5 frames (material properties don't change dynamically)
// Ultra-refined planet surface properties - sophisticated material evolution
pub fn update_planet_reflections(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    // Elegant update cadence - subtle material refinement
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 6;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 3;

    if !frame_number.is_multiple_of(update_stride) {
        return;
    }

    for (planet_comp, _global_transform) in query.iter() {
        if planet_comp.domain_planet.name == "Sun" {
            continue;
        }
        if let Some(material) = materials.get_mut(&planet_comp.material) {
            // Ultra-refined surface properties with elegant constraints
            material.perceptual_roughness = planet_comp.base_roughness.clamp(0.08, 0.85);
            material.reflectance = planet_comp.base_reflectance.clamp(0.015, 0.12);
            // Minimal but sophisticated
        }
    }
}

// Keep each orbit proportional to its relevant body: a moon path uses its
// parent radius while a heliocentric path uses the orbiting body's radius. A
// viewport-derived floor maintains legibility at overview distances without
// allowing a ribbon to become a visible fraction of its orbit.
pub fn update_orbit_thickness(
    camera_query: Query<(&Camera, &Transform, &Projection), With<CameraController>>,
    solar_params: Res<SolarSystemParameters>,
    origin: Res<SolarMapRenderOrigin>,
    mut meshes: ResMut<Assets<Mesh>>,
    planet_positions: Query<&SolarMapPosition>,
    planet_query: Query<&PlanetComponent>,
    mut orbit_query: Query<(&mut OrbitComponent, &mut Mesh3d, &Transform, Has<MoonOrbit>)>,
) {
    let Ok((camera, camera_transform, projection)) = camera_query.single() else {
        return;
    };
    let Some(viewport_height_px) = camera
        .logical_viewport_size()
        .filter(|size| size.y > 0.0)
        .map(|size| size.y)
    else {
        return;
    };
    let Projection::Perspective(perspective) = projection else {
        return;
    };
    let camera_pos = origin.position_units + DVec3::from(camera_transform.translation);

    for (mut orbit_comp, mut mesh3d, orbit_transform, is_moon) in orbit_query.iter_mut() {
        let Ok(reference_body) = planet_query.get(orbit_comp.planet_entity) else {
            continue;
        };
        let orbit_center = orbit_center_units(is_moon, orbit_comp.planet_entity, &planet_positions);
        let distance_to_path = orbit_comp.sampled_path_units.as_deref().map_or_else(
            || {
                camera_distance_to_orbit_path(
                    camera_pos,
                    &orbit_comp.orbit_shape,
                    orbit_center,
                    orbit_transform.rotation,
                    orbit_comp.segments,
                )
            },
            |path| {
                camera_distance_to_sampled_path(camera_pos, path, orbit_comp.sampled_path_closed)
            },
        );
        let new_thickness = orbit_ribbon_thickness_units(
            distance_to_path,
            perspective.fov,
            viewport_height_px,
            visual_radius_units(&reference_body.domain_planet, &solar_params),
            orbit_comp.radius,
        );
        if (new_thickness - orbit_comp.thickness).abs()
            > orbit_comp.thickness.max(ORBIT_RIBBON_NEAR_WIDTH_UNITS) * ORBIT_RIBBON_REBUILD_RATIO
        {
            let previous_mesh = mesh3d.0.clone();
            orbit_comp.thickness = new_thickness;
            let new_mesh = if let Some(path) = orbit_comp.sampled_path_units.as_deref() {
                create_sampled_orbit_ribbon_mesh(
                    &mut meshes,
                    path,
                    orbit_comp.render_anchor_units,
                    ORBIT_LINE_COLOR,
                    new_thickness,
                    orbit_comp.sampled_path_closed,
                )
            } else {
                create_orbit_ribbon_mesh(
                    &mut meshes,
                    &orbit_comp.orbit_shape,
                    ORBIT_LINE_COLOR,
                    new_thickness,
                    orbit_comp.segments,
                )
            };
            mesh3d.0 = new_mesh;
            meshes.remove(previous_mesh.id());
        }
    }
}

fn camera_distance_to_orbit_path(
    camera_position: DVec3,
    orbit_shape: &physics::OrbitShape,
    orbit_center: DVec3,
    orbit_rotation: Quat,
    segments: usize,
) -> f64 {
    let segments = segments.max(crate::application::mesh_factory::ORBIT_RIBBON_SEGMENTS);
    let mut closest_distance_squared = f64::INFINITY;
    let mut previous = orbit_point_world(
        orbit_shape,
        std::f64::consts::TAU * (segments - 1) as f64 / segments as f64,
        orbit_center,
        orbit_rotation,
    );

    for index in 0..segments {
        let current = orbit_point_world(
            orbit_shape,
            std::f64::consts::TAU * index as f64 / segments as f64,
            orbit_center,
            orbit_rotation,
        );
        closest_distance_squared = closest_distance_squared.min(point_segment_distance_squared(
            camera_position,
            previous,
            current,
        ));
        previous = current;
    }

    closest_distance_squared.sqrt()
}

fn camera_distance_to_sampled_path(camera_position: DVec3, path: &[DVec3], closed: bool) -> f64 {
    if path.len() < 2 {
        return f64::INFINITY;
    }
    let mut closest_distance_squared = f64::INFINITY;
    let mut previous = path[0];
    for point in path.iter().skip(1) {
        let current = *point;
        closest_distance_squared = closest_distance_squared.min(point_segment_distance_squared(
            camera_position,
            previous,
            current,
        ));
        previous = current;
    }
    if closed {
        closest_distance_squared = closest_distance_squared.min(point_segment_distance_squared(
            camera_position,
            previous,
            path[0],
        ));
    }
    closest_distance_squared.sqrt()
}

fn orbit_center_units(
    is_moon: bool,
    parent_entity: Entity,
    planet_positions: &Query<&SolarMapPosition>,
) -> DVec3 {
    if is_moon {
        planet_positions
            .get(parent_entity)
            .map_or(DVec3::ZERO, |position| position.0)
    } else {
        DVec3::ZERO
    }
}

fn orbit_point_world(
    orbit_shape: &physics::OrbitShape,
    eccentric_anomaly: f64,
    orbit_center: DVec3,
    orbit_rotation: Quat,
) -> DVec3 {
    let rotation = DQuat::from_xyzw(
        orbit_rotation.x as f64,
        orbit_rotation.y as f64,
        orbit_rotation.z as f64,
        orbit_rotation.w as f64,
    );
    orbit_center + rotation * physics::orbit_point_f64(orbit_shape, eccentric_anomaly)
}

fn point_segment_distance_squared(point: DVec3, start: DVec3, end: DVec3) -> f64 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared == 0.0 {
        return point.distance_squared(start);
    }
    let t = ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    point.distance_squared(start + segment * t)
}

fn orbit_ribbon_thickness_units(
    distance_to_path_units: f64,
    vertical_fov_rad: f32,
    viewport_height_px: f32,
    reference_body_radius_units: f32,
    orbit_radius_units: f32,
) -> f32 {
    let units_per_pixel =
        2.0 * distance_to_path_units.max(0.001) * (vertical_fov_rad as f64 * 0.5).tan()
            / viewport_height_px.max(1.0) as f64;
    let body_relative_width =
        reference_body_radius_units.max(f32::EPSILON) * ORBIT_BODY_WIDTH_FRACTION;
    let viewport_width = units_per_pixel as f32 * ORBIT_LINE_PIXELS;
    let geometry_limit =
        (orbit_radius_units.abs() * ORBIT_MAX_RELATIVE_WIDTH).max(body_relative_width);
    // A world-space ribbon cannot safely grow without bound: a near, edge-on
    // segment otherwise expands across the viewport as a distorted polygon.
    let maximum_width =
        geometry_limit.min(body_relative_width * ORBIT_MAX_VIEWPORT_WIDTH_MULTIPLIER);

    body_relative_width.max(viewport_width).min(maximum_width)
}

fn visual_radius_units(
    planet: &crate::domain::entities::planet::Planet,
    solar_params: &SolarSystemParameters,
) -> f32 {
    if planet.name == "Sun" {
        physics::calculate_sun_visual_radius(solar_params)
    } else {
        physics::calculate_visual_radius(planet, solar_params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_ribbon_scales_with_its_reference_body() {
        let small_body =
            orbit_ribbon_thickness_units(0.0, std::f32::consts::FRAC_PI_3, 1_080.0, 1.0, 1_000.0);
        let large_body =
            orbit_ribbon_thickness_units(0.0, std::f32::consts::FRAC_PI_3, 1_080.0, 10.0, 1_000.0);

        assert_eq!(large_body, small_body * 10.0);
    }

    #[test]
    fn orbit_ribbon_stays_legible_at_an_overview_distance() {
        let near =
            orbit_ribbon_thickness_units(0.0, std::f32::consts::FRAC_PI_3, 1_080.0, 2.0, 10_000.0);
        let overview = orbit_ribbon_thickness_units(
            20_000.0,
            std::f32::consts::FRAC_PI_3,
            1_080.0,
            2.0,
            10_000.0,
        );

        assert!(overview > near);
        assert!(overview <= 10_000.0 * ORBIT_MAX_RELATIVE_WIDTH);
    }

    #[test]
    fn orbit_ribbon_width_is_bounded_by_its_reference_body() {
        let body_radius = 10.0;
        let width = orbit_ribbon_thickness_units(
            10_000_000.0,
            std::f32::consts::FRAC_PI_3,
            1_080.0,
            body_radius,
            10_000_000.0,
        );

        assert!(
            width <= body_radius * ORBIT_BODY_WIDTH_FRACTION * ORBIT_MAX_VIEWPORT_WIDTH_MULTIPLIER
        );
    }

    #[test]
    fn orbit_path_distance_is_zero_for_a_camera_on_a_circular_path() {
        let orbit = physics::OrbitShape {
            semi_major_axis_units: 100.0,
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        };
        assert!(
            camera_distance_to_orbit_path(
                DVec3::new(100.0, 0.0, 0.0),
                &orbit,
                DVec3::ZERO,
                Quat::IDENTITY,
                1024,
            ) < 1e-9
        );
    }

    #[test]
    fn orbit_path_distance_handles_inclined_eccentric_orbits() {
        let orbit = physics::OrbitShape {
            semi_major_axis_units: 100.0,
            eccentricity: 0.4,
            inclination_rad: 0.5,
            long_asc_node_rad: 0.7,
            arg_periapsis_rad: 0.2,
        };
        let sample_index = 37;
        let rotation = Quat::from_rotation_z(0.3);
        let camera_position = orbit_point_world(
            &orbit,
            sample_index as f64 / 1024.0 * std::f64::consts::TAU,
            DVec3::new(2_000_000.0, -500_000.0, 100_000.0),
            rotation,
        );

        assert!(
            camera_distance_to_orbit_path(
                camera_position,
                &orbit,
                DVec3::new(2_000_000.0, -500_000.0, 100_000.0),
                rotation,
                1024,
            ) < 1e-9
        );
    }

    #[test]
    fn neptune_midpoint_does_not_inflate_orbit_width() {
        let orbit = physics::OrbitShape {
            semi_major_axis_units: 30.06 * 75_000.0,
            eccentricity: 0.0,
            inclination_rad: 0.0,
            long_asc_node_rad: 0.0,
            arg_periapsis_rad: 0.0,
        };
        // This point falls midway between the previous 128-point distance samples.
        let camera_position = physics::orbit_point_f64(&orbit, std::f64::consts::TAU * 0.5 / 128.0);
        let distance = camera_distance_to_orbit_path(
            camera_position,
            &orbit,
            DVec3::ZERO,
            Quat::IDENTITY,
            1024,
        );

        assert!(distance < 3.0, "distance was {distance}");
        assert_eq!(
            orbit_ribbon_thickness_units(
                distance,
                std::f32::consts::FRAC_PI_3,
                720.0,
                2.0,
                orbit.semi_major_axis_units,
            ),
            2.0 * ORBIT_BODY_WIDTH_FRACTION,
        );
    }
}
