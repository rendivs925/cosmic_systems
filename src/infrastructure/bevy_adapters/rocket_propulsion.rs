//! Propulsion force, mass-flow, and gimbal-torque adapters.

use crate::components::rocket::*;
use crate::domain::entities::rocket::{EngineState, Rocket, RocketStage};
use crate::domain::events::StageSeparatedEvent;
use crate::domain::services::guidance::AutopilotMode;
use crate::domain::services::rocket_propulsion::{
    active_vehicle_inertia, active_vehicle_mass_with_payload, air_start_allowed, burn_duration_s,
    clamp_gimbal, consume_propellant, gimbal_torque_body, ignition_allowed_during_ullage,
    separation_impulse, shed_stage, stage_gimbaled_thrust_body, stage_thrust_body,
    EngineOperatingPoint, MIN_SEPARATION_CLEARANCE_M, SEPARATION_UPPER_DV_MPS,
    SPENT_STAGE_RETRO_DV_MPS,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::rocket_separation::{spawn_spent_stage, SpentStageSpec};
use bevy::log::{info, warn};
use bevy::math::DVec3;
use bevy::prelude::{
    Assets, Commands, Entity, Mesh, MessageWriter, Query, Res, ResMut, StandardMaterial,
};

/// Select an active stage only when its engines can produce force this tick.
/// Every propulsion writer uses this guard so force, torque, and mass flow stay
/// consistent across air-start and ullage constraints.
fn ignitable_stage(propulsion: &RocketPropulsion) -> Option<(&RocketStage, f32)> {
    let stage = propulsion.vehicle.stages.get(propulsion.active_stage)?;
    let remaining = burnable_propellant_kg(propulsion);
    let throttle = propulsion.throttle.clamp(0.0, 1.0);
    let engines_restartable = stage.engines.iter().all(|engine| engine.restartable);
    if throttle <= 0.0
        || remaining <= 0.0
        || !air_start_allowed(propulsion.separations_count > 0, engines_restartable)
        || !ignition_allowed_during_ullage(
            propulsion.time_since_separation_s,
            propulsion.ullage_settle_time_s,
        )
    {
        return None;
    }
    Some((stage, throttle))
}

fn recovery_reserve_kg(propulsion: &RocketPropulsion) -> f32 {
    propulsion
        .vehicle
        .stages
        .get(propulsion.active_stage)
        .and_then(|stage| stage.recovery_propellant_reserve_kg)
        .unwrap_or(0.0)
}

fn burnable_propellant_kg(propulsion: &RocketPropulsion) -> f32 {
    (propulsion
        .propellant_remaining_kg
        .get(propulsion.active_stage)
        .copied()
        .unwrap_or(0.0)
        - recovery_reserve_kg(propulsion))
    .max(0.0)
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
        &mut RocketPhysicsState,
        &mut RocketMass,
        &mut RocketPropulsion,
        Option<&AblationState>,
        Option<&RocketAutopilot>,
        Option<&LandingLegs>,
    )>,
) {
    let dt = sim_time.fixed_timestep() as f32;
    for (
        entity,
        binding,
        mut geometry,
        mut rocket,
        mut mass,
        mut propulsion,
        ablation,
        autopilot,
        legs,
    ) in rocket_query.iter_mut()
    {
        propulsion.time_since_separation_s += dt;

        let remaining = propulsion
            .propellant_remaining_kg
            .get(propulsion.active_stage)
            .copied()
            .unwrap_or(0.0);
        if remaining > recovery_reserve_kg(&propulsion) {
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
        let outcome = separation_impulse(
            pre_separation.velocity_mps,
            pre_separation.orientation,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
        );
        rocket.dynamics.velocity_mps = outcome.upper_velocity_mps;

        let separated_stage = &propulsion.vehicle.stages[propulsion.active_stage - 1];
        let active_stage = &propulsion.vehicle.stages[propulsion.active_stage];
        geometry.radius_m = active_stage.diameter_m * 0.5;
        geometry.height_m = active_stage.height_m;

        let ablation_mass_loss_kg = ablation.map_or(0.0, |ablation| ablation.mass_loss_kg);
        let new_mass = (active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
        ) - ablation_mass_loss_kg)
            .max(1.0);
        mass.0 = new_mass;
        rocket.dynamics.mass_kg = new_mass;
        let (inertia, center_of_mass_m) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            propulsion.active_stage,
            propulsion.attached_payload_kg,
            ablation_mass_loss_kg,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = center_of_mass_m;

        let mut spent_dynamics = pre_separation;
        spent_dynamics.velocity_mps = outcome.spent_velocity_mps;
        spent_dynamics.mass_kg = shed;
        if MIN_SEPARATION_CLEARANCE_M > separated_stage.height_m as f64 {
            warn!(
                "Separation clearance {MIN_SEPARATION_CLEARANCE_M} m exceeds stage length {} m",
                separated_stage.height_m,
            );
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
                kind: SpentStageKind::Booster,
            },
        );
        if let Some(recovery_reserve_kg) =
            propulsion.vehicle.stages[propulsion.active_stage - 1].recovery_propellant_reserve_kg
        {
            // The parent stage is now a standalone vehicle. Clear its reserve
            // marker so the recovery burns may consume the propellant that was
            // deliberately withheld from ascent.
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
            };
            commands.entity(spent_entity).insert((
                RecoveringStage,
                RocketMissionState::Landing,
                RocketPropulsion {
                    vehicle: recovery_vehicle,
                    active_stage: 0,
                    propellant_remaining_kg: vec![recovery_reserve_kg],
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
            if let Some(legs) = legs {
                commands.entity(spent_entity).insert(legs.clone());
            }
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
        let Some((stage, throttle)) = ignitable_stage(propulsion) else {
            continue;
        };
        let (thrust_body, mass_flow_kg_s) = stage_gimbaled_thrust_body(
            &stage.engines,
            throttle,
            conditions.ambient_pressure_pa,
            propulsion.gimbal_pitch_rad as f64,
            propulsion.gimbal_yaw_rad as f64,
        );
        let remaining = burnable_propellant_kg(propulsion);
        let burn_fraction = burn_duration_s(remaining, mass_flow_kg_s, sim_time.fixed_timestep())
            / sim_time.fixed_timestep();
        force_accum.0 +=
            rocket.dynamics.orientation * thrust_body * retro.thrust_multiplier * burn_fraction;
    }
}

/// Consume pressure-independent engine mass flow and refresh vehicle mass data.
#[expect(
    clippy::type_complexity,
    reason = "The propulsion query combines cohesive rocket state for fixed-step consumption."
)]
pub fn propulsion_consumption(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketPhysicsState,
        &RocketGeometry,
        &RocketFlightConditions,
        &mut RocketPropulsion,
        &mut RocketMass,
        Option<&AblationState>,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut rocket, geometry, conditions, mut propulsion, mut mass, ablation) in
        rocket_query.iter_mut()
    {
        let Some((stage, throttle)) = ignitable_stage(&propulsion) else {
            continue;
        };
        let (_, mass_flow_kg_s) =
            stage_thrust_body(&stage.engines, throttle, conditions.ambient_pressure_pa);
        let active_stage = propulsion.active_stage;
        let reserve_kg = recovery_reserve_kg(&propulsion);
        let remaining = burnable_propellant_kg(&propulsion);
        let (remaining_new, _) = consume_propellant(remaining, mass_flow_kg_s, dt);
        propulsion.propellant_remaining_kg[active_stage] = reserve_kg + remaining_new;

        let ablation_mass_loss_kg = ablation.map_or(0.0, |ablation| ablation.mass_loss_kg);
        let new_mass = (active_vehicle_mass_with_payload(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            active_stage,
            propulsion.attached_payload_kg,
        ) - ablation_mass_loss_kg)
            .max(1.0);
        mass.0 = new_mass;
        rocket.dynamics.mass_kg = new_mass;
        let (inertia, center_of_mass_m) = active_vehicle_inertia(
            &propulsion.vehicle.stages,
            &propulsion.propellant_remaining_kg,
            active_stage,
            propulsion.attached_payload_kg,
            ablation_mass_loss_kg,
            geometry.radius_m as f64,
            geometry.height_m as f64,
        );
        rocket.dynamics.inertia_body = inertia;
        rocket.dynamics.center_of_mass_m = center_of_mass_m;
    }
}

/// Add pressure-corrected gimbal torque from each running active-stage engine.
pub fn propulsion_gimbal(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketPhysicsState,
        &RocketFlightConditions,
        &RocketPropulsion,
        &mut TorqueAccumulator,
    )>,
) {
    for (rocket, conditions, propulsion, mut torque_accum) in rocket_query.iter_mut() {
        let Some((stage, throttle)) = ignitable_stage(propulsion) else {
            continue;
        };
        let (_, stage_mass_flow_kg_s) =
            stage_thrust_body(&stage.engines, throttle, conditions.ambient_pressure_pa);
        let burn_fraction = burn_duration_s(
            burnable_propellant_kg(propulsion),
            stage_mass_flow_kg_s,
            sim_time.fixed_timestep(),
        ) / sim_time.fixed_timestep();
        for engine in &stage.engines {
            if engine.state != EngineState::Running {
                continue;
            }
            let operating_point =
                EngineOperatingPoint::from_engine(engine, throttle, conditions.ambient_pressure_pa);
            torque_accum.0 += gimbal_torque_body(
                engine.position_m.as_dvec3(),
                rocket.dynamics.center_of_mass_m,
                engine.thrust_axis.as_dvec3(),
                operating_point.thrust_n,
                clamp_gimbal(propulsion.gimbal_pitch_rad, engine.gimbal_range_deg) as f64,
                clamp_gimbal(propulsion.gimbal_yaw_rad, engine.gimbal_range_deg) as f64,
            ) * burn_fraction;
        }
    }
}
