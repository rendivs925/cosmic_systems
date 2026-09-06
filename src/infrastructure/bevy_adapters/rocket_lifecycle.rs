use crate::application::rocket_spawning::build_rocket_mesh;
use crate::components::rocket::*;
use crate::domain::services::landing_gear::LandingGear;
use crate::domain::services::reference_frames::surface_velocity_in_planet_inertial;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::{MaxQTracker, PlanetComponent};
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

/// Persistent hand-off from frame-rate input to the fixed flight simulation.
/// Unlike messages, these commands remain pending until a fixed tick consumes
/// them, even when rendering runs faster than the fixed schedule.
#[derive(Resource, Default)]
pub struct RelaunchCommandQueue(pub Vec<Entity>);

/// Highest permitted warp during an atmospheric descent. The bounded fixed
/// runner cannot present a controllable landing while it is draining a large
/// queued warp backlog, so terminal flight explicitly returns to real time.
const TERMINAL_FLIGHT_MAX_WARP: f64 = SimulationTime::REALTIME;
const TERMINAL_FLIGHT_ALTITUDE_M: f64 = 100_000.0;

/// Cancel outstanding warp demand once a vehicle enters terminal flight. This
/// is a presentation/control policy only: completed fixed steps remain intact,
/// while unexecuted future demand has never affected physical state.
pub fn constrain_terminal_time_warp(
    mut sim_time: ResMut<SimulationTime>,
    rocket_query: Query<(
        &RocketMissionState,
        &RocketPhysicsState,
        &RocketFlightConditions,
    )>,
) {
    if sim_time.time_acceleration <= TERMINAL_FLIGHT_MAX_WARP {
        return;
    }
    let terminal_flight = rocket_query.iter().any(|(mission, rocket, conditions)| {
        matches!(
            *mission,
            RocketMissionState::PoweredDescent
                | RocketMissionState::UnpoweredDescent
                | RocketMissionState::Landing
        ) || {
            let radial = rocket.dynamics.position_m.normalize_or_zero();
            conditions.altitude_m <= TERMINAL_FLIGHT_ALTITUDE_M
                && conditions.atmosphere_relative_velocity_mps.dot(radial) < -1.0
        }
    });
    if !terminal_flight {
        return;
    }
    let cancelled_backlog_s = sim_time.pending_simulation_s();
    sim_time.set_time_acceleration(TERMINAL_FLIGHT_MAX_WARP);
    sim_time.cancel_pending_simulation();
    bevy::log::info!(
        "Terminal flight forced time warp to real time; cancelled {cancelled_backlog_s:.1} s of unexecuted warp demand"
    );
}

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
#[expect(
    clippy::too_many_arguments,
    reason = "The relaunch transaction needs its queue, shared assets, and cohesive reset queries."
)]
pub fn apply_relaunch_requests(
    mut relaunch_queue: ResMut<RelaunchCommandQueue>,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    planet_query: Query<&PlanetComponent>,
    spent_stages: Query<(Entity, &SpentStage)>,
    mut commands: Commands,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &RocketGeometry,
        Option<&mut Mesh3d>,
        &mut RocketPhysicsState,
        &mut RocketPropulsion,
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
        Option<&mut SpecificForceAcceleration>,
        &mut OrbitalElements,
    )>,
) {
    for rocket_entity in std::mem::take(&mut relaunch_queue.0) {
        let (entity, total_mass_kg, initial_fairing) = {
            let Ok((
                entity,
                binding,
                geometry,
                rocket_mesh,
                mut rocket,
                mut propulsion,
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

            let attached_payload_kg = initial_fairing
                .map(|fairing| fairing.dry_mass_kg)
                .unwrap_or(0.0);
            propulsion.reset_for_relaunch(attached_payload_kg);
            if let (Some(mut rocket_mesh), Some(meshes)) = (rocket_mesh, meshes.as_deref_mut()) {
                *rocket_mesh = Mesh3d(build_rocket_mesh(meshes, &propulsion.vehicle));
            }

            // Mass and inertia from the refueled stack.
            let total_mass_kg =
                rocket.refresh_attached_mass_properties(&propulsion, *geometry, 0.0);

            // Upright and co-moving with the rotating pad at the current site.
            let Some(planet) = planet_query
                .iter()
                .find(|planet| planet.matches_body(&binding.planet_name))
            else {
                continue;
            };
            let Some(orientation) =
                ephemeris_snapshot.orientation_for_catalog_body(binding.planet_name.as_str())
            else {
                continue;
            };
            let radius_m = planet.domain_planet.radius_km as f64 * 1000.0;
            let position_m = rocket.dynamics.position_m;
            let up = position_m / position_m.length().max(1.0);
            rocket.dynamics.orientation = DQuat::from_rotation_arc(DVec3::Y, up);
            let lower_offset_world_m = rocket.dynamics.orientation * geometry.lower_extent_body_m();
            rocket.dynamics.position_m = up * radius_m - lower_offset_world_m;
            rocket.dynamics.velocity_mps =
                surface_velocity_in_planet_inertial(rocket.dynamics.position_m, orientation);
            rocket.dynamics.angular_velocity_radps = DVec3::ZERO;

            // Mission and lifecycle state back to a fresh pad.
            *mission_state = RocketMissionState::PreLaunch;
            rest.active = true;
            *scorecard = LandingScorecard::default();
            tip_over.reset();
            recorder.clear();
            match (
                propulsion
                    .vehicle
                    .stages
                    .first()
                    .and_then(|stage| stage.landing_gear),
                legs,
            ) {
                (Some(gear_spec), Some(mut legs)) => {
                    *legs = LandingLegs::new(LandingGear::new(gear_spec, total_mass_kg));
                }
                (Some(gear_spec), None) => {
                    commands
                        .entity(entity)
                        .insert(LandingLegs::new(LandingGear::new(gear_spec, total_mass_kg)));
                }
                (None, _) => {
                    commands.entity(entity).remove::<LandingLegs>();
                }
            }
            (entity, total_mass_kg, initial_fairing.copied())
        };

        // Restore the attached fairing independently of presentation/reset
        // facade availability. Its mass was already rebuilt from the same
        // immutable launch component above, so this cannot create a second
        // mass authority.
        if let Some(fairing) = initial_fairing {
            commands.entity(entity).insert(PayloadFairing {
                dry_mass_kg: fairing.dry_mass_kg,
            });
        }

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
            specific_force,
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
        if let Some(mut specific_force) = specific_force {
            *specific_force = SpecificForceAcceleration::default();
        }
        *orbital_elements = OrbitalElements::default();
        bevy::log::info!(
            "Relaunch ready: {:.0} kg refueled, vehicle upright and held on the pad",
            total_mass_kg
        );
    }
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "Launch input remains beside its lifecycle implementation."
)]
mod tests {
    use super::*;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use bevy::math::DMat3;

    #[test]
    fn terminal_descent_cancels_unexecuted_warp_before_more_physics_runs() {
        let mut app = App::new();
        let mut sim_time = SimulationTime::default();
        sim_time.set_time_acceleration(1_000.0);
        sim_time.accrue_warp(1.0);
        app.insert_resource(sim_time)
            .add_systems(Update, constrain_terminal_time_warp);
        app.world_mut().spawn((
            RocketMissionState::PoweredDescent,
            RocketPhysicsState {
                dynamics: RocketDynamicsState::new(
                    DVec3::X * 6_371_000.0,
                    DVec3::ZERO,
                    DQuat::IDENTITY,
                    1_000.0,
                    DMat3::IDENTITY,
                    DVec3::ZERO,
                ),
            },
            RocketFlightConditions::default(),
        ));

        app.update();

        let sim_time = app.world().resource::<SimulationTime>();
        assert_eq!(sim_time.time_acceleration, SimulationTime::REALTIME);
        assert_eq!(sim_time.pending_simulation_s(), 0.0);
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
