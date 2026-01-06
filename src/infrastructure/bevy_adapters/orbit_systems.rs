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
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 25; // Less frequent on web for performance
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 10; // Desktop can handle more frequent updates

    if frame_number % update_stride != 0 {
        return;
    }

    let elapsed = time.elapsed_secs();

    // Update each orbit material based on its distance rank and selection status
    for orbit_comp in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            // Calculate opacity based on distance hierarchy and selection
            let distance_factor = orbit_comp.distance_rank; // 0.0 = inner planets, 1.0 = outer planets

            // Balanced minimal visibility - perceptible but elegant guidance
            let base_opacity = if distance_factor < 0.25 {
                0.15 // Inner planets: clearly visible but subtle
            } else if distance_factor < 0.5 {
                0.10 // Middle planets: moderately visible
            } else {
                0.06 // Outer planets: minimal but present
            };

            // Subtle boost for selected planet's orbit - clearly enhanced but elegant
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let final_base_opacity = if is_selected {
                (base_opacity * 1.8_f32).min(0.25) // Enhanced visibility for context
            } else {
                base_opacity
            };

            // Ultra-elegant stellar harmonics - sophisticated orbital resonance
            let orbital_harmonic = orbit_comp.radius * 0.0001; // Unique frequency per orbit
            let stellar_resonance = 0.02 * (elapsed * 0.004 + orbital_harmonic).sin();
            let cosmic_pulse = 0.985 + 0.015 * (elapsed * 0.005).sin() + stellar_resonance;
            let alpha = final_base_opacity * cosmic_pulse;

            // Ultra-sophisticated spectral palette - refined cosmic elegance
            let (r, g, b) = if is_selected {
                (0.68, 0.74, 0.80) // Subtle sterling highlight for selection
            } else {
                (0.52, 0.58, 0.64) // Refined graphite blue-gray - pure minimalism
            };

            material.base_color = Color::srgb(r, g, b).with_alpha(alpha);

            // Subtle stellar residue - gentle cosmic energy hints
            let stellar_residue = if is_selected {
                0.015 + 0.005 * cosmic_pulse
            } else {
                0.008 + 0.002 * cosmic_pulse
            };
            material.emissive = LinearRgba::new(stellar_residue, stellar_residue, stellar_residue * 1.2, 1.0);
        }
    }
}

// System to toggle orbit visibility based on show_orbits parameter with contextual hierarchy
pub fn update_orbit_visibility(
    solar_params: Res<SolarSystemParameters>,
    selected_planet: Res<SelectedPlanet>,
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    mut orbit_query: Query<(&OrbitComponent, &mut Visibility)>,
) {
    if !solar_params.show_orbits {
        // Hide all orbits if disabled
        for (_, mut visibility) in orbit_query.iter_mut() {
            *visibility = Visibility::Hidden;
        }
        return;
    }

    let camera_pos = camera_query.single().unwrap().translation();
    let camera_distance_from_sun = camera_pos.length(); // Distance from solar system center

    // Show orbits with contextual hierarchy based on zoom level
    for (orbit_comp, mut visibility) in orbit_query.iter_mut() {
        let distance_factor = orbit_comp.distance_rank; // 0.0 = inner planets, 1.0 = outer planets
        let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);

        // Zoom-based visibility: show more orbits when zoomed out
        let zoom_factor = (camera_distance_from_sun / 10000.0).clamp(0.1, 3.0);
        let visibility_threshold = if zoom_factor > 1.5 {
            0.9 // Show almost all orbits when zoomed far out
        } else if zoom_factor > 0.8 {
            0.7 // Show more orbits at medium zoom
        } else {
            0.4 // Show fewer orbits when zoomed in close
        };

        let final_visibility = if is_selected {
            Visibility::Visible // Selected orbits always visible for context
        } else {
            // Distance and zoom-based visibility
            if distance_factor <= visibility_threshold {
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
// Ultra-refined planet surface properties - sophisticated material evolution
pub fn update_planet_reflections(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: Query<(&PlanetComponent, &GlobalTransform)>,
) {
    // Elegant update cadence - subtle material refinement
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 30; // Minimal updates for web performance
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 10; // Sophisticated pacing for desktop

    if frame_number % update_stride != 0 {
        return;
    }

    for (planet_comp, _global_transform) in query.iter() {
        if planet_comp.domain_planet.name == "Sun" {
            continue;
        }
        if let Some(material) = materials.get_mut(&planet_comp.material) {
            // Ultra-refined surface properties with elegant constraints
            material.perceptual_roughness = planet_comp.base_roughness.clamp(0.08, 0.85);
            material.reflectance = planet_comp.base_reflectance.clamp(0.015, 0.12); // Minimal but sophisticated
        }
    }
}