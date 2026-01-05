use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

// System to animate orbit visuals for a more dynamic presentation
// Optimized to update every 3 frames instead of every frame
#[allow(dead_code)]
pub(crate) fn update_orbit_visuals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<&OrbitComponent>,
    shared: Option<Res<crate::application::solar_system_startup::SharedOrbitMaterial>>,
) {
    // Performance optimization: update orbit visuals only every 3 frames
    // Visual pulsing is slow enough that this is imperceptible
    let frame_number = (time.elapsed_seconds() * 60.0) as u32; // Assume 60 FPS
    #[cfg(target_arch = "wasm32")]
    let update_stride = 12;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 3;

    if frame_number % update_stride != 0 {
        return;
    }

    let elapsed = time.elapsed_seconds();

    if query.is_empty() {
        return;
    }

    let Some(shared) = shared else {
        return;
    };

    if let Some(material) = materials.get_mut(&shared.handle) {
        // Very subtle, slow pulsing - almost static appearance
        let pulse = 0.8 + 0.2 * (elapsed * 0.05).sin(); // Much slower, smaller variation
        let alpha = 0.15 * pulse; // Base transparency with subtle modulation
        material.base_color = Color::srgb(0.8, 0.9, 1.0).with_alpha(alpha);
        let glow = 0.1 + 0.05 * pulse; // Much subtler glow
        material.emissive = LinearRgba::new(glow, glow, glow * 1.2, 1.0); // Slight blue bias
    }
}

// System to toggle orbit visibility based on show_orbits parameter and distance culling
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    planet_query: Query<&GlobalTransform, With<PlanetComponent>>,
    mut orbit_query: Query<(&OrbitComponent, &mut Visibility)>,
) {
    if !solar_params.show_orbits {
        // Hide all orbits if disabled
        for (_, mut visibility) in orbit_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let camera_pos = camera_query.single().translation();

    for (orbit_comp, mut visibility) in orbit_query.iter_mut() {
        // Check distance to the planet this orbit belongs to, not the orbit entity position
        if let Ok(planet_transform) = planet_query.get(orbit_comp.planet_entity) {
            let distance_to_planet = camera_pos.distance(planet_transform.translation());
            let max_orbit_distance = 80000.0; // Allow orbits to be visible for planets within this distance

            *visibility = if distance_to_planet <= max_orbit_distance {
                Visibility::Visible
            } else {
                Visibility::Hidden
            };
        } else {
            // If we can't find the planet, hide the orbit
            *visibility = Visibility::Hidden;
        }
    }
}

// System to add dynamic specular reflection response for planet materials
// Optimized to update every 5 frames (material properties don't change dynamically)
pub fn update_planet_reflections(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    // Performance optimization: skip most updates since values don't change
    let frame_number = (time.elapsed_seconds() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 20;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 5;

    if frame_number % update_stride != 0 {
        return;
    }

    for (planet_comp, _global_transform) in query.iter() {
        if planet_comp.domain_planet.name == "Sun" {
            continue;
        }
        if let Some(material) = materials.get_mut(&planet_comp.material) {
            material.perceptual_roughness = planet_comp.base_roughness;
            material.reflectance = planet_comp.base_reflectance;
        }
    }
}