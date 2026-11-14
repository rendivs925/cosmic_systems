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
            actuated_throttle(
                propulsion.throttle,
                commands.throttle_cmd,
                autopilot.actuation.max_throttle_slew_per_s,
                sim_time.fixed_timestep_f32(),
                stage_throttle_envelope(selected_engines),
            )
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

/// Upcoming physical throttle, shared by terminal gimbal allocation and
/// actuation. Allocating at the requested throttle over-drives torque during a
/// throttle-down slew, precisely when the landing burn changes attitude.
fn actuated_throttle(
    current: f32,
    target: f32,
    slew_per_s: f32,
    dt_s: f32,
    envelope: (f32, f32),
) -> f32 {
    let slewed = limit_throttle_slew(current, target, slew_per_s, dt_s);
    if target <= 0.0 {
        slewed
    } else {
        clamp_throttle_range(slewed, envelope.0, envelope.1)
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
        let envelope = propulsion
            .vehicle
            .stages
            .get(propulsion.active_stage)
            .map(|stage| {
                stage_throttle_envelope(engine_subset(&stage.engines, selected_count(selection)))
            })
            .unwrap_or((0.0, 1.0));
        propulsion.throttle = actuated_throttle(
            propulsion.throttle,
            commands.throttle_cmd,
            limits.max_throttle_slew_per_s,
            dt,
            envelope,
        );
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
    // A recoverable stage deliberately holds propellant back from its ascent
    // burn. Reaching that floor is not hardware depletion: the stage is about
    // to separate and own the reserve. Shut the engines down instead of
    // marking them terminally depleted, or the recovery burn can never light.
    let (core_has_burnable, core_has_reserve) =
        propulsion
            .active_core_stage()
            .map_or((false, false), |stage| {
                (
                    stage.has_burnable_propellant(),
                    stage.recovery_reserve_kg() > 0.0,
                )
            });
    let mut started = 0;
    let mut stopped = 0;
    if let Some(stage) = propulsion.vehicle.stages.get_mut(propulsion.active_stage) {
        let selected = active_count
            .map(|count| count as usize)
            .unwrap_or(stage.engines.len())
            .min(stage.engines.len());
        for (index, engine) in stage.engines.iter_mut().enumerate() {
            let was_running = engine.state == EngineState::Running;
            let engine_commanded = run_commanded && index < selected && core_has_burnable;
            if core_has_burnable || core_has_reserve {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::services::guidance::roll_preserving_attitude_from_direction;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;
    use bevy::math::{DMat3, DQuat};
    use bevy::prelude::{App, IntoScheduleConfigs, MinimalPlugins, Update};

    #[test]
    fn terminal_gimbal_torque_uses_slewed_throttle_without_rcs() {
        for (initial_throttle, target_throttle) in [(0.8, 0.1), (0.1, 0.8)] {
            let mut app = App::new();
            app.add_plugins(MinimalPlugins);
            app.insert_resource(SimulationTime::new(1.0 / 64.0));
            app.add_message::<EngineIgnitionEvent>();
            app.add_message::<EngineCutoffEvent>();
            app.add_systems(Update, (control_system, actuation_system).chain());
            let mut propulsion = recoverable_propulsion();
            propulsion.throttle = initial_throttle;
            for engine in &mut propulsion.vehicle.stages[0].engines {
                engine.state = EngineState::Running;
            }
            let target_attitude = roll_preserving_attitude_from_direction(
                DVec3::new(0.0, 1.0, 0.001),
                DQuat::IDENTITY,
            );
            let inertia_diag = DVec3::splat(1.0e6);
            let autopilot = RocketAutopilot::default();
            let mut integral = DVec3::ZERO;
            let expected = control_torque_body(
                target_attitude,
                DQuat::IDENTITY,
                DVec3::ZERO,
                inertia_diag,
                &autopilot.gains,
                &mut integral,
                1.0 / 64.0,
            );
            let geometry = RocketGeometry {
                radius_m: 1.85,
                height_m: 70.0,
                lower_extent_y_m: -35.0,
            };
            let origin = propulsion
                .active_stage_origin_in_stack_m(geometry.height_m)
                .unwrap();
            let entity = app
                .world_mut()
                .spawn((
                    RocketCommands {
                        target_attitude,
                        throttle_cmd: target_throttle,
                        ..Default::default()
                    },
                    RocketPhysicsState {
                        dynamics: RocketDynamicsState::new(
                            DVec3::X * 6_371_100.0,
                            DVec3::ZERO,
                            DQuat::IDENTITY,
                            30_000.0,
                            DMat3::from_diagonal(inertia_diag),
                            DVec3::ZERO,
                        ),
                    },
                    propulsion,
                    geometry,
                    RocketFlightConditions::default(),
                    autopilot,
                    RocketMissionState::Landing,
                    TorqueAccumulator::default(),
                ))
                .id();
            app.update();
            let applied = app.world().get::<RocketPropulsion>(entity).unwrap();
            let pressure = app
                .world()
                .get::<RocketFlightConditions>(entity)
                .unwrap()
                .ambient_pressure_pa;
            let actual = stage_gimbal_torque_body(
                &applied.vehicle.stages[0].engines,
                origin,
                DVec3::ZERO,
                applied.throttle,
                pressure,
                f64::from(applied.gimbal_pitch_rad),
                f64::from(applied.gimbal_yaw_rad),
            );
            assert!(
                (applied.throttle - target_throttle).abs() > 0.1,
                "fixture must exercise an unfinished slew"
            );
            assert!((actual - expected).length() / expected.length() < 1e-4,
                "physical gimbal torque must match allocation during slew: {actual:?} vs {expected:?}");
        }
    }

    fn recoverable_propulsion() -> RocketPropulsion {
        let vehicle = Rocket::falcon9_test_fixture();
        RocketPropulsion::for_fresh_flight(vehicle, 0.0, 0.0)
    }

    /// A recoverable stage reaches its reserve floor while the ascent engines
    /// are still running. That is a shutdown, not terminal hardware depletion:
    /// the separated stage must be able to light its boostback/entry engines.
    #[test]
    fn reserve_floor_shuts_down_instead_of_depleting_recovery_engines() {
        let mut propulsion = recoverable_propulsion();
        let reserve_kg = propulsion.vehicle.stages[0]
            .recovery_propellant_reserve_kg
            .expect("fixture first stage is recoverable");
        propulsion.propellant_remaining_kg[0] = reserve_kg;
        for engine in &mut propulsion.vehicle.stages[0].engines {
            engine.state = EngineState::Running;
            engine.ignition_count = 1;
        }

        let (_started, stopped) = command_engine_lifecycle(&mut propulsion, true, None);
        assert!(stopped > 0, "the ascent burn must shut down at the floor");
        assert!(propulsion.vehicle.stages[0]
            .engines
            .iter()
            .all(|engine| engine.state == EngineState::Off));

        // The recovery stage clears the reserve marker and re-lights its
        // engines through the normal lifecycle path.
        propulsion.vehicle.stages[0].recovery_propellant_reserve_kg = None;
        let (started, _stopped) = command_engine_lifecycle(&mut propulsion, true, None);
        assert_eq!(
            started as usize,
            propulsion.vehicle.stages[0].engines.len(),
            "every selected engine must restart after separation"
        );
        assert!(propulsion.vehicle.stages[0]
            .engines
            .iter()
            .all(|engine| engine.state == EngineState::Running));
    }

    /// An expendable stage has no reserve to protect; an empty tank still
    /// depletes its engines terminally.
    #[test]
    fn expendable_empty_stage_still_depletes() {
        let mut propulsion = recoverable_propulsion();
        propulsion.vehicle.stages[0].recovery_propellant_reserve_kg = None;
        propulsion.propellant_remaining_kg[0] = 0.0;

        command_engine_lifecycle(&mut propulsion, true, None);
        assert!(propulsion.vehicle.stages[0]
            .engines
            .iter()
            .all(|engine| engine.state == EngineState::Depleted));
    }
}
