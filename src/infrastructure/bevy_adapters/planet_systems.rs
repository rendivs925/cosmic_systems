use super::components::*;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

/// Update the solar map from the fixed simulation clock. This path intentionally
/// has no camera, worker, or GPU dependency so every platform evaluates the
/// same catalog and Kepler solver at a given simulation time.
pub fn update_planet_positions(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
    mut perf_stats: ResMut<PerformanceStats>,
) {
    let physics_start = std::time::Instant::now();
    let time_days = solar_params.time_to_days(time.elapsed_secs());

    update_planet_positions_sequential(time_days, &solar_params, &mut query);

    perf_stats.physics_update_time = physics_start.elapsed().as_secs_f32() * 1000.0;
    perf_stats.simd_enabled = false;
    perf_stats.parallel_enabled = false;
    perf_stats.cpu_cores_used = 1;
}

fn update_planet_positions_sequential(
    time_days: f32,
    solar_params: &SolarSystemParameters,
    query: &mut Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    // Parent bodies must be evaluated before moons so moon positions always use
    // the current fixed-step parent state.
    for (_, mut transform, planet_comp) in query.iter_mut() {
        if planet_comp.domain_planet.parent_entity.is_some() {
            continue;
        }
        transform.translation = physics::calculate_planet_position(
            &planet_comp.domain_planet,
            time_days,
            solar_params,
            Vec3::ZERO,
            None,
        );
    }

    let mut parent_positions = std::collections::HashMap::new();
    let mut parent_tilts = std::collections::HashMap::new();
    for (_, transform, planet_comp) in query.iter() {
        if planet_comp.domain_planet.parent_entity.is_none() {
            parent_positions.insert(
                planet_comp.domain_planet.name.clone(),
                transform.translation,
            );
            parent_tilts.insert(
                planet_comp.domain_planet.name.clone(),
                planet_comp.domain_planet.axial_tilt_deg,
            );
        }
    }

    for (_, mut transform, planet_comp) in query.iter_mut() {
        let Some(parent_name) = planet_comp.domain_planet.parent_entity.as_ref() else {
            continue;
        };
        let Some(parent_position) = parent_positions.get(parent_name).copied() else {
            continue;
        };
        let parent_tilt = parent_tilts.get(parent_name).copied();
        transform.translation = physics::calculate_planet_position(
            &planet_comp.domain_planet,
            time_days,
            solar_params,
            parent_position,
            parent_tilt,
        );
    }
}

pub fn update_planet_rotations(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    update_planet_rotations_at(solar_params.time_to_days(time.elapsed_secs()), &mut query);
}

/// Evaluate solar-map presentation at the fixed schedule's fractional overstep.
/// Authoritative fixed state remains independent from this visual smoothing.
pub fn interpolate_planet_transforms(
    time: Res<Time<Fixed>>,
    solar_params: Res<SolarSystemParameters>,
    mut query: Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    let presentation_days =
        solar_params.time_to_days(time.elapsed_secs() + time.overstep().as_secs_f32());
    update_planet_positions_sequential(presentation_days, &solar_params, &mut query);
    update_planet_rotations_at(presentation_days, &mut query);
}

fn update_planet_rotations_at(
    time_days: f32,
    query: &mut Query<(Entity, &mut Transform, &PlanetComponent)>,
) {
    for (_, mut transform, planet_comp) in query.iter_mut() {
        let rotation_angle =
            physics::calculate_planet_rotation(&planet_comp.domain_planet, time_days);
        let tilt = Quat::from_rotation_z(planet_comp.domain_planet.axial_tilt_deg.to_radians());
        transform.rotation = tilt * Quat::from_rotation_y(rotation_angle);
    }
}

/// Move moon orbit presentation with its parent body. The mesh itself remains in
/// its parent-relative orbital frame and does not participate in simulation.
pub fn update_moon_orbit_positions(
    mut moon_orbit_query: Query<(&mut Transform, &OrbitComponent), With<MoonOrbit>>,
    planet_query: Query<(&Transform, &PlanetComponent), Without<MoonOrbit>>,
) {
    for (mut orbit_transform, orbit_comp) in moon_orbit_query.iter_mut() {
        let Ok((parent_transform, parent_comp)) = planet_query.get(orbit_comp.planet_entity) else {
            continue;
        };
        orbit_transform.translation = parent_transform.translation;
        orbit_transform.rotation =
            Quat::from_rotation_z(parent_comp.domain_planet.axial_tilt_deg.to_radians());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presentation_time_advances_through_fixed_overstep() {
        let solar = SolarSystemParameters::for_visualization();
        let fixed_seconds = 10.0;
        let presentation_seconds = fixed_seconds + 0.25;

        assert!(solar.time_to_days(presentation_seconds) > solar.time_to_days(fixed_seconds));
    }
}
