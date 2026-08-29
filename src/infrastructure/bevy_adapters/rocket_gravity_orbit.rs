//! Gravity and orbital-state adapters for the fixed rocket flight pipeline.

use crate::components::rocket::{
    GravityAcceleration, OrbitalElements, RocketMissionState, RocketPhysicsState,
    RocketPlanetBinding,
};
use crate::domain::services::gravity::{
    differential_gravitational_acceleration, gravitational_acceleration, gravitational_parameter,
};
use crate::domain::services::physics_orbital::{
    heliocentric_inertial_state_m, orbital_elements_from_state_in_reference_frame,
};
use crate::domain::services::reference_frames::{
    planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res};

/// Compute the authoritative gravitational acceleration from each rocket's
/// typed dominant-body binding.
pub fn update_rocket_gravity(
    simulation_time: Res<SimulationTime>,
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
        let time_days = simulation_time.sim_time_s / 86_400.0;
        let sun = planet_query
            .iter()
            .find(|candidate| candidate.domain_planet.name == "Sun");
        if let (Some(bound_state), Some(sun)) = (
            heliocentric_inertial_state_m(&planet.domain_planet, time_days),
            sun,
        ) {
            if let Some(sun_state) = heliocentric_inertial_state_m(&sun.domain_planet, time_days) {
                acceleration += differential_gravitational_acceleration(
                    sun.domain_planet.mass_kg,
                    rocket.dynamics.position_m,
                    DVec3::ZERO,
                    sun_state.position_m - bound_state.position_m,
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
    use crate::domain::services::planet_factory::PlanetFactory;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use crate::domain::services::simulation_time::SimulationTime;
    use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
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
        app.insert_resource(SimulationTime::default());
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

        let earth_state = heliocentric_inertial_state_m(&earth, 0.0).unwrap();
        let sun_state = heliocentric_inertial_state_m(&sun, 0.0).unwrap();
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
