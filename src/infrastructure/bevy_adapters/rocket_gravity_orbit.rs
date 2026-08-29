//! Gravity and orbital-state adapters for the fixed rocket flight pipeline.

use crate::components::rocket::{
    GravityAcceleration, OrbitalElements, RocketMissionState, RocketPhysicsState,
    RocketPlanetBinding,
};
use crate::domain::services::ephemeris::NaifBodyId;
use crate::domain::services::gravity::{
    differential_gravitational_acceleration, gravitational_acceleration, gravitational_parameter,
};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    barycentric_to_solar_inertial_state, planet_equatorial_reference_x_axis,
    planet_inertial_spin_axis,
};
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res};

/// Compute the authoritative gravitational acceleration from each rocket's
/// typed dominant-body binding.
pub fn update_rocket_gravity(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &mut GravityAcceleration,
    )>,
) {
    for (binding, rocket, mut gravity) in rocket_query.iter_mut() {
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };

        let mut acceleration = gravitational_acceleration(
            planet.domain_planet.mass_kg,
            rocket.dynamics.position_m,
            DVec3::ZERO,
        );

        // The rocket state is planet-centered inertial, so the Sun contributes
        // only its acceleration relative to the bound planet. Adding the Sun's
        // full heliocentric acceleration here would double-count the frame
        // origin's own acceleration and eject local flights incorrectly.
        let bound_body = NaifBodyId::for_catalog_name(binding.planet_name.as_str());
        let bound_state = bound_body.and_then(|body| ephemeris_snapshot.state(body));
        let sun_state = ephemeris_snapshot.state(NaifBodyId::SUN);
        let sun_mass_kg = planet_query
            .iter()
            .find(|candidate| candidate.domain_planet.name == "Sun")
            .map(|sun| sun.domain_planet.mass_kg);
        if let (Some(bound_state), Some(sun_state), Some(sun_mass_kg)) =
            (bound_state, sun_state, sun_mass_kg)
        {
            if let Ok(sun_relative_to_bound) =
                barycentric_to_solar_inertial_state(sun_state, bound_state)
            {
                acceleration += differential_gravitational_acceleration(
                    sun_mass_kg,
                    rocket.dynamics.position_m,
                    DVec3::ZERO,
                    sun_relative_to_bound.position_m,
                );
            }
        }

        gravity.value = acceleration;
    }
}

/// Compute post-integration orbital elements for telemetry and next-tick
/// guidance from the authoritative planet-centered inertial state.
pub fn update_orbital_elements(
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &RocketPhysicsState,
        &RocketMissionState,
        &mut OrbitalElements,
    )>,
) {
    for (binding, rocket, mission, mut elements) in rocket_query.iter_mut() {
        if *mission == RocketMissionState::PreLaunch {
            *elements = OrbitalElements::default();
            continue;
        }
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };

        let state = orbital_elements_from_state_in_reference_frame(
            rocket.dynamics.position_m,
            rocket.dynamics.velocity_mps,
            gravitational_parameter(planet.domain_planet.mass_kg),
            planet_inertial_spin_axis(&planet.domain_planet),
            planet_equatorial_reference_x_axis(&planet.domain_planet),
        );
        elements.semi_major_axis_m = state.semi_major_axis_m;
        elements.eccentricity = state.eccentricity;
        elements.inclination_rad = state.inclination_rad;
        elements.longitude_ascending_node_rad = state.longitude_ascending_node_rad;
        elements.argument_of_periapsis_rad = state.argument_of_periapsis_rad;
        elements.true_anomaly_rad = state.true_anomaly_rad;
        elements.mean_anomaly_rad = state.mean_anomaly_rad;
        elements.orbital_period_s = state.orbital_period_s;
        elements.apoapsis_m = state.apoapsis_m;
        elements.periapsis_m = state.periapsis_m;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::ephemeris::{BodyState, TdbEpoch};
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
    use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
    use bevy::math::{DMat3, DQuat};
    use bevy::prelude::{default, App, Update};

    fn planet_component(name: &str) -> PlanetComponent {
        PlanetComponent {
            domain_planet: PlanetFactory::create_by_name(name).unwrap(),
            material: default(),
            has_texture: false,
            base_reflectance: 0.0,
            base_roughness: 0.0,
        }
    }

    #[test]
    fn primary_bound_rocket_adds_solar_differential_gravity() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let sun = PlanetFactory::create_by_name("Sun").unwrap();
        let rocket_position_m = DVec3::X * (earth.radius_km as f64 * 1_000.0 + 400_000.0);
        let mut app = App::new();
        let epoch = TdbEpoch::j2000();
        let earth_state = BodyState {
            target: NaifBodyId::EARTH,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: DVec3::ZERO,
            velocity_mps: DVec3::ZERO,
        };
        let sun_state = BodyState {
            target: NaifBodyId::SUN,
            center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            epoch,
            position_m: -DVec3::X * 149_597_870_700.0,
            velocity_mps: DVec3::ZERO,
        };
        app.insert_resource(EphemerisSnapshot::from_states(vec![earth_state, sun_state]));
        app.world_mut().spawn(planet_component("Earth"));
        app.world_mut().spawn(planet_component("Sun"));
        let rocket = app
            .world_mut()
            .spawn((
                RocketPlanetBinding {
                    planet_name: CelestialBodyId::earth(),
                },
                RocketPhysicsState {
                    dynamics: RocketDynamicsState::new(
                        rocket_position_m,
                        DVec3::ZERO,
                        DQuat::IDENTITY,
                        1_000.0,
                        DMat3::IDENTITY,
                        DVec3::ZERO,
                    ),
                },
                GravityAcceleration::default(),
            ))
            .id();
        app.add_systems(Update, update_rocket_gravity);

        app.update();

        let expected = gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO)
            + differential_gravitational_acceleration(
                sun.mass_kg,
                rocket_position_m,
                DVec3::ZERO,
                sun_state.position_m - earth_state.position_m,
            );
        let actual = app
            .world()
            .get::<GravityAcceleration>(rocket)
            .unwrap()
            .value;

        assert_eq!(actual, expected);
        assert!(
            (actual - gravitational_acceleration(earth.mass_kg, rocket_position_m, DVec3::ZERO))
                .length()
                > 1.0e-7
        );
    }
}
