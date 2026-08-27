//! Guidance-command control and actuator-limit adapters.

use crate::components::rocket::{
    RocketCommands, RocketFlightConditions, RocketPhysicsState, RocketPropulsion, TorqueAccumulator,
};
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::rocket_propulsion::{
    allocate_gimbal_deflections, clamp_throttle_range, stage_throttle_envelope,
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
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (mut commands, rocket, propulsion, conditions, mut autopilot) in rocket_query.iter_mut() {
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
        let (gimbal_pitch, gimbal_yaw) = allocate_gimbal_deflections(
            &stage.engines,
            rocket.dynamics.center_of_mass_m,
            torque,
            propulsion.throttle,
            conditions.ambient_pressure_pa,
        );
        commands.gimbal_pitch_cmd_rad = gimbal_pitch;
        commands.gimbal_yaw_cmd_rad = gimbal_yaw;
        commands.rcs_torque_cmd_body = DVec3::new(0.0, torque.y, 0.0);
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
    )>,
) {
    let dt = sim_time.fixed_timestep_f32();
    for (commands, mut propulsion, mut torque_accum, autopilot) in rocket_query.iter_mut() {
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
