//! Terrain-contact preparation and constraint adapters.

use crate::components::rocket::{
    GroundRest, LandingLegs, RocketMissionState, RocketPhysicsState, RocketPlanetBinding,
    TipOverState,
};
use crate::domain::services::gravity::gravitational_parameter;
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::GroundContact;
use crate::infrastructure::bevy_adapters::components::{PlanetComponent, TerrainCollisionState};
use bevy::log::info;
use bevy::math::{DMat3, DQuat, DVec3};
use bevy::prelude::{Query, Res};

/// Advance the one-way gear-deployment latch before terrain-contact resolution.
pub fn deploy_landing_legs(
    mut rocket_query: Query<(
        &TerrainCollisionState,
        &RocketPhysicsState,
        &GroundRest,
        &mut LandingLegs,
    )>,
) {
    for (collision, rocket, ground_rest, mut legs) in rocket_query.iter_mut() {
        if ground_rest.active || collision.ground_contact == GroundContact::Landed {
            continue;
        }
        let radius_m = rocket.dynamics.position_m.length();
        if radius_m < 1.0 {
            continue;
        }
        let up_dir = rocket.dynamics.position_m / radius_m;
        let vertical_speed_mps = rocket.dynamics.velocity_mps.dot(up_dir);
        let deploy_gate_altitude_m = legs.deploy_gate_altitude_m();
        if legs.deployment.update(
            deploy_gate_altitude_m,
            collision.radar_altitude_m,
            vertical_speed_mps,
        ) {
            info!(
                "Landing legs deployed at {:.0} m AGL",
                collision.radar_altitude_m
            );
        }
    }
}

/// Advance an armed ground-contact topple and apply its attitude to the
/// authoritative simulation state. This runs after contact resolution.
pub fn advance_topple(
    sim_time: Res<SimulationTime>,
    planet_query: Query<&PlanetComponent>,
    mut rocket_query: Query<(
        &RocketPlanetBinding,
        &mut RocketPhysicsState,
        &mut TipOverState,
        &mut RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (binding, mut rocket, mut tip_over, mut mission_state) in rocket_query.iter_mut() {
        if tip_over.fall.is_none() {
            continue;
        }
        let com_height_m = tip_over.com_height_m;
        let Some(planet) = planet_query
            .iter()
            .find(|planet| planet.matches_body(&binding.planet_name))
        else {
            continue;
        };
        let radius_m = rocket.dynamics.position_m.length();
        if radius_m < 1.0 {
            continue;
        }
        let up_dir = rocket.dynamics.position_m / radius_m;
        let body_y = rocket.dynamics.orientation * DVec3::Y;
        let fall_dir_h = (body_y - up_dir * body_y.dot(up_dir)).normalize_or_zero();
        if fall_dir_h.length_squared() < 0.5 {
            continue;
        }

        let gravity_mps2 = gravitational_parameter(planet.domain_planet.mass_kg) / radius_m.powi(2);
        let fall = tip_over.fall.as_mut().expect("armed above");
        let completed = fall.advance(gravity_mps2, com_height_m, dt);

        let y_new = up_dir * fall.tilt_rad.cos() + fall_dir_h * fall.tilt_rad.sin();
        let x_old = body_y.cross(y_new).cross(body_y).normalize_or_zero();
        let x_new = if x_old.length_squared() > 0.5 {
            x_old
        } else {
            fall_dir_h.cross(up_dir).normalize_or_zero()
        };
        if x_new.length_squared() < 0.5 {
            continue;
        }
        let z_new = x_new.cross(y_new);
        rocket.dynamics.orientation = DQuat::from_mat3(&DMat3::from_cols(
            x_new.normalize(),
            y_new,
            z_new.normalize(),
        ));

        if completed && *mission_state != RocketMissionState::Crashed {
            *mission_state = RocketMissionState::Crashed;
            info!("Vehicle toppled over; mission lost");
        }
    }
}
