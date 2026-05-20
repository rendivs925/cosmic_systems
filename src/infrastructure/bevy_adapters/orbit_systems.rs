use super::components::*;
use crate::application::material_factory::ORBIT_LINE_COLOR;
use crate::application::mesh_factory::{create_orbit_ribbon_mesh, create_orbital_plane_mesh, create_eccentricity_marker_mesh, create_uv_sphere_mesh};
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::performance_components::{PerformanceStats, QualityLevel};
use bevy::prelude::*;
use bevy::render::alpha::AlphaMode;

// System to update orbit visuals with class-based colors and camera-distance-based opacity
pub(crate) fn update_orbit_visuals(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    time: Res<Time>,
    selected_planet: Res<SelectedPlanet>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<(&OrbitComponent, &GlobalTransform)>,
) {
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 4;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 2;

    if !frame_number.is_multiple_of(update_stride) {
        return;
    }

    let Ok(camera) = camera_query.single() else {
        return;
    };
    let camera_pos = camera.translation();
    let elapsed = time.elapsed_secs();

    for (orbit_comp, orbit_transform) in orbit_query.iter() {
        if let Some(material) = materials.get_mut(&orbit_comp.material) {
            let is_selected = selected_planet.entity == Some(orbit_comp.planet_entity);
            let linear_color: LinearRgba = ORBIT_LINE_COLOR.into();

            // Distance-based opacity using actual orbit center in world space
            let orbit_center = orbit_transform.translation();
            let dist = camera_pos.distance(orbit_center).max(1.0);
            let radius = orbit_comp.radius.max(1.0);
            let ratio = dist / radius;
            // Fade in gradually from 0.1× to 0.5× radius, hold, fade out from 5× to 8× radius
            let near_fade = ((ratio / 0.5).min(1.0)).powf(2.0);
            let far_fade = (1.0 - ((ratio - 3.0) / 5.0).clamp(0.0, 1.0)).max(0.01);
            let base_opacity = near_fade * far_fade;

            let base_opacity = base_opacity.clamp(0.01, 0.07);
            let final_base_opacity = if is_selected {
                (base_opacity * 2.5).min(0.16)
            } else {
                base_opacity
            };

            let orbital_harmonic = orbit_comp.radius * 0.0001;
            let stellar_resonance = 0.02 * (elapsed * 0.004 + orbital_harmonic).sin();
            let cosmic_pulse = 0.985 + 0.015 * (elapsed * 0.005).sin() + stellar_resonance;
            let alpha = final_base_opacity * cosmic_pulse;

            material.base_color = ORBIT_LINE_COLOR.with_alpha(alpha);

            let emissive_intensity = if is_selected { 0.06 } else { 0.03 };
            let emissive_pulse = 0.90 + 0.10 * cosmic_pulse;
            material.emissive = LinearRgba::new(
                linear_color.red * emissive_intensity * emissive_pulse,
                linear_color.green * emissive_intensity * emissive_pulse,
                linear_color.blue * emissive_intensity * emissive_pulse,
                1.0,
            );
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
        let eccentricity = orbit_comp.orbit_shape.eccentricity;
        if eccentricity < 0.01 {
            continue;
        }

        let semi_major = orbit_comp.orbit_shape.semi_major_axis_units;

        // Apoapsis at true anomaly = 0°, periapsis at true anomaly = π
        let e = orbit_comp.orbit_shape.eccentricity.clamp(0.0, 0.99);
        let semi_latus = semi_major * (1.0 - e * e);
        let apoapsis_r = semi_latus / (1.0 + e * 1.0);
        let periapsis_r = semi_latus / (1.0 - e);

        let apoapsis_pos = physics::transform_orbital_point(
            apoapsis_r, 0.0,
            orbit_comp.orbit_shape.inclination_rad,
            orbit_comp.orbit_shape.long_asc_node_rad,
            orbit_comp.orbit_shape.arg_periapsis_rad,
        );
        let periapsis_pos = physics::transform_orbital_point(
            -periapsis_r, 0.0,
            orbit_comp.orbit_shape.inclination_rad,
            orbit_comp.orbit_shape.long_asc_node_rad,
            orbit_comp.orbit_shape.arg_periapsis_rad,
        );

        let marker_mesh = create_eccentricity_marker_mesh(&mut meshes, 2.0);
        let apoapsis_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.85, 0.89, 0.93).with_alpha(0.8),
            emissive: LinearRgba::new(0.1, 0.12, 0.15, 1.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        let periapsis_material = materials.add(StandardMaterial {
            base_color: Color::srgb(0.75, 0.82, 0.88).with_alpha(0.9),
            emissive: LinearRgba::new(0.08, 0.10, 0.12, 1.0),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });

        commands.spawn((
            Mesh3d(marker_mesh.clone()),
            MeshMaterial3d(apoapsis_material.clone()),
            Transform::from_translation(apoapsis_pos),
            Name::new(format!("Apoapsis marker for {:?}", orbit_comp.planet_entity)),
        ));

        commands.spawn((
            Mesh3d(marker_mesh),
            MeshMaterial3d(periapsis_material.clone()),
            Transform::from_translation(periapsis_pos),
            Name::new(format!("Periapsis marker for {:?}", orbit_comp.planet_entity)),
        ));

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

// System to update orbit ribbon thickness based on camera distance
pub fn update_orbit_thickness(
    camera_query: Query<&GlobalTransform, With<CameraController>>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut orbit_query: Query<(&mut OrbitComponent, &mut Mesh3d, &GlobalTransform)>,
) {
    let frame_number = (time.elapsed_secs() * 60.0) as u32;
    #[cfg(target_arch = "wasm32")]
    let update_stride = 6;
    #[cfg(not(target_arch = "wasm32"))]
    let update_stride = 3;

    if !frame_number.is_multiple_of(update_stride) {
        return;
    }

    let Ok(camera) = camera_query.single() else {
        return;
    };
    let camera_pos = camera.translation();

    for (mut orbit_comp, mut mesh3d, orbit_transform) in orbit_query.iter_mut() {
        let orbit_center = orbit_transform.translation();
        let dist_to_camera = camera_pos.distance(orbit_center).max(1.0);
        let ref_dist = orbit_comp.radius.max(1.0);
        let thickness_scale = (dist_to_camera / ref_dist).clamp(0.5, 8.0);
        let new_thickness = orbit_comp.radius * 0.0001 * thickness_scale;

        if (new_thickness - orbit_comp.thickness).abs() > orbit_comp.thickness * 0.15 {
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

// System to spawn position tracker markers for each orbit
pub fn spawn_position_trackers(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    orbit_query: Query<(Entity, &OrbitComponent), Without<TrackerSpawned>>,
    planet_query: Query<&PlanetComponent>,
) {
    for (orbit_entity, orbit_comp) in orbit_query.iter() {
        let Ok(planet_comp) = planet_query.get(orbit_comp.planet_entity) else {
            continue;
        };
        let tracker_radius = 3.0;
        let tracker_mesh = create_uv_sphere_mesh(&mut meshes, tracker_radius);
        let tracker_color = ORBIT_LINE_COLOR;
        let tracker_material = materials.add(StandardMaterial {
            base_color: tracker_color,
            emissive: {
                let c: LinearRgba = tracker_color.into();
                LinearRgba::new(c.red * 0.5, c.green * 0.5, c.blue * 0.5, 1.0)
            },
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            ..default()
        });
        commands.entity(orbit_entity).insert(TrackerSpawned);
        commands.spawn((
            Mesh3d(tracker_mesh),
            MeshMaterial3d(tracker_material),
            Transform::default(),
            PositionTracker {
                planet_entity: orbit_comp.planet_entity,
                planet_name: planet_comp.domain_planet.name.clone(),
            },
            Name::new(format!("Position tracker {}", planet_comp.domain_planet.name)),
        ));
    }
}

// System to update position tracker markers to follow their planet's position
pub fn update_position_trackers(
    planet_query: Query<(Entity, &Transform), With<PlanetComponent>>,
    mut tracker_query: Query<(&PositionTracker, &mut Transform), Without<PlanetComponent>>,
) {
    for (tracker, mut transform) in tracker_query.iter_mut() {
        if let Ok((_, planet_transform)) = planet_query.get(tracker.planet_entity) {
            transform.translation = planet_transform.translation;
        }
    }
}

// System to adapt orbit segment count and thickness based on quality level
pub fn update_orbit_quality(
    perf_stats: Res<PerformanceStats>,
    mut last_quality: Local<Option<QualityLevel>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut orbit_query: Query<(&mut OrbitComponent, &mut Mesh3d)>,
) {
    let current_quality = perf_stats.quality_level;
    if *last_quality == Some(current_quality) {
        return;
    }
    *last_quality = Some(current_quality);

    let (segments, thickness_mult) = match current_quality {
        QualityLevel::Ultra => (512, 1.4),
        QualityLevel::High => (256, 1.2),
        QualityLevel::Medium => (128, 1.0),
        QualityLevel::Low => (64, 0.7),
        QualityLevel::Minimal => (32, 0.4),
    };

    for (mut orbit_comp, mut mesh3d) in orbit_query.iter_mut() {
        if orbit_comp.segments != segments {
            orbit_comp.segments = segments;
            let new_thickness = orbit_comp.radius * 0.0001 * thickness_mult;
            orbit_comp.thickness = new_thickness;
            let new_mesh = create_orbit_ribbon_mesh(
                &mut meshes,
                &orbit_comp.orbit_shape,
                ORBIT_LINE_COLOR,
                new_thickness,
                segments,
            );
            mesh3d.0 = new_mesh;
        }
    }
}