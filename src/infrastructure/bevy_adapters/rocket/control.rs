//! Guidance-command control and actuator-limit adapters.

use super::components::{
    EngineSelection, GridFins, RocketAutopilot, RocketCommands, RocketFlightConditions,
    RocketGeometry, RocketMissionState, RocketPhysicsState, RocketPropulsion, RocketRcs,
    TorqueAccumulator,
};
use super::events::{EngineCutoffEvent, EngineIgnitionEvent};
use crate::domain::entities::rocket::EngineState;
use crate::domain::services::actuation::{clamp_deflection, clamp_rcs_torque, limit_throttle_slew};
use crate::domain::services::control::control_torque_body;
use crate::domain::services::rocket_propulsion::{
    allocate_gimbal_deflections_at_stage_origin, clamp_throttle_range, engine_subset,
    ignition_allowed_during_ullage, stage_gimbal_torque_body, stage_throttle_envelope,
};
use crate::domain::services::simulation_time::SimulationTime;
use bevy::math::DVec3;
use bevy::prelude::{Entity, MessageWriter, Query, Res};

/// Convert guidance attitude targets into gimbal, grid-fin, and RCS commands.
#[expect(
    clippy::type_complexity,
    reason = "Control consumes the complete guidance, airframe, atmosphere, and actuator state."
)]
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
        Option<&mut GridFins>,
        Option<&EngineSelection>,
    )>,
) {
    let dt = sim_time.fixed_timestep();
    for (
        mut commands,
        rocket,
        propulsion,
        geometry,
        conditions,
        mut autopilot,
        mission,
        grid_fins,
        engine_selection,
    ) in rocket_query.iter_mut()
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
        let Some(stage) = propulsion.active_stage_configuration() else {
            continue;
        };
        let selected_count = engine_selection.map(|selection| selection.active_count);
        let selected_engines = engine_subset(&stage.engines, selected_count);
        // Trimming a nonzero zero-deflection torque is only valid for a
        // selected subset whose thrust line misses the center of mass. A full
        // stage keeps the original independent-axis inversion bit-for-bit, so
        // the existing ascent baselines are untouched.
        let trim_static_torque =
            selected_count.is_some_and(|count| (count as usize) < stage.engines.len());
        let terminal_descent = matches!(
            *mission,
            RocketMissionState::PoweredDescent | RocketMissionState::Landing
        );
        let gimbal_throttle = if terminal_descent {
            commands.throttle_cmd
        } else {
            propulsion.throttle
        };
        let Some(stage_origin_in_stack_m) =
            propulsion.active_stage_origin_in_stack_m(geometry.height_m)
        else {
            continue;
        };
        let (gimbal_pitch, gimbal_yaw) = allocate_gimbal_deflections_at_stage_origin(
            selected_engines,
            stage_origin_in_stack_m,
            rocket.dynamics.center_of_mass_m,
            torque,
            gimbal_throttle,
            conditions.ambient_pressure_pa,
            trim_static_torque,
        );
        commands.gimbal_pitch_cmd_rad =
            clamp_deflection(gimbal_pitch, autopilot.actuation.max_gimbal_deflection_rad);
        commands.gimbal_yaw_cmd_rad =
            clamp_deflection(gimbal_yaw, autopilot.actuation.max_gimbal_deflection_rad);
        // Gimbals receive the primary pitch/yaw command in every phase; RCS
        // supplies only the remaining torque (including roll) instead of
        // doubling the PID authority. The old ascent path sent the full torque
        // to both, which now costs real RCS propellant.
        let gimbal_torque = stage_gimbal_torque_body(
            selected_engines,
            stage_origin_in_stack_m,
            rocket.dynamics.center_of_mass_m,
            gimbal_throttle,
            conditions.ambient_pressure_pa,
            commands.gimbal_pitch_cmd_rad as f64,
            commands.gimbal_yaw_cmd_rad as f64,
        );
        let residual_torque = torque - gimbal_torque;
        // In the atmosphere the grid fins are the primary attitude actuator.
        // They take as much of the gimbal residual as their hinges and the
        // current dynamic pressure allow; RCS receives only what is left. In
        // vacuum the allocation is identically zero, so RCS keeps full
        // authority exactly as before.
        commands.rcs_torque_cmd_body = if let Some(mut fins) = grid_fins {
            let center_of_mass_y_m = rocket.dynamics.center_of_mass_m.y;
            let deflections = fins.spec.allocate_deflections(
                residual_torque,
                conditions.dynamic_pressure_pa,
                conditions.mach_number,
                center_of_mass_y_m,
            );
            fins.deflections_rad = deflections;
            residual_torque
                - fins.spec.torque_body_nm(
                    &deflections,
                    conditions.dynamic_pressure_pa,
                    conditions.mach_number,
                    center_of_mass_y_m,
                )
        } else {
            residual_torque
        };
    }
}

/// Apply actuator slew and mechanical limits to guidance-control commands.
#[allow(clippy::type_complexity)]
pub fn actuation_system(
    sim_time: Res<SimulationTime>,
    mut ignition_writer: MessageWriter<EngineIgnitionEvent>,
    mut cutoff_writer: MessageWriter<EngineCutoffEvent>,
    mut rocket_query: Query<(
        Entity,
        &RocketCommands,
        &mut RocketPropulsion,
        &mut TorqueAccumulator,
        &RocketAutopilot,
        &RocketMissionState,
        Option<&mut RocketRcs>,
        Option<&EngineSelection>,
    )>,
) {
    let dt = sim_time.fixed_timestep_f32();
    for (entity, commands, mut propulsion, mut torque_accum, autopilot, mission, rcs, selection) in
        rocket_query.iter_mut()
    {
        if matches!(
            *mission,
            RocketMissionState::Landed | RocketMissionState::Crashed
        ) {
            propulsion.throttle = 0.0;
            let stage_index = propulsion.active_stage as u32;
            let (started, stopped) =
                command_engine_lifecycle(&mut propulsion, false, selected_count(selection));
            write_engine_lifecycle_events(
                entity,
                stage_index,
                started,
                stopped,
                &mut ignition_writer,
                &mut cutoff_writer,
            );
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
            .map(|stage| {
                stage_throttle_envelope(engine_subset(&stage.engines, selected_count(selection)))
            })
            .unwrap_or((0.0, 1.0));
        propulsion.throttle = if commands.throttle_cmd <= 0.0 {
            slewed
        } else {
            clamp_throttle_range(slewed, envelope.0, envelope.1)
        };
        let run_commanded = commands.throttle_cmd > 0.0 && propulsion.throttle > 0.0;
        let stage_index = propulsion.active_stage as u32;
        let (started, stopped) =
            command_engine_lifecycle(&mut propulsion, run_commanded, selected_count(selection));
        write_engine_lifecycle_events(
            entity,
            stage_index,
            started,
            stopped,
            &mut ignition_writer,
            &mut cutoff_writer,
        );
        propulsion.gimbal_pitch_rad = clamp_deflection(
            commands.gimbal_pitch_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        propulsion.gimbal_yaw_rad = clamp_deflection(
            commands.gimbal_yaw_cmd_rad,
            limits.max_gimbal_deflection_rad,
        );
        let rcs_torque = if let Some(mut rcs) = rcs {
            // The thruster cluster is the single owner of the hardware torque
            // envelope. Do not pre-clamp the command with the legacy scalar
            // limit as well: `allocate` saturates at the same per-axis authority
            // and reports it, so applying both would duplicate the rule.
            let available_kg = propulsion.final_stage_propellant_kg() - rcs.pending_tank_draw_kg;
            let allocation = rcs.cluster.allocate(
                commands.rcs_torque_cmd_body,
                sim_time.fixed_timestep(),
                available_kg,
            );
            rcs.propellant_used_kg += allocation.propellant_used_kg;
            rcs.pending_tank_draw_kg += allocation.propellant_used_kg;
            // A single tick's RCS draw is well below the f32 resolution of a
            // 100 t stage tank. Drain in 0.05 kg chunks so the mass inventory
            // follows the exact f64 draw instead of rounding every tick away.
            if rcs.pending_tank_draw_kg >= 0.05 {
                let removed_kg =
                    propulsion.consume_final_stage_propellant(rcs.pending_tank_draw_kg);
                rcs.pending_tank_draw_kg = (rcs.pending_tank_draw_kg - removed_kg).max(0.0);
            }
            rcs.last_allocation = Some(allocation);
            allocation.torque_nm
        } else {
            // Minimal harnesses without a thruster set keep the validated
            // per-axis clamp; the force/torque envelope is identical.
            clamp_rcs_torque(commands.rcs_torque_cmd_body, limits.max_rcs_torque_nm)
        };
        torque_accum.add_torque_nm(rcs_torque);
    }
}

/// Actuation is the sole owner of commanded starts and cutoffs. The lifecycle
/// state then drives every downstream force, torque, and mass-flow calculation.
/// Returns `(engines_started, engines_stopped)` so the caller can publish the
/// authoritative ignition/cutoff events without re-deriving them elsewhere.
fn command_engine_lifecycle(
    propulsion: &mut RocketPropulsion,
    run_commanded: bool,
    active_count: Option<u32>,
) -> (u32, u32) {
    let ignition_permitted = ignition_allowed_during_ullage(
        propulsion.time_since_separation_s,
        propulsion.ullage_settle_time_s,
    );
    let core_has_propellant = propulsion
        .active_core_stage()
        .is_some_and(|stage| stage.has_burnable_propellant());
    let mut started = 0;
    let mut stopped = 0;
    if let Some(stage) = propulsion.vehicle.stages.get_mut(propulsion.active_stage) {
        let selected = active_count
            .map(|count| count as usize)
            .unwrap_or(stage.engines.len())
            .min(stage.engines.len());
        for (index, engine) in stage.engines.iter_mut().enumerate() {
            let was_running = engine.state == EngineState::Running;
            let engine_commanded = run_commanded && index < selected;
            if core_has_propellant {
                engine.command_lifecycle(engine_commanded, ignition_permitted);
            } else {
                engine.deplete();
            }
            count_lifecycle_transition(was_running, engine.state, &mut started, &mut stopped);
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
                let was_running = engine.state == EngineState::Running;
                if boosters_have_propellant {
                    engine.command_lifecycle(run_commanded, ignition_permitted);
                } else {
                    engine.deplete();
                }
                count_lifecycle_transition(was_running, engine.state, &mut started, &mut stopped);
            }
        }
    }
    (started, stopped)
}

fn selected_count(selection: Option<&EngineSelection>) -> Option<u32> {
    selection.map(|selection| selection.active_count)
}

fn count_lifecycle_transition(
    was_running: bool,
    state: EngineState,
    started: &mut u32,
    stopped: &mut u32,
) {
    let is_running = state == EngineState::Running;
    if !was_running && is_running {
        *started += 1;
    } else if was_running && !is_running {
        *stopped += 1;
    }
}

fn write_engine_lifecycle_events(
    rocket: Entity,
    stage_index: u32,
    started: u32,
    stopped: u32,
    ignition_writer: &mut MessageWriter<EngineIgnitionEvent>,
    cutoff_writer: &mut MessageWriter<EngineCutoffEvent>,
) {
    if started > 0 {
        ignition_writer.write(EngineIgnitionEvent {
            rocket,
            stage_index,
            engines_started: started,
        });
    }
    if stopped > 0 {
        cutoff_writer.write(EngineCutoffEvent {
            rocket,
            stage_index,
            engines_stopped: stopped,
        });
    }
}
