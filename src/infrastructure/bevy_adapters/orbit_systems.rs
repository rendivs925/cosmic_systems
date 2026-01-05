use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

// System to animate orbit visuals with contextual hierarchy and elegance
// Creates sophisticated visual hierarchy based on distance and selection
#[allow(dead_code)]
pub(crate) fn update_orbit_visuals(
    time: Res<Time>,
    selected_planet: Res<SelectedPlanet>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<&OrbitComponent>,
) {
    // Performance optimization: update orbit visuals periodically
    let frame_number = (time.elapsed_seconds() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 25; // Less frequent on web for performance
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 10; // Desktop can handle more frequent updates

    if frame_number % update_stride != 0 {
        return;
    }

    let elapsed = time.elapsed_seconds();

    // Update each orbit material based on its distance rank and selection status
    for orbit_comp in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            // Calculate opacity based on distance hierarchy and selection
            let distance_factor = orbit_comp.distance_rank; // 0.0 = inner planets, 1.0 = outer planets

            // Inner planets (Mercury, Venus, Earth, Mars) get higher base visibility
            // Outer planets (Jupiter, Saturn, Uranus, Neptune) get lower visibility
            let base_opacity = if distance_factor < 0.3 {
                0.12 // Inner planets: more visible
            } else if distance_factor < 0.6 {
                0.08 // Middle planets: medium visibility
            } else {
                0.05 // Outer planets: least visible
            };

            // Boost opacity for selected planet's orbit
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let final_base_opacity = if is_selected {
                (base_opacity * 2.5_f32).min(0.25) // Selected orbits: significantly more visible
            } else {
                base_opacity
            };

            // Ultra-subtle cosmic breathing effect - hints at orbital motion
            let cosmic_pulse = 0.97 + 0.03 * (elapsed * 0.008).sin();
            let alpha = final_base_opacity * cosmic_pulse;

            // Contextual color palette based on distance and selection
            let (r, g, b) = if is_selected {
                (0.7, 0.8, 0.9) // Slightly brighter for selected orbits
            } else if distance_factor < 0.4 {
                (0.65, 0.75, 0.85) // Inner planets: warmer blue
            } else {
                (0.55, 0.65, 0.75) // Outer planets: cooler blue
            };

            material.base_color = Color::srgb(r, g, b).with_alpha(alpha);

            // Barely perceptible emissive hint - stronger for selected orbits
            let stellar_glow = if is_selected {
                0.025 + 0.008 * cosmic_pulse
            } else {
                0.012 + 0.004 * cosmic_pulse
            };
            material.emissive = LinearRgba::new(stellar_glow, stellar_glow, stellar_glow * 1.3, 1.0);
        }
    }
}

// System to toggle orbit visibility based on show_orbits parameter with contextual hierarchy
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    mut orbit_query: Query<(&OrbitComponent, &mut Visibility)>,
) {
    if !solar_params.show_orbits {
        // Hide all orbits if disabled
        for (_, mut visibility) in orbit_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    // Show orbits with contextual hierarchy
    for (orbit_comp, mut visibility) in orbit_query.iter_mut() {
        // Create distance-based hierarchy: inner planets more visible than outer ones
        let distance_factor = (orbit_comp.radius / 1000.0).clamp(0.0, 1.0); // Normalize distance
        let visibility_threshold = 0.3 + (distance_factor * 0.4); // Inner planets: 0.3-0.7, outer: 0.3-0.4

        // Boost visibility for selected planet's orbit
        let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
        let final_visibility = if is_selected {
            Visibility::Visible // Selected orbits always visible
        } else {
            // Distance-based probabilistic visibility (creates natural hierarchy)
            if distance_factor > visibility_threshold {
                Visibility::Visible
            } else {
                Visibility::Hidden
            }
        };

        *visibility = final_visibility;
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