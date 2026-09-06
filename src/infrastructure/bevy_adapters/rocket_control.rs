//! Guidance-command control and actuator-limit adapters.

use crate::components::rocket::{
    RocketCommands, RocketFlightConditions, RocketGeometry, RocketMissionState, RocketPhysicsState,
    RocketPropulsion, TorqueAccumulator,
};
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::rocket_propulsion::{
    allocate_gimbal_deflections_at_stage_origin, clamp_throttle_range,
    ignition_allowed_during_ullage, stage_gimbal_torque_body, stage_throttle_envelope,
};
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::components::RocketAutopilot;
use bevy::math::DVec3;
use bevy::prelude::{Query, Res};

/// Convert guidance attitude targets into gimbal and RCS commands.
pub fn control_system(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &mut RocketCommands,
        &RocketPhysicsState,
        &RocketPropulsion,
        &RocketGeometry,
        &RocketFlightConditions,
        &mut RocketAutopilot,
        &RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut commands, rocket, propulsion, geometry, conditions, mut autopilot, mission) in
        rocket_query.iter_mut()
    {
        let gains = autopilot.gains;
        let inertia_diag = DVec3::new(
            rocket.dynamics.inertia_body.x_axis.x,
            rocket.dynamics.inertia_body.y_axis.y,
            rocket.dynamics.inertia_body.z_axis.z,
        );
        let torque = control_torque_body(
            commands.target_attitude,
            rocket.dynamics.orientation,
            rocket.dynamics.angular_velocity_radps,
            inertia_diag,
            &gains,
            &mut autopilot.integral,
            dt,
        );
        let Some(stage) = propulsion.vehicle.stages.get(propulsion.active_stage) else {
            continue;
        };
        let terminal_descent = matches!(
            *mission,
            RocketMissionState::PoweredDescent | RocketMissionState::Landing
        );
        let gimbal_throttle = if terminal_descent {
            commands.throttle_cmd
        } else {
            propulsion.throttle
        };
        let attached_stages = &propulsion.vehicle.stages[propulsion.active_stage..];
        let stage_origin_in_stack_m =
            Rocket::stage_origin_in_stack_m(attached_stages, geometry.height_m, 0)
                .expect("active stage was checked above")
                .as_dvec3();
        let (gimbal_pitch, gimbal_yaw) = allocate_gimbal_deflections_at_stage_origin(
            &stage.engines,
            stage_origin_in_stack_m,
            rocket.dynamics.center_of_mass_m,
            torque,
            gimbal_throttle,
            conditions.ambient_pressure_pa,
        );
        commands.gimbal_pitch_cmd_rad = if terminal_descent {
            clamp_deflection(gimbal_pitch, autopilot.actuation.max_gimbal_deflection_rad)
        } else {
            gimbal_pitch
        };
        commands.gimbal_yaw_cmd_rad = if terminal_descent {
            clamp_deflection(gimbal_yaw, autopilot.actuation.max_gimbal_deflection_rad)
        } else {
            gimbal_yaw
        };
        if !terminal_descent {
            // Preserve ascent and coast allocation: RCS damps all axes unless
            // terminal descent has main-engine authority to subtract.
            commands.rcs_torque_cmd_body = torque;
            continue;
        }
        let gimbal_torque = stage_gimbal_torque_body(
            &stage.engines,
            stage_origin_in_stack_m,
            rocket.dynamics.center_of_mass_m,
            gimbal_throttle,
            conditions.ambient_pressure_pa,
            commands.gimbal_pitch_cmd_rad as f64,
            commands.gimbal_yaw_cmd_rad as f64,
        );
        // Gimbals receive the primary pitch/yaw command; RCS only supplies the
        // remaining torque (including roll) instead of doubling PID authority.
        commands.rcs_torque_cmd_body = torque - gimbal_torque;
    }
}

/// Apply actuator slew and mechanical limits to guidance-control commands.
pub fn actuation_system(
    sim_time: Res<SimulationTime>,
    mut rocket_query: Query<(
        &RocketCommands,
        &mut RocketPropulsion,
        &mut TorqueAccumulator,
        &RocketAutopilot,
        &RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep_f32();
    for (commands, mut propulsion, mut torque_accum, autopilot, mission) in rocket_query.iter_mut()
    {
        if matches!(
            *mission,
            RocketMissionState::Landed | RocketMissionState::Crashed
        ) {
            propulsion.throttle = 0.0;
            command_engine_lifecycle(&mut propulsion, false);
            propulsion.gimbal_pitch_rad = 0.0;
            propulsion.gimbal_yaw_rad = 0.0;
            continue;
        }
        let limits = autopilot.actuation;
        let slewed = limit_throttle_slew(
            propulsion.throttle,
            commands.throttle_cmd,
            limits.max_throttle_slew_per_s,
            dt,
        );
        let envelope = propulsion
            .vehicle
            .stages
            .get(propulsion.active_stage)
            .map(|stage| stage_throttle_envelope(&stage.engines))
            .unwrap_or((0.0, 1.0));
        propulsion.throttle = if commands.throttle_cmd <= 0.0 {
            slewed
        } else {
            clamp_throttle_range(slewed, envelope.0, envelope.1)
        };
        let run_commanded = commands.throttle_cmd > 0.0 && propulsion.throttle > 0.0;
        command_engine_lifecycle(&mut propulsion, run_commanded);
        propulsion.gimbal_pitch_rad = clamp_deflection(
            commands.gimbal_pitch_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        propulsion.gimbal_yaw_rad = clamp_deflection(
            commands.gimbal_yaw_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        torque_accum.0 += clamp_rcs_torque(commands.rcs_torque_cmd_body, limits.max_rcs_torque_nm);
    }
}

/// Actuation is the sole owner of commanded starts and cutoffs. The lifecycle
/// state then drives every downstream force, torque, and mass-flow calculation.
fn command_engine_lifecycle(propulsion: &mut RocketPropulsion, run_commanded: bool) {
    let ignition_permitted = ignition_allowed_during_ullage(
        propulsion.time_since_separation_s,
        propulsion.ullage_settle_time_s,
    );
    let core_has_propellant = propulsion
        .active_core_stage()
        .is_some_and(|stage| stage.has_burnable_propellant());
    if let Some(stage) = propulsion.vehicle.stages.get_mut(propulsion.active_stage) {
        for engine in &mut stage.engines {
            if core_has_propellant {
                engine.command_lifecycle(run_commanded, ignition_permitted);
            } else {
                engine.deplete();
            }
        }
    }
    if propulsion.boosters_attached() {
        let boosters_have_propellant = propulsion
            .attached_booster_inventory()
            .expect("attached boosters have a fixed propellant inventory")
            .iter()
            .any(|remaining_kg| *remaining_kg > 0.0);
        if let Some(boosters) = &mut propulsion.vehicle.parallel_boosters {
            for engine in &mut boosters.stage.engines {
                if boosters_have_propellant {
                    engine.command_lifecycle(run_commanded, ignition_permitted);
                } else {
                    engine.deplete();
                }
            }
        }
    }
}
