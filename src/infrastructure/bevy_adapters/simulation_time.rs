//! Bevy scheduling and input adapters for the authoritative domain clock.

use crate::domain::services::simulation_time::{stepped_time_acceleration, SimulationTime};
use bevy::app::FixedMain;
use bevy::prelude::*;

impl Resource for SimulationTime {}

pub fn handle_time_acceleration_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut sim_time: ResMut<SimulationTime>,
) {
    let target = if keyboard.just_pressed(KeyCode::Period) {
        Some(stepped_time_acceleration(sim_time.time_acceleration, 1))
    } else if keyboard.just_pressed(KeyCode::Comma) {
        Some(stepped_time_acceleration(sim_time.time_acceleration, -1))
    } else if keyboard.just_pressed(KeyCode::Digit0) {
        Some(SimulationTime::REALTIME)
    } else {
        None
    };
    let Some(target) = target else { return };
    if (target - sim_time.time_acceleration).abs() > f64::EPSILON {
        info!("Time acceleration set to ×{}", target);
    }
    sim_time.set_time_acceleration(target);
}

pub fn accrue_time_warp(time: Res<Time<Real>>, mut sim_time: ResMut<SimulationTime>) {
    sim_time.accrue_warp(time.delta_secs_f64());
}

pub fn advance_fixed_simulation_time(mut sim_time: ResMut<SimulationTime>) {
    sim_time.advance_fixed_step();
}

pub fn sync_fixed_timestep(mut fixed_time: ResMut<Time<Fixed>>, sim_time: Res<SimulationTime>) {
    fixed_time.set_timestep_hz(sim_time.fixed_update_hz());
}

/// Run a deterministic bounded fixed batch, retaining unprocessed warp demand.
pub fn run_bounded_fixed_main_schedule(world: &mut World) {
    let steps = world
        .resource_mut::<SimulationTime>()
        .take_pending_fixed_steps();
    if steps == 0 {
        *world.resource_mut::<Time>() = world.resource::<Time<Virtual>>().as_generic();
        return;
    }
    let _ = world.try_schedule_scope(FixedMain, |world, schedule| {
        for _ in 0..steps {
            let timestep = world.resource::<Time<Fixed>>().timestep();
            world.resource_mut::<Time<Fixed>>().advance_by(timestep);
            *world.resource_mut::<Time>() = world.resource::<Time<Fixed>>().as_generic();
            schedule.run(world);
        }
    });
    *world.resource_mut::<Time>() = world.resource::<Time<Virtual>>().as_generic();
}
