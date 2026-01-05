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

            // Ultra-minimal visibility hierarchy - barely perceptible guidance
            let base_opacity = if distance_factor < 0.25 {
                0.06 // Inner planets: minimal but present
            } else if distance_factor < 0.5 {
                0.04 // Middle planets: very subtle
            } else {
                0.02 // Outer planets: almost invisible
            };

            // Subtle boost for selected planet's orbit - just enough to notice
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let final_base_opacity = if is_selected {
                (base_opacity * 2.0_f32).min(0.12) // Gentle enhancement, not overwhelming
            } else {
                base_opacity
            };

            // Minimal cosmic variation - barely noticeable stellar influence
            let stellar_noise = 0.02 * (elapsed * 0.005 + orbit_comp.radius * 0.001).sin();
            let cosmic_pulse = 0.98 + 0.02 * (elapsed * 0.006).sin() + stellar_noise;
            let alpha = final_base_opacity * cosmic_pulse;

            // Ultra-minimal monochromatic palette - cosmic void colors
            let (r, g, b) = if is_selected {
                (0.65, 0.72, 0.78) // Barely perceptible highlight for selection
            } else {
                (0.55, 0.62, 0.68) // Deep space blue-gray - almost invisible
            };

            material.base_color = Color::srgb(r, g, b).with_alpha(alpha);

            // Minimal stellar residue - just a hint of cosmic energy
            let stellar_residue = if is_selected {
                0.008 + 0.003 * cosmic_pulse
            } else {
                0.004 + 0.001 * cosmic_pulse
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

    let camera_pos = camera_query.single().translation();
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