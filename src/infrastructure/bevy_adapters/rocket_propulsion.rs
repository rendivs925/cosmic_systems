//! Propulsion force, mass-flow, and gimbal-torque adapters.

use crate::application::rocket_spawning::{build_rocket_mesh, build_serial_stage_mesh};
use crate::components::rocket::*;
use crate::domain::entities::rocket::Rocket;
use crate::domain::events::StageSeparatedEvent;
use crate::domain::services::guidance::AutopilotMode;
use crate::domain::services::landing_gear::LandingGear;
use crate::domain::services::rocket_propulsion::{
    burn_duration_s, consume_propellant, separate_parallel_boosters_dynamics,
    separate_stage_dynamics, shed_stage, stage_gimbal_torque_body, stage_gimbaled_thrust_body,
    stage_mass_properties, stage_thrust_body, MIN_SEPARATION_CLEARANCE_M,
    PARALLEL_BOOSTER_SEPARATION_DV_MPS, SEPARATION_UPPER_DV_MPS, SPENT_STAGE_RETRO_DV_MPS,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::rocket_separation::{spawn_spent_stage, SpentStageSpec};
use bevy::log::info;
use bevy::math::DVec3;
use bevy::prelude::{
    Assets, Commands, Entity, Mesh, Mesh3d, MessageWriter, Query, Res, ResMut, StandardMaterial,
};

fn booster_is_ignitable(propulsion: &RocketPropulsion, booster_index: usize) -> bool {
    propulsion
        .attached_boosters()
        .is_some_and(|(boosters, inventory)| {
            propulsion.throttle > 0.0
                && inventory.get(booster_index).copied().unwrap_or(0.0) > 0.0
                && boosters.stage.engines.iter().any(|engine| {
                    engine.state == crate::domain::entities::rocket::EngineState::Running
                })
        })
}

fn refresh_attached_mass_properties(
    rocket: &mut RocketPhysicsState,
    geometry: &RocketGeometry,
    propulsion: &RocketPropulsion,
    ablation_mass_loss_kg: f64,
) {
    let mass_properties = propulsion.mass_properties(*geometry, ablation_mass_loss_kg);
    rocket.dynamics.mass_kg = mass_properties.mass_kg;
    rocket.dynamics.inertia_body = mass_properties.inertia_body;
    rocket.dynamics.center_of_mass_m = mass_properties.center_of_mass_m;
}

/// Separate an empty stage and refresh the surviving vehicle mass.
/// The spent stage receives the domain separation impulse and is spawned as
/// independent debris; the upper stage must settle propellant before restart.
#[expect(
    clippy::type_complexity,
    reason = "The staging query combines cohesive rocket state required for one transition."
)]
pub fn propulsion_staging(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sim_time: Res<SimulationTime>,
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
                if let Some(rocket_mesh) = rocket_mesh.as_deref_mut() {
                    *rocket_mesh = Mesh3d(build_rocket_mesh(&mut meshes, &serial_stack));
                }
                refresh_attached_mass_properties(
                    &mut rocket,
                    &geometry,
                    &propulsion,
                    ablation.map_or(0.0, |state| state.mass_loss_kg),
                );
                for dynamics in booster_dynamics {
                    let spent_entity = spawn_spent_stage(
                        &mut commands,
                        &mut meshes,
                        &mut materials,
                        SpentStageSpec {
                            parent_rocket: entity,
                            planet_id: binding.planet_name.clone(),
                            dynamics,
                            radius_m: boosters.stage.diameter_m * 0.5,
                            height_m: boosters.stage.height_m,
                            // Parallel boosters never inherit core stage gear.
                            landing_gear: None,
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
        if let Some(rocket_mesh) = rocket_mesh.as_deref_mut() {
            let attached_upper_envelope_height_m = (propulsion.vehicle.height_m
                - propulsion.vehicle.stages[..=propulsion.active_stage]
                    .iter()
                    .map(|stage| stage.height_m)
                    .sum::<f32>())
            .max(0.0);
            *rocket_mesh = Mesh3d(build_serial_stage_mesh(
                &mut meshes,
                active_stage,
                attached_upper_envelope_height_m,
            ));
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
            &mut meshes,
            &mut materials,
            SpentStageSpec {
                parent_rocket: entity,
                planet_id: binding.planet_name.clone(),
                dynamics: spent_dynamics,
                radius_m: separated_stage.diameter_m * 0.5,
                height_m: separated_stage.height_m,
                // Only an independently recoverable serial stage receives
                // its own gear assembly and child presentation meshes.
                landing_gear: recovery_reserve_kg.and(separated_stage.landing_gear),
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
            recovery_autopilot.mode = AutopilotMode::Boostback;
            recovery_autopilot.integral = DVec3::ZERO;
            recovery_autopilot.time_since_liftoff_s = 0.0;
            let recovery_vehicle = Rocket {
                name: format!("{} recovery", recovery_stage.name),
                diameter_m: separated_stage.diameter_m,
                height_m: separated_stage.height_m,
                stages: vec![recovery_stage],
                parallel_boosters: None,
            };
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
                recovery_autopilot,
                AerodynamicForces::default(),
                MaxQTracker::default(),
                TerrainCollisionState::default(),
                GroundRest { active: false },
                TipOverState::default(),
                LandingScorecard::default(),
                OrbitalElements::default(),
            ));
            commands.entity(spent_entity).insert((
                ThermalState::default(),
                AblationState::default(),
                ParachuteState::default(),
                CommsState::default(),
                RetroPropulsionEffect::default(),
            ));
        }
        propulsion.time_since_separation_s = 0.0;

        separated_writer.write(StageSeparatedEvent {
            rocket: entity,
            spent_stage: spent_entity,
            shed_mass_kg: shed,
        });
        info!(
            "Stage {} separated: shed {shed:.0} kg, upper stage {:.0} kg",
            propulsion.active_stage - 1,
            new_mass
        );
    }
}

/// Add pressure-corrected engine thrust in the inertial frame.
pub fn propulsion_thrust(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &RocketPropulsion,
        &RetroPropulsionEffect,
        &mut ForceAccumulator,
    )>,
) {
    for (rocket, conditions, propulsion, retro, mut force_accum) in rocket_query.iter_mut() {
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
            force_accum.0 +=
                rocket.dynamics.orientation * thrust_body * retro.thrust_multiplier * burn_fraction;
        }
        if let Some((boosters, _)) = propulsion.attached_boosters() {
            for booster_index in 0..boosters.count() {
                if !booster_is_ignitable(propulsion, booster_index) {
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
                force_accum.0 += rocket.dynamics.orientation
                    * booster_thrust_body
                    * retro.thrust_multiplier
                    * burn_fraction;
            }
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
            debug_assert!(
                propulsion.set_active_core_burnable_propellant_kg(consumption.remaining_kg)
            );
        }

        if let Some((boosters, _)) = propulsion.attached_boosters() {
            let boosters = boosters.clone();
            for booster_index in 0..boosters.count() {
                if !booster_is_ignitable(&propulsion, booster_index) {
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
        refresh_attached_mass_properties(
            &mut rocket,
            geometry,
            &propulsion,
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
            let attached_stages = &propulsion.vehicle.stages[propulsion.active_stage..];
            let stage_origin_in_stack_m =
                Rocket::stage_origin_in_stack_m(attached_stages, geometry.height_m, 0)
                    .expect("active stage was checked above")
                    .as_dvec3();
            torque_accum.0 += stage_gimbal_torque_body(
                &active_core_stage.stage().engines,
                stage_origin_in_stack_m,
                rocket.dynamics.center_of_mass_m,
                core_throttle,
                conditions.ambient_pressure_pa,
                propulsion.gimbal_pitch_rad as f64,
                propulsion.gimbal_yaw_rad as f64,
            ) * burn_fraction;
        }
        if let Some((boosters, _)) = propulsion.attached_boosters() {
            for booster_index in 0..boosters.count() {
                if !booster_is_ignitable(propulsion, booster_index) {
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
                torque_accum.0 += stage_gimbal_torque_body(
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
                ) * burn_fraction;
            }
        }
    }
}
