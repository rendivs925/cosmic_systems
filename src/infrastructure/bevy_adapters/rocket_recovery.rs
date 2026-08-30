//! ECS adapters for autonomous drone-ship recovery.
//!
//! The domain recovery models own prediction and station-keeping control. This
//! module integrates their f64 state and applies a post-integration deck
//! constraint, analogous to terrain resting contact, without touching render
//! transforms.

use crate::components::rocket::{
    DroneShip, DroneShipLandingTarget, GroundRest, LandingLegs, LandingScorecard, RocketAutopilot,
    RocketGeometry, RocketMissionState, RocketPhysicsState,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::services::terrain_collision::{
    decompose_velocity, evaluate_touchdown, GroundContact, TouchdownCriteria, TOUCHDOWN_BAND_M,
};
use crate::infrastructure::bevy_adapters::components::TerrainCollisionState;
use crate::infrastructure::bevy_adapters::rocket_contact::record_scorecard;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res};

/// Integrate each vessel from its bounded station-keeping thrust. The domain
/// controller supplies the command; this adapter owns only ECS state updates.
pub fn station_keep_drone_ships(sim_time: Res<SimulationTime>, mut ships: Query<&mut DroneShip>) {
    let dt = sim_time.fixed_timestep();
    for mut ship in &mut ships {
        if ship.state.mass_kg <= 0.0 || !ship.state.mass_kg.is_finite() {
            continue;
        }
        let thrust_n = ship
            .station_keeper
            .thrust(&ship.state, ship.station_target_position_m);
        let acceleration_mps2 = ship.state.external_accel_mps2 + thrust_n / ship.state.mass_kg;
        ship.state.velocity_mps += acceleration_mps2 * dt;
        let velocity_mps = ship.state.velocity_mps;
        ship.state.position_m += velocity_mps * dt;
    }
}

/// Resolve a stage against a drone ship's moving deck after 6-DOF integration.
/// The contact verdict is entirely deck-relative: both velocity and lateral
/// error are measured in the deck frame, then the normal constraint latches
/// the authoritative rocket state to the vessel rather than terrain.
#[expect(
    clippy::type_complexity,
    reason = "The recovery query combines cohesive rocket state for landing-target guidance."
)]
pub fn resolve_drone_ship_deck_contact(
    sim_time: Res<SimulationTime>,
    ships: Query<&DroneShip>,
    mut rockets: Query<(
        &mut DroneShipLandingTarget,
        &mut RocketPhysicsState,
        &RocketGeometry,
        &mut TerrainCollisionState,
        &mut GroundRest,
        &mut RocketMissionState,
        Option<&LandingLegs>,
        &mut LandingScorecard,
        &RocketAutopilot,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (
        mut target,
        mut rocket,
        geometry,
        mut collision,
        mut rest,
        mut mission,
        legs,
        mut scorecard,
        autopilot,
    ) in &mut rockets
    {
        let Ok(ship) = ships.get(target.drone_ship) else {
            continue;
        };
        let normal = ship.state.position_m.normalize_or_zero();
        if normal.length_squared() < 0.5 {
            continue;
        }
        let relative_position_m = rocket.dynamics.position_m - ship.state.position_m;
        let deck_altitude_m = relative_position_m.dot(normal);
        let deck_lateral_m = relative_position_m - normal * deck_altitude_m;
        let relative_velocity_mps = rocket.dynamics.velocity_mps - ship.state.velocity_mps;

        if target.deck_contact {
            // The deck is a moving resting-contact frame. Remove normal motion
            // and damp lateral slip deterministically while following its drift.
            if deck_altitude_m < 0.0 {
                rocket.dynamics.position_m -= normal * deck_altitude_m;
            }
            let normal_speed_mps = relative_velocity_mps.dot(normal);
            let tangential_mps = relative_velocity_mps - normal * normal_speed_mps;
            rocket.dynamics.velocity_mps =
                ship.state.velocity_mps + tangential_mps * (-12.0 * dt).exp();
            rest.active = true;
            collision.radar_altitude_m = 0.0;
            collision.slope_deg = 0.0;
            collision.over_water = false;
            collision.ground_contact = GroundContact::Landed;
            continue;
        }

        if deck_lateral_m
            .x
            .abs()
            .max(deck_lateral_m.y.abs())
            .max(deck_lateral_m.z.abs())
            > ship.deck_half_extent_m
            || deck_altitude_m > TOUCHDOWN_BAND_M
        {
            continue;
        }

        let components = decompose_velocity(relative_velocity_mps, normal);
        if components.normal_mps > 0.0 {
            continue;
        }
        let tilt_deg = (rocket.dynamics.orientation * DVec3::Y)
            .angle_between(normal)
            .to_degrees();
        let criteria = match legs.filter(|legs| legs.deployed()) {
            Some(legs) => legs
                .gear
                .touchdown_criteria(TouchdownCriteria::default(), geometry.height_m as f64),
            None => TouchdownCriteria::default(),
        };
        let verdict = evaluate_touchdown(
            -components.normal_mps,
            components.lateral_mps,
            0.0,
            tilt_deg,
            &criteria,
        );
        collision.ground_contact = verdict;
        match verdict {
            GroundContact::Landed => {
                if deck_altitude_m < 0.0 {
                    rocket.dynamics.position_m -= normal * deck_altitude_m;
                }
                rocket.dynamics.velocity_mps = ship.state.velocity_mps + relative_velocity_mps
                    - normal * relative_velocity_mps.dot(normal);
                rest.active = true;
                target.deck_contact = true;
                collision.radar_altitude_m = 0.0;
                collision.slope_deg = 0.0;
                collision.over_water = false;
                record_scorecard(
                    &mut scorecard,
                    -components.normal_mps,
                    components.lateral_mps,
                    tilt_deg,
                    0.0,
                    rocket.dynamics.position_m,
                    ship.state.position_m.length(),
                    autopilot.target_landing_position_m,
                    false,
                );
                if matches!(
                    *mission,
                    RocketMissionState::PoweredDescent
                        | RocketMissionState::UnpoweredDescent
                        | RocketMissionState::Landing
                        | RocketMissionState::ReentryCorridor
                ) {
                    *mission = RocketMissionState::Landed;
                }
            }
            GroundContact::Crash => {
                if *mission != RocketMissionState::PreLaunch {
                    *mission = RocketMissionState::Crashed;
                }
            }
            GroundContact::None => {}
        }
    }
}
