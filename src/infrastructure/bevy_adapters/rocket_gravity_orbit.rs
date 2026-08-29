//! Gravity and orbital-state adapters for the fixed rocket flight pipeline.

use crate::components::rocket::{
    GravityAcceleration, OrbitalElements, RocketMissionState, RocketPhysicsState,
    RocketPlanetBinding,
};
use crate::domain::services::gravity::{gravitational_acceleration, gravitational_parameter};
use crate::domain::services::physics_orbital::orbital_elements_from_state_in_reference_frame;
use crate::domain::services::reference_frames::{
    planet_equatorial_reference_x_axis, planet_inertial_spin_axis,
};
use crate::infrastructure::bevy_adapters::components::PlanetComponent;
use bevy::math::DVec3;
use bevy::prelude::Query;

/// Compute the authoritative gravitational acceleration from each rocket's
/// typed dominant-body binding.
pub fn update_rocket_gravity(
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

        gravity.value = gravitational_acceleration(
            planet.domain_planet.mass_kg,
            rocket.dynamics.position_m,
            DVec3::ZERO,
        );
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
