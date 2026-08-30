use super::components::*;
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::physics;
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, catalog_body_fixed_to_inertial_rotation,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::math::DVec3;
use bevy::prelude::*;

/// An unresolved HDR point expands through bloom as the post-process kernel,
/// which is visibly rectangular. Resolve a small circular Sun disc first.
const MIN_SUN_PRESENTATION_RADIUS_PX: f32 = 5.0;

/// Update the solar map from the shared scientific epoch. This path intentionally
/// has no camera, worker, or GPU dependency so every platform evaluates the
/// same catalog and Kepler solver at a given simulation time.
pub fn update_planet_positions(
    simulation_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    physical_scale: Res<PhysicalScale>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut SolarMapPosition, &PlanetComponent)>,
    mut perf_stats: ResMut<PerformanceStats>,
) {
    let physics_start = std::time::Instant::now();
    let Ok(epoch) = simulation_time.tdb_epoch() else {
        bevy::log::error!("cannot update solar-map positions without a scientific epoch");
        return;
    };
    let time_days = epoch.seconds_since_j2000() / 86_400.0;

    update_planet_positions_sequential(
        time_days,
        &ephemeris_snapshot,
        &physical_scale,
        &solar_params,
        &mut query,
    );

    perf_stats.physics_update_time = physics_start.elapsed().as_secs_f32() * 1000.0;
    perf_stats.simd_enabled = false;
    perf_stats.parallel_enabled = false;
    perf_stats.cpu_cores_used = 1;
}

fn update_planet_positions_sequential(
    time_days: f64,
    ephemeris_snapshot: &EphemerisSnapshot,
    physical_scale: &PhysicalScale,
    solar_params: &SolarSystemParameters,
    query: &mut Query<(&mut SolarMapPosition, &PlanetComponent)>,
) {
    // Parent bodies must be evaluated before moons so moon positions always use
    // the current fixed-step parent state.
    for (mut position, planet_comp) in query.iter_mut() {
        if planet_comp.domain_planet.parent_entity.is_some() {
            continue;
        }
        if let Some(ephemeris_position) = solar_map_position_from_snapshot(
            ephemeris_snapshot,
            &planet_comp.domain_planet.name,
            physical_scale,
        ) {
            position.0 = ephemeris_position;
        }
    }

    let mut parent_positions = std::collections::HashMap::new();
    let mut parent_tilts = std::collections::HashMap::new();
    for (position, planet_comp) in query.iter() {
        if planet_comp.domain_planet.parent_entity.is_none() {
            parent_positions.insert(planet_comp.domain_planet.name.clone(), position.0);
            parent_tilts.insert(
                planet_comp.domain_planet.name.clone(),
                planet_comp.domain_planet.axial_tilt_deg,
            );
        }
    }

    for (mut position, planet_comp) in query.iter_mut() {
        let Some(parent_name) = planet_comp.domain_planet.parent_entity.as_ref() else {
            continue;
        };
        if let Some(ephemeris_position) = solar_map_position_from_snapshot(
            ephemeris_snapshot,
            &planet_comp.domain_planet.name,
            physical_scale,
        ) {
            position.0 = ephemeris_position;
            continue;
        }
        let Some(parent_position) = parent_positions.get(parent_name).copied() else {
            continue;
        };
        let parent_tilt = parent_tilts.get(parent_name).copied();
        position.0 = physics::calculate_planet_position_f64(
            &planet_comp.domain_planet,
            time_days,
            solar_params,
            parent_position,
            parent_tilt,
        );
    }
}

pub fn update_planet_rotations(
    simulation_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    let Ok(epoch) = simulation_time.tdb_epoch() else {
        bevy::log::error!("cannot update solar-map rotations without a scientific epoch");
        return;
    };
    update_planet_rotations_at(
        epoch.seconds_since_j2000() / 86_400.0,
        &ephemeris_snapshot,
        &mut query,
    );
}

/// Evaluate solar-map presentation at the fixed schedule's fractional overstep.
/// Authoritative fixed state remains independent from this visual smoothing.
pub fn interpolate_planet_transforms(
    simulation_time: Res<SimulationTime>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    physical_scale: Res<PhysicalScale>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(&mut SolarMapPosition, &PlanetComponent)>,
) {
    let Ok(epoch) = simulation_time.tdb_epoch() else {
        bevy::log::error!("cannot interpolate solar-map positions without a scientific epoch");
        return;
    };
    update_planet_positions_sequential(
        epoch.seconds_since_j2000() / 86_400.0,
        &ephemeris_snapshot,
        &physical_scale,
        &solar_params,
        &mut query,
    );
}

/// Project a snapshot's SSB/ICRF state into the existing heliocentric
/// solar-map frame. The f64 meter-to-display conversion remains solely at this
/// presentation boundary.
pub(crate) fn solar_map_position_from_snapshot(
    ephemeris_snapshot: &EphemerisSnapshot,
    catalog_name: &str,
    physical_scale: &PhysicalScale,
) -> Option<DVec3> {
    let target = NaifBodyId::for_catalog_name(catalog_name)?;
    let solar_state = ephemeris_snapshot.solar_inertial_relative_state(target, NaifBodyId::SUN)?;
    Some(DVec3::new(
        physical_scale.solar_meters_to_units(solar_state.position_m.x),
        physical_scale.solar_meters_to_units(solar_state.position_m.y),
        physical_scale.solar_meters_to_units(solar_state.position_m.z),
    ))
}

fn update_planet_rotations_at(
    time_days: f64,
    ephemeris_snapshot: &EphemerisSnapshot,
    query: &mut Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    for (_, mut transform, planet_comp) in query.iter_mut() {
        transform.rotation = ephemeris_snapshot
            .orientation_for_catalog_body(&planet_comp.domain_planet.name)
            .map(body_fixed_to_planet_inertial_rotation)
            // Unmapped moons remain presentation-only catalog approximations.
            .unwrap_or_else(|| {
                catalog_body_fixed_to_inertial_rotation(&planet_comp.domain_planet, time_days)
            })
            .as_quat();
    }
}

/// Project f64 solar-map positions into an origin-relative render frame. The
/// origin tracks the selected body, preserving the local motion of outer moons
/// instead of subtracting multi-million-unit f32 coordinates on the GPU.
pub fn rebase_solar_presentation(
    selected_planet: Res<SelectedPlanet>,
    mut origin: ResMut<SolarMapRenderOrigin>,
    positions: Query<&SolarMapPosition>,
    mut camera_query: Query<&mut Transform, (With<CameraController>, Without<PlanetComponent>)>,
    mut planet_query: Query<(&SolarMapPosition, &mut Transform), With<PlanetComponent>>,
    mut solar_light_query: Query<
        &mut Transform,
        (
            With<SolarMapLight>,
            Without<CameraController>,
            Without<PlanetComponent>,
        ),
    >,
    mut previous_selected: Local<Option<Entity>>,
) {
    let next_origin = selected_planet
        .entity
        .and_then(|entity| positions.get(entity).ok())
        .map_or(DVec3::ZERO, |position| position.0);
    let rebase_delta = (origin.position_units - next_origin).as_vec3();

    // A selected-body follow camera is already expressed in the moving local
    // frame. Shifting it every presentation update introduces visible jitter.
    if *previous_selected != selected_planet.entity && rebase_delta != Vec3::ZERO {
        for mut camera_transform in camera_query.iter_mut() {
            camera_transform.translation += rebase_delta;
        }
    }
    origin.position_units = next_origin;
    *previous_selected = selected_planet.entity;

    for (position, mut transform) in planet_query.iter_mut() {
        transform.translation = solar_map_render_translation(position.0, origin.position_units);
    }

    // The sunlight source belongs at solar-inertial origin, never at the
    // selected body's render origin.
    for mut light_transform in solar_light_query.iter_mut() {
        light_transform.translation = solar_light_render_position(origin.position_units);
    }
}

/// Keep the rendered solar disc large enough to remain circular at overview
/// distances. This changes only the presentation mesh scale: `SolarMapPosition`,
/// the physical solar radius, and the calibrated point light remain authoritative.
pub fn preserve_sun_disc_at_overview_distances(
    camera_query: Query<(&Camera, &GlobalTransform, &Projection), With<CameraController>>,
    solar_params: Res<SolarSystemParameters>,
    mut planet_query: Query<(&PlanetComponent, &mut Transform)>,
) {
    let Some((camera, camera_transform, Projection::Perspective(projection))) =
        camera_query.iter().find(|(camera, _, _)| camera.is_active)
    else {
        return;
    };
    let Some(viewport_size) = camera.logical_viewport_size() else {
        return;
    };

    let physical_radius_units = physics::calculate_sun_visual_radius(&solar_params);
    for (planet, mut transform) in &mut planet_query {
        if planet.domain_planet.name != "Sun" {
            continue;
        }

        let distance_units = camera_transform
            .translation()
            .distance(transform.translation);
        let scale = sun_presentation_scale(
            distance_units,
            projection.fov,
            viewport_size.y,
            physical_radius_units,
        );
        transform.scale = Vec3::splat(scale);
    }
}

fn sun_presentation_scale(
    distance_units: f32,
    vertical_fov_rad: f32,
    viewport_height_px: f32,
    physical_radius_units: f32,
) -> f32 {
    if !distance_units.is_finite()
        || !vertical_fov_rad.is_finite()
        || !viewport_height_px.is_finite()
        || !physical_radius_units.is_finite()
        || distance_units <= 0.0
        || vertical_fov_rad <= 0.0
        || viewport_height_px <= 0.0
        || physical_radius_units <= 0.0
    {
        return 1.0;
    }

    let units_per_pixel =
        2.0 * distance_units * (vertical_fov_rad * 0.5).tan() / viewport_height_px;
    let minimum_radius_units = units_per_pixel * MIN_SUN_PRESENTATION_RADIUS_PX;
    (minimum_radius_units / physical_radius_units).max(1.0)
}

fn solar_light_render_position(render_origin_units: DVec3) -> Vec3 {
    (-render_origin_units).as_vec3()
}

/// Project local orbit meshes into the shared camera-relative solar-map frame.
/// Mesh vertices remain parent-relative (moons) or heliocentric (planets), so
/// rebasing never requires regenerating static orbital geometry.
pub fn update_orbit_positions(
    mut orbit_query: Query<(&mut Transform, &OrbitComponent, Has<MoonOrbit>)>,
    planet_query: Query<(&SolarMapPosition, &PlanetComponent), Without<MoonOrbit>>,
    origin: Res<SolarMapRenderOrigin>,
) {
    for (mut orbit_transform, orbit_comp, is_moon) in orbit_query.iter_mut() {
        let (orbit_center, orbit_rotation) = if is_moon {
            let Ok((parent_position, parent_comp)) = planet_query.get(orbit_comp.planet_entity)
            else {
                continue;
            };
            (
                parent_position.0,
                Quat::from_rotation_z(parent_comp.domain_planet.axial_tilt_deg.to_radians()),
            )
        } else {
            (DVec3::ZERO, Quat::IDENTITY)
        };
        orbit_transform.translation =
            solar_map_render_translation(orbit_center, origin.position_units);
        orbit_transform.rotation = orbit_rotation;
    }
}

fn solar_map_render_translation(position_units: DVec3, render_origin_units: DVec3) -> Vec3 {
    (position_units - render_origin_units).as_vec3()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;

    #[test]
    fn presentation_time_advances_through_fixed_overstep() {
        let solar = SolarSystemParameters::for_visualization();
        let fixed_seconds = 10.0;
        let presentation_seconds = fixed_seconds + 0.25;

        assert!(solar.time_to_days(presentation_seconds) > solar.time_to_days(fixed_seconds));
    }

    #[test]
    fn snapshot_primary_position_uses_the_solar_display_boundary() {
        let epoch = TdbEpoch::j2000();
        let snapshot = EphemerisSnapshot::from_states(vec![
            BodyState {
                target: NaifBodyId::SUN,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                epoch,
                position_m: DVec3::ZERO,
                velocity_mps: DVec3::ZERO,
            },
            BodyState {
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                epoch,
                position_m: DVec3::X * crate::domain::value_objects::physical_scale::AU_IN_METERS,
                velocity_mps: DVec3::ZERO,
            },
        ]);
        let scale = PhysicalScale::default();

        let position = solar_map_position_from_snapshot(&snapshot, "Earth", &scale).unwrap();

        assert!(
            (position.x - scale.solar_scale_factor as f64).abs()
                < scale.solar_scale_factor as f64 * 1.0e-6
        );
        assert!(position.y.abs() < 1.0e-9);
        assert!(position.z.abs() < 1.0e-9);
    }

    #[test]
    fn solar_light_remains_at_solar_origin_after_rebasing() {
        assert_eq!(
            solar_light_render_position(DVec3::new(1_500_000.0, -25.0, 800.0)),
            Vec3::new(-1_500_000.0, 25.0, -800.0),
        );
    }

    #[test]
    fn orbit_and_body_projection_share_the_same_render_origin() {
        let origin = DVec3::new(1_500_000.0, -25.0, 800.0);

        assert_eq!(
            solar_map_render_translation(DVec3::ZERO, origin),
            solar_light_render_position(origin)
        );
    }

    #[test]
    fn distant_sun_uses_a_minimum_resolved_disc() {
        let scale = sun_presentation_scale(1_500_000.0, 1.0, 1_000.0, 350.0);

        assert!(scale > 1.0);
        let displayed_radius_px = scale * 350.0 / (2.0 * 1_500_000.0 * 0.5_f32.tan()) * 1_000.0;
        assert!((displayed_radius_px - MIN_SUN_PRESENTATION_RADIUS_PX).abs() < 1e-5);
    }

    #[test]
    fn resolved_sun_keeps_its_physical_radius() {
        assert_eq!(sun_presentation_scale(1_000.0, 1.0, 1_000.0, 350.0), 1.0);
    }
}
