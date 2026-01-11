use super::components::*;
use crate::application::mesh_factory::{create_orbital_plane_mesh, create_eccentricity_marker_mesh};
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

// System to spawn orbital plane visualizations for inclined orbits
pub fn spawn_orbital_planes(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<(Entity, &OrbitComponent), Without<OrbitalPlaneComponent>>,
) {
    for (orbit_entity, orbit_comp) in orbit_query.iter() {
        // Skip orbits with negligible inclination (mostly equatorial)
        if orbit_comp.tilt.x.abs() < 0.1 { // Less than ~6 degrees
            continue;
        }

        // Get orbit shape for this planet
        if commands.get_entity(orbit_comp.planet_entity).is_ok() {
            // Create orbital plane mesh - a translucent circle/disk
            let plane_radius = orbit_comp.radius * 1.2; // Slightly larger than orbit
            let plane_mesh = create_orbital_plane_mesh(&mut meshes, plane_radius);

            // Create material with inclination-based color and opacity
            let inclination_factor = orbit_comp.tilt.x.abs() / (std::f32::consts::PI / 2.0); // 0-1 scale
            let plane_color = Color::srgb(
                0.75 + 0.15 * inclination_factor, // Red increases with inclination
                0.82 + 0.08 * inclination_factor, // Green subtle increase
                0.90 - 0.10 * inclination_factor, // Blue decreases with inclination
            );
            let plane_opacity = 0.08 + 0.12 * inclination_factor; // More inclined = more visible

            let plane_material = StandardMaterial {
                base_color: plane_color.with_alpha(plane_opacity),
                emissive: LinearRgba::new(0.02, 0.025, 0.03, 1.0).with_alpha(plane_opacity * 0.5),
                unlit: true,
                alpha_mode: AlphaMode::Blend,
                double_sided: true,
                ..default()
            };

            let plane_material_handle = materials.add(plane_material);

            // Spawn orbital plane entity as child of orbit
            let plane_entity = commands.spawn((
                Mesh3d(plane_mesh),
                MeshMaterial3d(plane_material_handle.clone()),
                Transform::from_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    orbit_comp.tilt.x, // Inclination
                    orbit_comp.tilt.y, // Argument of periapsis approximation
                    0.0, // Ascending node (would need additional data)
                )),
                OrbitalPlaneComponent {
                    planet_entity: orbit_comp.planet_entity,
                    inclination_rad: orbit_comp.tilt.x,
                    ascending_node_rad: orbit_comp.tilt.y,
                    semi_major_axis: orbit_comp.radius,
                    eccentricity: 0.0, // Would need access to orbit shape data
                    material: plane_material_handle,
                    opacity: plane_opacity,
                },
                Name::new(format!("Orbital Plane for {:?}", orbit_comp.planet_entity)),
            )).id();

            // Add as child of orbit entity
            commands.entity(orbit_entity).add_child(plane_entity);
        }
    }
}

// System to update orbital plane materials with dynamic effects
pub fn update_orbital_planes(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut plane_query: Query<&mut OrbitalPlaneComponent>,
) {
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    if !frame_number.is_multiple_of(15) { // Less frequent updates for planes
        return;
    }

    let elapsed = time.elapsed_secs();

    for plane_comp in plane_query.iter_mut() {
        if let Some(material) = materials.get_mut(&plane_comp.material) {
            // Subtle pulsing effect based on inclination
            let inclination_pulse = 0.02 * (elapsed * 0.3 + plane_comp.inclination_rad * 10.0).sin();
            let dynamic_opacity = (plane_comp.opacity + inclination_pulse).clamp(0.03, 0.25);

            material.base_color = material.base_color.with_alpha(dynamic_opacity);
            material.emissive = LinearRgba::new(
                0.02 + inclination_pulse * 0.01,
                0.025 + inclination_pulse * 0.005,
                0.03 - inclination_pulse * 0.005,
                1.0,
            ).with_alpha(dynamic_opacity * 0.3);
        }
    }
}

// System to spawn eccentricity markers (apoapsis/periapsis points) for elliptical orbits
pub fn spawn_eccentricity_markers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<(Entity, &OrbitComponent), Without<EccentricityMarkersComponent>>,
) {
    for (orbit_entity, orbit_comp) in orbit_query.iter() {
        // Only create markers for orbits with significant eccentricity (>0.01)
        // For now, we'll use a placeholder eccentricity value since we need to access orbit shape data
        // In a real implementation, this would come from the planet's orbital elements
        let eccentricity = 0.0167; // Earth's actual eccentricity as example
        if eccentricity < 0.01 {
            continue;
        }

        // Calculate apoapsis and periapsis positions
        let semi_major_axis = orbit_comp.radius;
        let apoapsis_distance = semi_major_axis * (1.0 + eccentricity);
        let periapsis_distance = semi_major_axis * (1.0 - eccentricity);

        // For simplicity, place markers along the orbit path
        // In a full implementation, these would use proper Keplerian orbital calculations
        let apoapsis_pos = Vec3::new(apoapsis_distance, 0.0, 0.0);
        let periapsis_pos = Vec3::new(-periapsis_distance, 0.0, 0.0);

        // Create glowing sphere markers
        let marker_mesh = create_eccentricity_marker_mesh(&mut meshes, 2.0); // Small 2-unit radius spheres
        let apoapsis_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.89, 0.93).with_alpha(0.8), // Bright cool white for apoapsis
            emissive: LinearRgba::new(0.1, 0.12, 0.15, 1.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let periapsis_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.82, 0.88).with_alpha(0.9), // Slightly dimmer for periapsis
            emissive: LinearRgba::new(0.08, 0.10, 0.12, 1.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });

        // Spawn apoapsis marker
        commands.spawn((
            Mesh3d(marker_mesh.clone()),
            MeshMaterial3d(apoapsis_material.clone()),
            Transform::from_translation(apoapsis_pos),
            Name::new(format!("Apoapsis marker for {:?}", orbit_comp.planet_entity)),
        ));

        // Spawn periapsis marker
        commands.spawn((
            Mesh3d(marker_mesh),
            MeshMaterial3d(periapsis_material.clone()),
            Transform::from_translation(periapsis_pos),
            Name::new(format!("Periapsis marker for {:?}", orbit_comp.planet_entity)),
        ));

        // Add eccentricity markers component to orbit
        commands.entity(orbit_entity).insert(EccentricityMarkersComponent {
            planet_entity: orbit_comp.planet_entity,
            apoapsis_position: apoapsis_pos,
            periapsis_position: periapsis_pos,
            apoapsis_material,
            periapsis_material,
            eccentricity,
        });
    }
}

// System to update eccentricity markers with pulsing effects
pub fn update_eccentricity_markers(
    time: Res<Time>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    marker_query: Query<&EccentricityMarkersComponent>,
) {
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    if !frame_number.is_multiple_of(20) { // Less frequent updates
        return;
    }

    let elapsed = time.elapsed_secs();

    for marker_comp in marker_query.iter() {
        // Update apoapsis marker
        if let Some(apoapsis_material) = materials.get_mut(&marker_comp.apoapsis_material) {
            let apoapsis_pulse = 0.1 * (elapsed * 0.5 + marker_comp.eccentricity * 20.0).sin();
            let apoapsis_intensity = (0.8 + apoapsis_pulse).clamp(0.5, 1.2);
            apoapsis_material.emissive = LinearRgba::new(
                0.1 * apoapsis_intensity,
                0.12 * apoapsis_intensity,
                0.15 * apoapsis_intensity,
                1.0,
            );
        }

        // Update periapsis marker
        if let Some(periapsis_material) = materials.get_mut(&marker_comp.periapsis_material) {
            let periapsis_pulse = 0.08 * (elapsed * 0.7 + marker_comp.eccentricity * 15.0).sin();
            let periapsis_intensity = (0.9 + periapsis_pulse).clamp(0.6, 1.1);
            periapsis_material.emissive = LinearRgba::new(
                0.08 * periapsis_intensity,
                0.10 * periapsis_intensity,
                0.12 * periapsis_intensity,
                1.0,
            );
        }
    }
}