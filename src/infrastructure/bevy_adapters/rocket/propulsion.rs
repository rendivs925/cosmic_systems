//! Propulsion force, mass-flow, and gimbal-torque adapters.

use super::components::*;
use super::events::StageSeparatedEvent;
use super::recovery::RecoveryDeckEntity;
use super::separation::{spawn_spent_stage, SpentStageSpec};
use crate::application::rocket_spawning::{build_rocket_mesh, build_serial_stage_mesh};
use crate::domain::entities::rocket::{EngineState, Rocket};
use crate::domain::services::guidance::{AutopilotMode, EntryBurnPlan, EntryBurnSequence};
use crate::domain::services::landing_gear::LandingGear;
use crate::domain::services::rcs::REPRESENTATIVE_RCS_ISP_S;
use crate::domain::services::rocket_propulsion::{
    burn_duration_s, consume_propellant, separate_parallel_boosters_dynamics,
    separate_stage_dynamics, shed_stage, stage_gimbal_torque_body, stage_gimbaled_thrust_body,
    stage_mass_properties, stage_thrust_body, MIN_SEPARATION_CLEARANCE_M,
    PARALLEL_BOOSTER_SEPARATION_DV_MPS, SEPARATION_UPPER_DV_MPS, SPENT_STAGE_RETRO_DV_MPS,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use bevy::camera::primitives::Aabb;
use bevy::log::info;
use bevy::math::DVec3;
use bevy::prelude::{
    Assets, Commands, Entity, Mesh, Mesh3d, MessageWriter, Query, Res, ResMut, StandardMaterial,
};

/// Separate an empty stage and refresh the surviving vehicle mass.
/// The spent stage receives the domain separation impulse and is spawned as
/// independent debris; the upper stage must settle propellant before restart.
#[expect(
    clippy::type_complexity,
    reason = "The staging query combines cohesive rocket state required for one transition."
)]
pub fn propulsion_staging(
    mut commands: Commands,
    mut meshes: Option<ResMut<Assets<Mesh>>>,
    mut materials: Option<ResMut<Assets<StandardMaterial>>>,
    sim_time: Res<SimulationTime>,
    recovery_deck: Option<Res<RecoveryDeckEntity>>,
    mut separated_writer: MessageWriter<StageSeparatedEvent>,
    mut rocket_query: Query<(
        Entity,
        &RocketPlanetBinding,
        &mut RocketGeometry,
        Option<&mut Mesh3d>,
        &mut RocketPhysicsState,
        &mut RocketPropulsion,
        Option<&AblationState>,
        Option<&RocketAutopilot>,
        Option<&LaunchSiteCoordinates>,
    )>,
) {
    let dt = sim_time.fixed_timestep() as f32;
    for (
        entity,
        binding,
        mut geometry,
        mut rocket_mesh,
        mut rocket,
        mut propulsion,
        ablation,
        autopilot,
        launch_site,
    ) in rocket_query.iter_mut()
    {
        propulsion.time_since_separation_s += dt;

        if let Some((boosters, booster_propellant_remaining_kg)) = propulsion.attached_boosters() {
            if propulsion.attached_boosters_are_depleted() {
                let boosters = boosters.clone();
                let booster_dynamics = separate_parallel_boosters_dynamics(
                    rocket.dynamics,
                    &boosters,
                    booster_propellant_remaining_kg,
                    PARALLEL_BOOSTER_SEPARATION_DV_MPS,
                );
                propulsion.detach_boosters();
                let mut serial_stack = propulsion.vehicle.clone();
                serial_stack.parallel_boosters = None;
                if let (Some(rocket_mesh), Some(meshes)) =
                    (rocket_mesh.as_deref_mut(), meshes.as_deref_mut())
                {
                    *rocket_mesh = Mesh3d(build_rocket_mesh(meshes, &serial_stack, 0.0));
                    // Bevy only inserts an `Aabb` for entities that lack one, so
                    // an in-place mesh swap must drop the stale bounds or the
                    // vehicle can be frustum-culled against its former size.
                    commands.entity(entity).remove::<Aabb>();
                }
                rocket.refresh_attached_mass_properties(
                    &propulsion,
                    *geometry,
                    ablation.map_or(0.0, |state| state.mass_loss_kg),
                );
                for dynamics in booster_dynamics {
                    let spent_entity = spawn_spent_stage(
                        &mut commands,
                        meshes.as_deref_mut(),
                        materials.as_deref_mut(),
                        SpentStageSpec {
                            parent_rocket: entity,
                            planet_id: binding.planet_name.clone(),
                            dynamics,
                            radius_m: boosters.stage.diameter_m * 0.5,
                            height_m: boosters.stage.height_m,
                            // Parallel boosters never inherit core stage gear.
                            landing_gear: None,
                            grid_fins: None,
                            kind: SpentStageKind::Booster,
                        },
                    );
                    separated_writer.write(StageSeparatedEvent {
                        rocket: entity,
                        spent_stage: spent_entity,
                        shed_mass_kg: dynamics.mass_kg,
                    });
                }
                info!("{} parallel boosters separated", boosters.count());
                continue;
            }
            // A serial core stage remains structurally attached until its
            // parallel boosters have completed their concurrent burn.
            continue;
        }

        let Some(active_core_stage) = propulsion.active_core_stage() else {
            continue;
        };
        if active_core_stage.has_burnable_propellant() {
            continue;
        }
        let Some((next, shed)) = shed_stage(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
        ) else {
            continue;
        };

        let pre_separation = rocket.dynamics;
        propulsion.active_stage = next;
        propulsion.separations_count += 1;

        let separated_stage = &propulsion.vehicle.stages[propulsion.active_stage - 1];
        let active_stage = &propulsion.vehicle.stages[propulsion.active_stage];
        geometry.radius_m = active_stage.diameter_m * 0.5;
        geometry.height_m = active_stage.height_m;
        geometry.lower_extent_y_m = -active_stage.height_m * 0.5;
        if let (Some(rocket_mesh), Some(meshes)) =
            (rocket_mesh.as_deref_mut(), meshes.as_deref_mut())
        {
            let attached_upper_envelope_height_m = (propulsion.vehicle.height_m
                - propulsion.vehicle.stages[..=propulsion.active_stage]
                    .iter()
                    .map(|stage| stage.height_m)
                    .sum::<f32>())
            .max(0.0);
            *rocket_mesh = Mesh3d(build_serial_stage_mesh(
                meshes,
                active_stage,
                attached_upper_envelope_height_m,
            ));
            // Drop the stale bounds so Bevy recomputes them for the smaller
            // upper-stage mesh (see the parallel-booster swap above).
            commands.entity(entity).remove::<Aabb>();
        }

        let ablation_mass_loss_kg = ablation.map_or(0.0, |ablation| ablation.mass_loss_kg);
        let upper_properties = stage_mass_properties(
            active_stage,
            propulsion
                .propellant_remaining_kg
                .get(propulsion.active_stage)
                .copied()
                .unwrap_or(0.0),
            propulsion.attached_payload_kg,
            ablation_mass_loss_kg,
        );
        let spent_properties = stage_mass_properties(
            separated_stage,
            propulsion
                .propellant_remaining_kg
                .get(propulsion.active_stage - 1)
                .copied()
                .unwrap_or(0.0),
            0.0,
            0.0,
        );
        let separated = separate_stage_dynamics(
            pre_separation,
            upper_properties,
            spent_properties,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
            MIN_SEPARATION_CLEARANCE_M,
        );
        rocket.dynamics = separated.upper;
        let new_mass = rocket.dynamics.mass_kg;
        let spent_dynamics = separated.spent;
        let recovery_reserve_kg = separated_stage.recovery_propellant_reserve_kg;

        // The surviving stack owns only its newly active stage's hardware.
        // Replacing an existing component is required when the new active
        // stage also has gear; otherwise remove the lower-stage component.
        match active_stage.landing_gear {
            Some(gear_spec) => {
                commands
                    .entity(entity)
                    .insert(LandingLegs::new(LandingGear::new(gear_spec, new_mass)));
            }
            None => {
                commands.entity(entity).remove::<LandingLegs>();
            }
        }

        let spent_entity = spawn_spent_stage(
            &mut commands,
            meshes.as_deref_mut(),
            materials.as_deref_mut(),
            SpentStageSpec {
                parent_rocket: entity,
                planet_id: binding.planet_name.clone(),
                dynamics: spent_dynamics,
                radius_m: separated_stage.diameter_m * 0.5,
                height_m: separated_stage.height_m,
                // Only an independently recoverable serial stage receives
                // its own gear assembly and child presentation meshes.
                landing_gear: recovery_reserve_kg.and(separated_stage.landing_gear),
                grid_fins: recovery_reserve_kg.and(separated_stage.grid_fins),
                kind: SpentStageKind::Booster,
            },
        );
        if let Some(recovery_reserve_kg) = recovery_reserve_kg {
            // The parent stage is now a standalone vehicle. Clear its reserve
            // marker so the recovery burns may consume the propellant that was
            // deliberately withheld from ascent. Cloning preserves each
            // engine's consumed ignition count; it is never reset on stage
            // separation.
            let mut recovery_stage = propulsion.vehicle.stages[propulsion.active_stage - 1].clone();
            recovery_stage.recovery_propellant_reserve_kg = None;
            let mut recovery_autopilot = autopilot.cloned().unwrap_or_default();
            // A deck recovery is a downrange mission: the stage has no
            // boostback energy to fly home, so it separates directly into the
            // engines-first entry law and lights an entry burn below. Without a
            // deck the existing return-to-launch profile is unchanged.
            let recovery_deck_entity = recovery_deck.as_ref().and_then(|deck| deck.0);
            recovery_autopilot.mode = if recovery_deck_entity.is_some() {
                AutopilotMode::EntryDescent
            } else {
                AutopilotMode::Boostback
            };
            recovery_autopilot.integral = DVec3::ZERO;
            recovery_autopilot.time_since_liftoff_s = 0.0;
            // The recovery stage's landing target must come from the recovery
            // mission input: the launch pad is re-derived every tick from the
            // launch site, and a moving deck owns it when present. Carrying
            // the ascent autopilot's snapshot forward is only correct for a
            // return-to-launch flight; a downrange stage has no pad, so a stale
            // ascent target would steer terminal guidance at a site hundreds of
            // kilometres away.
            recovery_autopilot.target_landing_position_m = DVec3::ZERO;
            let recovery_vehicle = Rocket {
                name: format!("{} recovery", recovery_stage.name),
                diameter_m: separated_stage.diameter_m,
                height_m: separated_stage.height_m,
                stages: vec![recovery_stage],
                parallel_boosters: None,
            };
            // A recovering booster carries cold-gas thrusters. Capture the
            // cluster sizing before the vehicle and autopilot are moved into
            // their components.
            let rcs_authority_nm = recovery_autopilot.actuation.max_rcs_torque_nm;
            let rcs_isp_s = recovery_vehicle
                .stages
                .first()
                .and_then(|stage| stage.engines.first())
                .map_or(REPRESENTATIVE_RCS_ISP_S, |engine| {
                    f64::from(engine.isp_vacuum)
                });
            commands.entity(spent_entity).insert((
                RecoveringStage,
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle: recovery_vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: vec![recovery_reserve_kg],
                    booster_attachment: BoosterAttachmentState::Detached,
                    throttle: 0.0,
                    gimbal_pitch_rad: 0.0,
                    gimbal_yaw_rad: 0.0,
                    time_since_separation_s: 0.0,
                    ullage_settle_time_s: propulsion.ullage_settle_time_s,
                    separations_count: 1,
                    attached_payload_kg: 0.0,
                },
                RocketCommands::default(),
                // The recovery stage selects engines by subset; nine keeps
                // the full stage until a recovery profile asks otherwise.
                EngineSelection::new(9),
                recovery_autopilot,
                AerodynamicForces::default(),
                MaxQTracker::default(),
                TerrainCollisionState::default(),
                GroundRest { active: false },
                TipOverState::default(),
                LandingScorecard::default(),
                OrbitalElements::default(),
            ));
            if let Some(launch_site) = launch_site {
                // The recovery stage returns to the pad that launched it. The
                // launch site is a body-fixed coordinate; guidance re-derives
                // the inertial target every tick from the current body
                // orientation, because a return flight spans enough time for
                // the pad to move hundreds of kilometres.
                commands.entity(spent_entity).insert(launch_site.clone());
            }
            commands.entity(spent_entity).insert((
                ThermalState::default(),
                AblationState::default(),
                ParachuteState::default(),
                CommsState::default(),
                RetroPropulsionEffect::default(),
                // The recovered stage flies its own authoritative force
                // pipeline, so it needs the same applied-propulsion sample as
                // the primary vehicle; presentation reads it to keep the
                // recovery-burn plume tied to real thrust instead of throttle.
                AppliedPropulsionState::default(),
            ));
            // A recovering booster carries cold-gas thrusters. Without the
            // per-thruster cluster it would fall back to the legacy scalar
            // clamp, which cannot represent minimum-impulse quantization or
            // the propellant draw that the cluster models.
            commands
                .entity(spent_entity)
                .insert(RocketRcs::representative(rcs_authority_nm, rcs_isp_s));
            if let Some(deck) = recovery_deck_entity {
                // The deck owns the landing target from separation onward. A
                // zero prediction horizon makes guidance read the deck's live
                // body-fixed position every tick instead of extrapolating a
                // rotating platform in a straight line.
                commands.entity(spent_entity).insert((
                    DroneShipLandingTarget {
                        drone_ship: deck,
                        prediction_horizon_s: 0.0,
                        deck_contact: false,
                    },
                    EntryBurnState {
                        sequence: EntryBurnSequence::default(),
                        plan: EntryBurnPlan::FALCON9_DOWNRANGE,
                    },
                ));
            }
        }
        propulsion.time_since_separation_s = 0.0;

        separated_writer.write(StageSeparatedEvent {
            rocket: entity,
            spent_stage: spent_entity,
            shed_mass_kg: shed,
        });
        info!(
            "Stage {} separated at T+{:.1} s: shed {shed:.0} kg, upper stage {:.0} kg",
            propulsion.active_stage - 1,
            sim_time.sim_time_s,
            new_mass
        );
    }
}

/// Add pressure-corrected engine thrust in the inertial frame.
#[expect(
    clippy::type_complexity,
    reason = "The query groups the cohesive authoritative propulsion state and its optional diagnostic channel."
)]
pub fn propulsion_thrust(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &RocketPropulsion,
        &RetroPropulsionEffect,
        Option<&mut AppliedPropulsionState>,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, conditions, propulsion, retro, applied, mut force_accum) in rocket_query.iter_mut()
    {
        // Diagnostic output is optional so minimal test fixtures without the
        // channel still apply authoritative thrust. Production always spawns it.
        let mut thrust_n = 0.0;
        let mut total_mass_flow_kg_s = 0.0;
        let mut active_engine_count = 0_u32;
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if let Some((active_core_stage, core_throttle)) = propulsion.running_core_stage() {
            let (thrust_body, mass_flow_kg_s) = stage_gimbaled_thrust_body(
                &active_core_stage.stage().engines,
                core_throttle,
                conditions.ambient_pressure_pa,
                propulsion.gimbal_pitch_rad as f64,
                propulsion.gimbal_yaw_rad as f64,
            );
            let burn_fraction = burn_duration_s(
                active_core_stage.burnable_propellant_kg(),
                mass_flow_kg_s,
                sim_time.fixed_timestep(),
            ) / sim_time.fixed_timestep();
            thrust_n += thrust_body.length() * retro.thrust_multiplier * burn_fraction;
            total_mass_flow_kg_s += mass_flow_kg_s * burn_fraction;
            active_engine_count += active_core_stage
                .stage()
                .engines
                .iter()
                .filter(|engine| engine.state == EngineState::Running)
                .count() as u32;
            force_accum.add_force_n(
                rocket.dynamics.orientation * thrust_body * retro.thrust_multiplier * burn_fraction,
            );
        }
        if let Some((boosters, _)) = propulsion.attached_boosters() {
            for booster_index in 0..boosters.count() {
                if !propulsion.booster_is_ignitable(booster_index) {
                    continue;
                }
                let (booster_thrust_body, booster_mass_flow_kg_s) = stage_gimbaled_thrust_body(
                    &boosters.stage.engines,
                    throttle,
                    conditions.ambient_pressure_pa,
                    propulsion.gimbal_pitch_rad as f64,
                    propulsion.gimbal_yaw_rad as f64,
                );
                let remaining = propulsion
                    .attached_booster_inventory()
                    .expect("attached boosters have a fixed propellant inventory")[booster_index];
                let burn_fraction =
                    burn_duration_s(remaining, booster_mass_flow_kg_s, sim_time.fixed_timestep())
                        / sim_time.fixed_timestep();
                force_accum.add_force_n(
                    rocket.dynamics.orientation
                        * booster_thrust_body
                        * retro.thrust_multiplier
                        * burn_fraction,
                );
                thrust_n += booster_thrust_body.length() * retro.thrust_multiplier * burn_fraction;
                total_mass_flow_kg_s += booster_mass_flow_kg_s * burn_fraction;
                active_engine_count += boosters
                    .stage
                    .engines
                    .iter()
                    .filter(|engine| engine.state == EngineState::Running)
                    .count() as u32;
            }
        }
        if let Some(mut applied) = applied {
            *applied = AppliedPropulsionState {
                thrust_n,
                mass_flow_kg_s: total_mass_flow_kg_s,
                active_engine_count,
            };
        }
    }
}

/// Consume pressure-independent engine mass flow and refresh vehicle mass data.
pub fn propulsion_consumption(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &RocketFlightConditions,
        &mut RocketPropulsion,
        Option<&AblationState>,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut rocket, geometry, conditions, mut propulsion, ablation) in rocket_query.iter_mut() {
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if let Some((active_core_stage, core_throttle)) = propulsion.running_core_stage() {
            let (_, mass_flow_kg_s) = stage_thrust_body(
                &active_core_stage.stage().engines,
                core_throttle,
                conditions.ambient_pressure_pa,
            );
            let consumption = consume_propellant(
                active_core_stage.burnable_propellant_kg(),
                mass_flow_kg_s,
                dt,
            );
            // The write-back MUST be unconditional. Previously it lived inside
            // `debug_assert!`, so release builds never drained the core stage
            // (fuel pinned at 100%, mass constant). Keep only the sanity check
            // debug-gated.
            let applied =
                propulsion.set_active_core_burnable_propellant_kg(consumption.remaining_kg);
            debug_assert!(
                applied,
                "active core stage inventory must be synchronized before consumption"
            );
        }

        if let Some((boosters, _)) = propulsion.attached_boosters() {
            let boosters = boosters.clone();
            for booster_index in 0..boosters.count() {
                if !propulsion.booster_is_ignitable(booster_index) {
                    continue;
                }
                let (_, mass_flow_kg_s) = stage_thrust_body(
                    &boosters.stage.engines,
                    throttle,
                    conditions.ambient_pressure_pa,
                );
                let inventory = propulsion
                    .attached_booster_inventory_mut()
                    .expect("attached boosters have a fixed propellant inventory");
                let remaining = inventory[booster_index];
                inventory[booster_index] =
                    consume_propellant(remaining, mass_flow_kg_s, dt).remaining_kg;
            }
        }
        rocket.refresh_attached_mass_properties(
            &propulsion,
            *geometry,
            ablation.map_or(0.0, |state| state.mass_loss_kg),
        );
    }
}

/// Add pressure-corrected gimbal torque from each running active-stage engine.
pub fn propulsion_gimbal(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketGeometry,
        &RocketFlightConditions,
        &RocketPropulsion,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, geometry, conditions, propulsion, mut torque_accum) in rocket_query.iter_mut() {
        let throttle = propulsion.throttle.clamp(0.0, 1.0);
        if let Some((active_core_stage, core_throttle)) = propulsion.running_core_stage() {
            let (_, stage_mass_flow_kg_s) = stage_thrust_body(
                &active_core_stage.stage().engines,
                core_throttle,
                conditions.ambient_pressure_pa,
            );
            let burn_fraction = burn_duration_s(
                active_core_stage.burnable_propellant_kg(),
                stage_mass_flow_kg_s,
                sim_time.fixed_timestep(),
            ) / sim_time.fixed_timestep();
            let Some(stage_origin_in_stack_m) =
                propulsion.active_stage_origin_in_stack_m(geometry.height_m)
            else {
                continue;
            };
            torque_accum.add_torque_nm(
                stage_gimbal_torque_body(
                    &active_core_stage.stage().engines,
                    stage_origin_in_stack_m,
                    rocket.dynamics.center_of_mass_m,
                    core_throttle,
                    conditions.ambient_pressure_pa,
                    propulsion.gimbal_pitch_rad as f64,
                    propulsion.gimbal_yaw_rad as f64,
                ) * burn_fraction,
            );
        }
        if let Some((boosters, _)) = propulsion.attached_boosters() {
            for booster_index in 0..boosters.count() {
                if !propulsion.booster_is_ignitable(booster_index) {
                    continue;
                }
                let (_, booster_mass_flow_kg_s) = stage_thrust_body(
                    &boosters.stage.engines,
                    throttle,
                    conditions.ambient_pressure_pa,
                );
                let burn_fraction = burn_duration_s(
                    propulsion
                        .attached_booster_inventory()
                        .expect("attached boosters have a fixed propellant inventory")
                        [booster_index],
                    booster_mass_flow_kg_s,
                    sim_time.fixed_timestep(),
                ) / sim_time.fixed_timestep();
                torque_accum.add_torque_nm(
                    stage_gimbal_torque_body(
                        &boosters.stage.engines,
                        boosters
                            .attachment_position_m(booster_index)
                            .expect("booster index is bounded by its attachment inventory")
                            .as_dvec3(),
                        rocket.dynamics.center_of_mass_m,
                        throttle,
                        conditions.ambient_pressure_pa,
                        propulsion.gimbal_pitch_rad as f64,
                        propulsion.gimbal_yaw_rad as f64,
                    ) * burn_fraction,
                );
            }
        }
    }
}
