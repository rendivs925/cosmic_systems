use crate::components::rocket::*;
use crate::domain::services::landing_gear::LegDeploymentState;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass_with_payload,
};
use crate::infrastructure::bevy_adapters::components::{MaxQTracker, PlanetComponent};
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

/// Persistent hand-off from frame-rate input to the fixed flight simulation.
/// Unlike messages, these commands remain pending until a fixed tick consumes
/// them, even when rendering runs faster than the fixed schedule.
#[derive(Resource, Default)]
pub struct RelaunchCommandQueue(pub Vec<Entity>);

/// Relaunch input (Phase 14): R commands a full pad-style reset of every
/// vehicle. Presentation-adjacent input handling only — the mutation happens
/// in FixedUpdate via [`apply_relaunch_requests`].
pub fn handle_relaunch_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    rockets: Query<Entity, With<RocketPhysicsState>>,
    mut relaunch_queue: ResMut<RelaunchCommandQueue>,
) {
    if !keyboard.just_pressed(KeyCode::KeyR) {
        return;
    }
    for entity in rockets.iter() {
        relaunch_queue.0.push(entity);
        bevy::log::info!("Relaunch requested for rocket {entity}");
    }
}

/// Apply relaunch commands (Phase 14): refuel every stage to its configured
/// propellant load, reset the vehicle upright and at rest at the current
/// site, restore mission PreLaunch, re-stow gear, clear lifecycle state, and
/// despawn jettisoned debris. One authority for the whole reset; runs before
/// guidance so the auto-launch takes over on the same tick.
#[allow(clippy::type_complexity)]
pub fn apply_relaunch_requests(
    mut relaunch_queue: ResMut<RelaunchCommandQueue>,
    planet_query: Query<&PlanetComponent>,
    spent_stages: Query<(Entity, &SpentStage)>,
    mut commands: Commands,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketGeometry,
        &mut RocketPhysicsState,
        &mut RocketPropulsion,
        &mut RocketMass,
        &mut RocketMissionState,
        &mut GroundRest,
        Option<&mut LandingLegs>,
        &mut LandingScorecard,
        &mut TipOverState,
        &mut FlightRecorder,
        Option<&InitialPayloadFairing>,
    )>,
    mut reset_query: Query<(
        &mut RocketCommands,
        &mut RocketAutopilot,
        &mut RocketFlightConditions,
        &mut TerrainCollisionState,
        &mut ThermalState,
        &mut AblationState,
        &mut CommsState,
        &mut ParachuteState,
        &mut RetroPropulsionEffect,
        &mut MaxQTracker,
        &mut ForceAccumulator,
        &mut TorqueAccumulator,
        &mut OrbitalElements,
    )>,
) {
    for rocket_entity in std::mem::take(&mut relaunch_queue.0) {
        let (entity, total_mass_kg, initial_fairing) = {
            let Ok((
                entity,
                binding,
                geometry,
                mut rocket,
                mut propulsion,
                mut mass,
                mut mission_state,
                mut rest,
                legs,
                mut scorecard,
                mut tip_over,
                mut recorder,
                initial_fairing,
            )) = rocket_query.get_mut(rocket_entity)
            else {
                continue;
            };

            // Jettisoned hardware goes away with the flight that shed it.
            for (debris, spent) in spent_stages.iter() {
                if spent.parent_rocket == entity {
                    commands.entity(debris).despawn();
                }
            }

            // Refuel from the authoritative configuration and reset propulsion.
            propulsion.propellant_remaining_kg = propulsion
                .vehicle
                .stages
                .iter()
                .map(|stage| stage.propellant_mass_kg)
                .collect();
            propulsion.active_stage = 0;
            propulsion.separations_count = 0;
            propulsion.throttle = 0.0;
            propulsion.gimbal_pitch_rad = 0.0;
            propulsion.gimbal_yaw_rad = 0.0;
            propulsion.time_since_separation_s = propulsion.ullage_settle_time_s;
            propulsion.attached_payload_kg = initial_fairing
                .map(|fairing| fairing.dry_mass_kg)
                .unwrap_or(0.0);

            // Mass and inertia from the refueled stack.
            let total_mass_kg = active_vehicle_mass_with_payload(
                &propulsion.vehicle.stages,
                &propulsion.propellant_remaining_kg,
                0,
                propulsion.attached_payload_kg,
            );
            let (inertia, com) = active_vehicle_inertia(
                &propulsion.vehicle.stages,
                &propulsion.propellant_remaining_kg,
                0,
                propulsion.attached_payload_kg,
                0.0,
                geometry.radius_m as f64,
                geometry.height_m as f64,
            );
            rocket.dynamics.mass_kg = total_mass_kg;
            rocket.dynamics.inertia_body = inertia;
            rocket.dynamics.center_of_mass_m = com;
            mass.0 = total_mass_kg;

            // Upright, motionless, resting at the current site.
            let Some(planet) = planet_query
                .iter()
                .find(|planet| planet.matches_body(&binding.planet_name))
            else {
                continue;
            };
            let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
            let position_m = rocket.dynamics.position_m;
            let up = position_m / position_m.length().max(1.0);
            rocket.dynamics.position_m = up * (position_m.length().max(radius_m));
            rocket.dynamics.velocity_mps = DVec3::ZERO;
            rocket.dynamics.angular_velocity_radps = DVec3::ZERO;
            rocket.dynamics.orientation = DQuat::from_rotation_arc(DVec3::Y, up);

            // Mission and lifecycle state back to a fresh pad.
            *mission_state = RocketMissionState::PreLaunch;
            rest.active = true;
            if let Some(mut legs) = legs {
                legs.deployment = LegDeploymentState::default();
                legs.compression_m = 0.0;
            }
            *scorecard = LandingScorecard::default();
            tip_over.reset();
            recorder.clear();
            (entity, total_mass_kg, initial_fairing.copied())
        };

        let Ok((
            mut rocket_commands,
            mut autopilot,
            mut flight_conditions,
            mut terrain_collision,
            mut thermal,
            mut ablation,
            mut comms,
            mut parachute,
            mut retro_propulsion,
            mut max_q,
            mut force_accumulator,
            mut torque_accumulator,
            mut orbital_elements,
        )) = reset_query.get_mut(entity)
        else {
            continue;
        };
        *rocket_commands = RocketCommands::default();
        autopilot.integral = DVec3::ZERO;
        autopilot.time_since_liftoff_s = 0.0;
        *flight_conditions = RocketFlightConditions::default();
        *terrain_collision = TerrainCollisionState::default();
        *thermal = ThermalState::default();
        *ablation = AblationState::default();
        *comms = CommsState::default();
        *parachute = ParachuteState::default();
        *retro_propulsion = RetroPropulsionEffect::default();
        *max_q = MaxQTracker::default();
        *force_accumulator = ForceAccumulator::default();
        *torque_accumulator = TorqueAccumulator::default();
        *orbital_elements = OrbitalElements::default();
        if let Some(fairing) = initial_fairing {
            commands.entity(entity).insert(PayloadFairing {
                dry_mass_kg: fairing.dry_mass_kg,
            });
        }
        commands.entity(entity).remove::<PlannedManeuver>();

        bevy::log::info!(
            "Relaunch ready: {:.0} kg refueled, vehicle upright and held on the pad",
            total_mass_kg
        );
    }
}

/// Handles the pre-launch hold: Space key arms the launch.
pub fn handle_rocket_launch_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut mission_query: Query<(
        &mut RocketMissionState,
        &RocketPhysicsState,
        &mut RocketRenderState,
    )>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        for (mut mission, rocket, mut render) in mission_query.iter_mut() {
            if *mission == RocketMissionState::PreLaunch {
                // Prelaunch renders the latest body-fixed pad state rather
                // than its interpolation buffer. Reset that buffer before
                // enabling airborne interpolation to avoid blending two
                // stale rotating-pad snapshots on the launch transition.
                *render = RocketRenderState::new(rocket.dynamics);
                *mission = RocketMissionState::Launch;
            }
        }
    }
}
