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

    if !frame_number.is_multiple_of(update_stride) {
        return;
    }

    let elapsed = time.elapsed_secs();

    // Update each orbit material based on its distance rank and selection status
    for orbit_comp in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            // Calculate opacity based on distance hierarchy and selection
            let distance_factor = orbit_comp.distance_rank; // 0.0 = inner planets, 1.0 = outer planets

            // Enhanced visibility for long-distance viewing - all orbits clearly visible
            let base_opacity = if distance_factor < 0.25 {
                0.18 // Inner planets: clearly visible
            } else if distance_factor < 0.5 {
                0.15 // Middle planets: well visible
            } else {
                0.12 // Outer planets: sufficiently visible at long distance
            };

            // Boost for selected planet's orbit - clearly enhanced for navigation
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let final_base_opacity = if is_selected {
                (base_opacity * 2.0_f32).min(0.30) // Enhanced visibility for selected orbit
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
                (0.85, 0.89, 0.93) // Refined cool white highlight for selection
            } else {
                (0.82, 0.86, 0.90) // Refined cool elegant white - pure sophistication
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

// System to toggle orbit visibility based on show_orbits parameter
// Orbits are now visible at all distances for better navigation
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

    // Show all orbits regardless of distance - always visible for navigation
    for (orbit_comp, mut visibility) in orbit_query.iter_mut() {
        let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);

        // All orbits are visible, but selected ones are highlighted
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
    let update_stride = 30; // Minimal updates for web performance
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 10; // Sophisticated pacing for desktop

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
            material.reflectance = planet_comp.base_reflectance.clamp(0.015, 0.12); // Minimal but sophisticated
        }
    }
}