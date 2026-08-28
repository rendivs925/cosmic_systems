//! Guidance-command control and actuator-limit adapters.

use crate::components::rocket::{
    RocketCommands, RocketFlightConditions, RocketMissionState, RocketPhysicsState,
    RocketPropulsion, TorqueAccumulator,
};
use crate::domain::entities::rocket::EngineState;
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::rocket_propulsion::{
    allocate_gimbal_deflections, clamp_gimbal, clamp_throttle_range, engine_thrust_n,
    gimbal_torque_body, stage_throttle_envelope,
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
        &RocketFlightConditions,
        &mut RocketAutopilot,
        &RocketMissionState,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut commands, rocket, propulsion, conditions, mut autopilot, mission) in
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
        let (gimbal_pitch, gimbal_yaw) = allocate_gimbal_deflections(
            &stage.engines,
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
        let gimbal_torque = stage
            .engines
            .iter()
            .filter(|engine| engine.state == EngineState::Running)
            .fold(DVec3::ZERO, |total, engine| {
                total
                    + gimbal_torque_body(
                        engine.position_m.as_dvec3(),
                        rocket.dynamics.center_of_mass_m,
                        engine.thrust_axis.as_dvec3(),
                        engine_thrust_n(engine, gimbal_throttle, conditions.ambient_pressure_pa),
                        clamp_gimbal(commands.gimbal_pitch_cmd_rad, engine.gimbal_range_deg) as f64,
                        clamp_gimbal(commands.gimbal_yaw_cmd_rad, engine.gimbal_range_deg) as f64,
                    )
            });
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
