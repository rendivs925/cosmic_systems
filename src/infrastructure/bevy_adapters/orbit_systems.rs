use super::components::*;
use crate::application::material_factory::ORBIT_LINE_COLOR;
use crate::application::mesh_factory::{create_orbit_ribbon_mesh, ORBIT_RIBBON_NEAR_WIDTH_UNITS};
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

const DISTANT_ORBIT_LINE_PIXELS: f32 = 2.5;
const ORBIT_OVERVIEW_DISTANCE_AU: f32 = 10.0;
const ORBIT_RIBBON_REBUILD_RATIO: f32 = 0.05;

// System to update stable orbit opacity from the camera's distance to the path.
pub(crate) fn update_orbit_visuals(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<(&OrbitComponent, &GlobalTransform)>,
) {
    let Ok(camera) = camera_query.single() else {
        return;
    };
    let camera_pos = camera.translation();

    for (orbit_comp, orbit_transform) in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let linear_color: LinearRgba = ORBIT_LINE_COLOR.into();

            let distance_to_path = camera_distance_to_orbit_path(
                camera_pos,
                orbit_transform.translation(),
                orbit_comp.radius,
            );
            let overview_progress = (distance_to_path
                / (solar_params.scale_factor * ORBIT_OVERVIEW_DISTANCE_AU))
                .clamp(0.0, 1.0);
            let base_opacity = 0.025 + 0.05 * overview_progress;
            let final_base_opacity = if is_selected {
                (base_opacity * 2.5).min(0.18)
            } else {
                base_opacity
            };

            material.base_color = ORBIT_LINE_COLOR.with_alpha(final_base_opacity);

            let emissive_intensity = if is_selected { 0.06 } else { 0.03 };
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

// Keep every orbit at the same narrow width nearby, then make it legible at a
// solar-system overview. The camera-to-path distance avoids making a nearby
// planetary orbit wider than a nearby moon orbit merely because its center is far away.
pub fn update_orbit_thickness(
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<CameraController>>,
    solar_params: Res<SolarSystemParameters>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut orbit_query: Query<(&mut OrbitComponent, &mut Mesh3d, &GlobalTransform)>,
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
    let camera_pos = camera_transform.translation();

    for (mut orbit_comp, mut mesh3d, orbit_transform) in orbit_query.iter_mut() {
        let orbit_center = orbit_transform.translation();
        let distance_to_path =
            camera_distance_to_orbit_path(camera_pos, orbit_center, orbit_comp.radius);
        let new_thickness = orbit_ribbon_thickness_units(
            distance_to_path,
            perspective.fov,
            viewport_height_px,
            solar_params.scale_factor,
        );

        if (new_thickness - orbit_comp.thickness).abs()
            > orbit_comp.thickness.max(0.0001) * ORBIT_RIBBON_REBUILD_RATIO
        {
            orbit_comp.thickness = new_thickness;
            let new_mesh = create_orbit_ribbon_mesh(
                &mut meshes,
                &orbit_comp.orbit_shape,
                ORBIT_LINE_COLOR,
                new_thickness,
                orbit_comp.segments,
            );
            mesh3d.0 = new_mesh;
        }
    }
}

fn camera_distance_to_orbit_path(camera_position: Vec3, orbit_center: Vec3, radius: f32) -> f32 {
    (camera_position.distance(orbit_center) - radius.max(0.0)).abs()
}

fn orbit_ribbon_thickness_units(
    distance_to_path_units: f32,
    vertical_fov_rad: f32,
    viewport_height_px: f32,
    scale_factor: f32,
) -> f32 {
    let units_per_pixel = 2.0 * distance_to_path_units.max(0.001) * (vertical_fov_rad * 0.5).tan()
        / viewport_height_px.max(1.0);
    let overview_progress = (distance_to_path_units
        / (scale_factor * ORBIT_OVERVIEW_DISTANCE_AU).max(0.001))
    .clamp(0.0, 1.0);
    let overview_width = units_per_pixel * DISTANT_ORBIT_LINE_PIXELS;

    ORBIT_RIBBON_NEAR_WIDTH_UNITS
        + (overview_width - ORBIT_RIBBON_NEAR_WIDTH_UNITS).max(0.0) * overview_progress
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_ribbon_grows_for_a_solar_system_overview() {
        let near =
            orbit_ribbon_thickness_units(0.0, std::f32::consts::FRAC_PI_3, 1_080.0, 75_000.0);
        let overview = orbit_ribbon_thickness_units(
            1_425_000.0,
            std::f32::consts::FRAC_PI_3,
            1_080.0,
            75_000.0,
        );

        assert!(overview > near);
    }

    #[test]
    fn nearby_orbits_use_the_shared_near_width() {
        assert_eq!(
            orbit_ribbon_thickness_units(0.0, std::f32::consts::FRAC_PI_3, 1_080.0, 75_000.0),
            ORBIT_RIBBON_NEAR_WIDTH_UNITS
        );
    }

    #[test]
    fn orbit_path_distance_is_zero_for_a_camera_on_a_circular_path() {
        assert_eq!(
            camera_distance_to_orbit_path(Vec3::new(100.0, 0.0, 0.0), Vec3::ZERO, 100.0),
            0.0
        );
    }
}
