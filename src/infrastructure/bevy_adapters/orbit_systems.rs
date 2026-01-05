use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

// System to animate orbit visuals for a more dynamic presentation
// Optimized to update every few frames for performance
#[allow(dead_code)]
pub(crate) fn update_orbit_visuals(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<&OrbitComponent>,
    shared: Option<Res<crate::application::solar_system_startup::SharedOrbitMaterial>>,
) {
    // Performance optimization: update orbit visuals periodically
    let frame_number = (time.elapsed_seconds() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 15; // Less frequent on web
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 5; // Desktop can handle more frequent updates

    if frame_number % update_stride != 0 {
        return;
    }

    let elapsed = time.elapsed_seconds();

    if query.is_empty() {
        return;
    }

    if let Some(shared) = shared {
        if let Some(material) = materials.get_mut(&shared.handle) {
            // Ultra-subtle cosmic breathing effect - barely noticeable
            let cosmic_pulse = 0.95 + 0.05 * (elapsed * 0.015).sin(); // Extremely slow, minimal variation
            let alpha = 0.08 * cosmic_pulse; // Ultra-subtle transparency that hints at orbital motion
            material.base_color = Color::srgb(0.65, 0.75, 0.85).with_alpha(alpha); // Cool cosmic blue
            let stellar_glow = 0.02 + 0.01 * cosmic_pulse; // Barely perceptible stellar influence
            material.emissive = LinearRgba::new(stellar_glow, stellar_glow, stellar_glow * 1.3, 1.0); // Subtle blue stellar accent
        }
    }
}

// System to toggle orbit visibility based on show_orbits parameter
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    mut orbit_query: Query<&mut Visibility, With<OrbitComponent>>,
) {
    let visibility = if solar_params.show_orbits {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for mut orbit_visibility in orbit_query.iter_mut() {
        *orbit_visibility = visibility;
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