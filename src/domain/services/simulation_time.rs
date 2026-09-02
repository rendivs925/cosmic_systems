//! Simulation time resource for time acceleration and fixed timestep management.
//!
//! Centralizes simulation time control (AGENTS.md section 12).
//! Physics systems should consume the simulation timestep from this resource.

use bevy::app::FixedMain;
use bevy::prelude::*;

use crate::domain::services::ephemeris::TdbEpoch;
use crate::domain::services::simulation_epoch::{
    LeapSecondTable, ScientificEpoch, SimulationEpochError, UtcDateTime,
};

/// Upper bound on authoritative physics ticks run during one render-loop pass.
/// Unprocessed time remains queued, rather than being discarded.
pub const MAX_FIXED_STEPS_PER_RENDER_FRAME: u32 = 32;

/// Simulation time resource managing real time, simulation time, and time acceleration.
///
/// This is the single authoritative source for time control. Physics systems should
/// read `fixed_timestep()` rather than using `Time::delta_secs()` directly.
#[derive(Resource, Debug, Clone)]
pub struct SimulationTime {
    /// Real time elapsed since simulation start (seconds).
    pub real_time_s: f64,
    /// Simulation time elapsed (seconds), affected by time acceleration.
    pub sim_time_s: f64,
    /// Current time acceleration factor (1.0 = real-time, 10.0 = 10x, etc.).
    pub time_acceleration: f64,
    /// Whether simulation time is paused.
    pub paused: bool,
    /// Fixed physics timestep in seconds (e.g., 1/120 = 120 Hz physics).
    pub fixed_timestep_s: f64,
    /// Simulated seconds accrued from wall time but not yet integrated. This
    /// permits a bounded fixed-step runner to catch up without losing time.
    pending_simulation_s: f64,
    /// Versioned civil/dynamical-time conversion authority and the instant at
    /// simulation time zero. Both are configured from the validated LSK.
    leap_seconds: Option<LeapSecondTable>,
    epoch_at_simulation_start: Option<ScientificEpoch>,
}

impl SimulationTime {
    /// Create a new SimulationTime with default 60 Hz fixed timestep and 1x acceleration.
    pub fn new(fixed_timestep_s: f64) -> Self {
        Self {
            real_time_s: 0.0,
            sim_time_s: 0.0,
            time_acceleration: 1.0,
            paused: false,
            fixed_timestep_s,
            pending_simulation_s: 0.0,
            leap_seconds: None,
            epoch_at_simulation_start: None,
        }
    }

    /// Get the bounded physics timestep (simulation seconds per physics step).
    ///
    /// Time acceleration changes how often Bevy runs `FixedUpdate`, not the
    /// duration integrated by one physics tick. This keeps powered-flight,
    /// contact, and propellant calculations numerically stable at warp.
    pub fn fixed_timestep(&self) -> f64 {
        self.fixed_timestep_s
    }

    /// Get the fixed physics timestep as f32.
    pub fn fixed_timestep_f32(&self) -> f32 {
        self.fixed_timestep() as f32
    }

    /// Authoritative physics frequency. Warp is represented by queued
    /// simulation time, never by shrinking the fixed physics timestep.
    pub fn fixed_update_hz(&self) -> f64 {
        1.0 / self.fixed_timestep_s
    }

    /// Configure this clock from a validated local leap-second source. The
    /// default simulation instant is J2000 TDB, preserving existing ephemeris
    /// coverage while exposing its matching civil and dynamical time scales.
    pub fn configure_scientific_epoch(
        &mut self,
        leap_seconds: LeapSecondTable,
    ) -> Result<(), SimulationEpochError> {
        let epoch_at_simulation_start = leap_seconds.epoch_from_tdb(TdbEpoch::j2000())?;
        self.leap_seconds = Some(leap_seconds);
        self.epoch_at_simulation_start = Some(epoch_at_simulation_start);
        Ok(())
    }

    /// Select a UTC simulation start through the same versioned LSK authority.
    /// The elapsed simulation clock is reset because it is measured relative to
    /// this selected physical instant.
    pub fn set_scientific_epoch_from_utc(
        &mut self,
        utc: UtcDateTime,
    ) -> Result<(), SimulationEpochError> {
        let leap_seconds = self
            .leap_seconds
            .as_ref()
            .ok_or(SimulationEpochError::UnconfiguredAuthority)?;
        self.epoch_at_simulation_start = Some(leap_seconds.epoch_from_utc(utc)?);
        self.sim_time_s = 0.0;
        Ok(())
    }

    /// The authoritative physical instant after completed fixed ticks. It has
    /// no dependency on wall-clock or render-frame time.
    pub fn scientific_epoch(&self) -> Result<ScientificEpoch, SimulationEpochError> {
        let leap_seconds = self
            .leap_seconds
            .as_ref()
            .ok_or(SimulationEpochError::UnconfiguredAuthority)?;
        let epoch_at_simulation_start = self
            .epoch_at_simulation_start
            .ok_or(SimulationEpochError::UnconfiguredAuthority)?;
        let tdb_epoch = TdbEpoch::from_seconds_since_j2000(
            epoch_at_simulation_start.tdb_epoch().seconds_since_j2000() + self.sim_time_s,
        )?;
        leap_seconds.epoch_from_tdb(tdb_epoch)
    }

    /// Shared ephemeris input derived from [`Self::scientific_epoch`].
    pub fn tdb_epoch(&self) -> Result<TdbEpoch, SimulationEpochError> {
        Ok(self.scientific_epoch()?.tdb_epoch())
    }

    /// Set time acceleration factor. Pausing is controlled separately.
    pub fn set_time_acceleration(&mut self, factor: f64) {
        self.time_acceleration = factor.clamp(TIME_ACCELERATION_MIN, Self::ACCEL_10000X);
    }

    /// Toggle pause state.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Record real time and accrue the corresponding simulation-time demand.
    /// Physics systems must advance `sim_time_s` only through completed fixed
    /// ticks, using [`Self::advance_fixed_step`].
    pub fn advance(&mut self, real_dt_s: f64) {
        self.accrue_warp(real_dt_s);
    }

    /// Record real time without advancing the authoritative simulation clock.
    pub fn advance_real_time(&mut self, real_dt_s: f64) {
        self.real_time_s += real_dt_s;
    }

    /// Accrue wall-clock time for the bounded fixed-step runner. Backlog is
    /// intentionally retained: slowing down under load is visible, but no
    /// simulation interval is silently dropped based on render FPS.
    pub fn accrue_warp(&mut self, real_dt_s: f64) {
        if !real_dt_s.is_finite() || real_dt_s <= 0.0 {
            return;
        }
        self.real_time_s += real_dt_s;
        if !self.paused {
            self.pending_simulation_s += real_dt_s * self.time_acceleration;
        }
    }

    /// Reserve at most the configured number of fixed ticks for this render
    /// pass. Remaining whole and fractional ticks stay queued for later
    /// passes, making the policy deterministic and lossless.
    pub fn take_pending_fixed_steps(&mut self) -> u32 {
        if self.paused || !self.fixed_timestep_s.is_finite() || self.fixed_timestep_s <= 0.0 {
            return 0;
        }
        // Decimal fixed steps cannot always be represented exactly. The small
        // epsilon avoids retaining an otherwise complete tick as backlog.
        let available = (self.pending_simulation_s / self.fixed_timestep_s + 1.0e-12).floor();
        let steps = available.clamp(0.0, MAX_FIXED_STEPS_PER_RENDER_FRAME as f64) as u32;
        self.pending_simulation_s -= steps as f64 * self.fixed_timestep_s;
        steps
    }

    /// Outstanding simulation work in seconds. This is telemetry for the
    /// bounded catch-up policy, not authoritative elapsed simulation time.
    pub fn pending_simulation_s(&self) -> f64 {
        self.pending_simulation_s
    }

    /// Explicitly cancel unexecuted warp demand when the flight mode changes to
    /// a real-time-only regime such as terminal descent. These queued seconds
    /// have not advanced authoritative state yet.
    pub fn cancel_pending_simulation(&mut self) {
        self.pending_simulation_s = 0.0;
    }

    /// Advance the authoritative clock after one completed bounded physics tick.
    pub fn advance_fixed_step(&mut self) {
        if !self.paused {
            self.sim_time_s += self.fixed_timestep();
        }
    }

    /// Predefined time acceleration presets.
    pub const REALTIME: f64 = 1.0;
    pub const ACCEL_10X: f64 = 10.0;
    pub const ACCEL_100X: f64 = 100.0;
    pub const ACCEL_1000X: f64 = 1000.0;
    pub const ACCEL_10000X: f64 = 10000.0;
}

/// Lower bound of the usable acceleration range (slower than real time by at
/// most 10×; below this the fixed step degenerates).
pub const TIME_ACCELERATION_MIN: f64 = 0.1;

/// Step the acceleration one decade up (+1) or down (−1), clamped to
/// [`TIME_ACCELERATION_MIN`..=`SimulationTime::ACCEL_10000X`]. Pure function;
/// the key-binding system consumes it.
pub fn stepped_time_acceleration(current: f64, direction: i32) -> f64 {
    match direction.signum() {
        1 => (current * 10.0).clamp(TIME_ACCELERATION_MIN, SimulationTime::ACCEL_10000X),
        -1 => (current / 10.0).clamp(TIME_ACCELERATION_MIN, SimulationTime::ACCEL_10000X),
        _ => current.clamp(TIME_ACCELERATION_MIN, SimulationTime::ACCEL_10000X),
    }
}

/// Time-acceleration keys for rocket mode (Phase 15): `.` speeds up a decade,
/// `,` slows down a decade, `0` resets to real time. Centralized here —
/// physics systems keep consuming the bounded fixed timestep unchanged.
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
        bevy::log::info!("Time acceleration set to ×{}", target);
    }
    sim_time.set_time_acceleration(target);
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new(1.0 / 120.0) // 120 Hz physics
    }
}

/// System to record wall-clock time from the render schedule.
pub fn advance_real_time(time: Res<Time>, mut sim_time: ResMut<SimulationTime>) {
    sim_time.advance_real_time(time.delta_secs_f64());
}

/// Accrue warp demand after Bevy updates [`Time<Real>`]. This system is meant
/// for `First.after(TimeSystems)` when the bounded runner is installed.
pub fn accrue_time_warp(time: Res<Time<Real>>, mut sim_time: ResMut<SimulationTime>) {
    sim_time.accrue_warp(time.delta_secs_f64());
}

/// System to advance simulation time only after a bounded physics tick.
pub fn advance_fixed_simulation_time(mut sim_time: ResMut<SimulationTime>) {
    sim_time.advance_fixed_step();
}

/// Configure Bevy's fixed timestep. The bounded warp runner consumes queued
/// simulation time; changing this timer's frequency for warp would reintroduce
/// unbounded fixed-loop work.
pub fn sync_fixed_timestep(mut fixed_time: ResMut<Time<Fixed>>, sim_time: Res<SimulationTime>) {
    fixed_time.set_timestep_hz(sim_time.fixed_update_hz());
}

/// Replacement for Bevy's default fixed-loop runner when time warp is enabled.
///
/// Bevy's standard runner exhausts all accumulated fixed time in one render
/// pass. At high warp that can run millions of ticks and stall presentation.
/// This runner executes a deterministic bounded batch and leaves all remaining
/// demand in [`SimulationTime`]. See `docs/required_plugin_schedule_edits.md`
/// for the required schedule registration.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_120hz() {
        let sim = SimulationTime::default();
        assert!((sim.fixed_timestep() - 1.0 / 120.0).abs() < 1e-9);
    }

    #[test]
    fn time_acceleration_keeps_physics_frequency_bounded() {
        let mut sim = SimulationTime::new(1.0 / 60.0);
        sim.set_time_acceleration(10.0);
        assert!((sim.fixed_timestep() - 1.0 / 60.0).abs() < 1e-9);
        assert!((sim.fixed_update_hz() - 60.0).abs() < 1e-9);
    }

    #[test]
    fn pause_stops_sim_time() {
        let mut sim = SimulationTime {
            paused: true,
            ..Default::default()
        };
        sim.advance(1.0);
        assert_eq!(sim.sim_time_s, 0.0);
        assert_eq!(sim.real_time_s, 1.0);
    }

    #[test]
    fn fixed_steps_advance_simulation_time_independently_of_warp() {
        let mut sim = SimulationTime::new(0.25);
        sim.set_time_acceleration(100.0);
        sim.advance_real_time(0.0025);
        sim.advance_fixed_step();

        assert_eq!(sim.real_time_s, 0.0025);
        assert_eq!(sim.sim_time_s, 0.25);
    }

    #[test]
    fn completed_fixed_time_maps_to_tdb_epoch() {
        let mut sim = SimulationTime::new(60.0);
        sim.configure_scientific_epoch(
            LeapSecondTable::parse_lsk(include_str!(
                "../../../assets/large_files/kernels/de440/naif0012.tls"
            ))
            .unwrap(),
        )
        .unwrap();
        sim.advance_fixed_step();

        let epoch = sim.tdb_epoch().unwrap();
        assert!((epoch.seconds_since_j2000() - 60.0).abs() < 1e-5);
        assert!((epoch.julian_date() - (2_451_545.0 + 60.0 / 86_400.0)).abs() < 1e-12);
    }

    #[test]
    fn selected_utc_start_advances_only_after_completed_fixed_ticks() {
        let mut sim = SimulationTime::new(60.0);
        sim.configure_scientific_epoch(
            LeapSecondTable::parse_lsk(include_str!(
                "../../../assets/large_files/kernels/de440/naif0012.tls"
            ))
            .unwrap(),
        )
        .unwrap();
        sim.set_scientific_epoch_from_utc(UtcDateTime {
            year: 2017,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0.0,
        })
        .unwrap();

        let initial = sim.scientific_epoch().unwrap();
        sim.accrue_warp(60.0);
        assert_eq!(sim.scientific_epoch().unwrap(), initial);
        sim.advance_fixed_step();
        assert!(
            (sim.scientific_epoch()
                .unwrap()
                .tdb_epoch()
                .seconds_since_j2000()
                - initial.tdb_epoch().seconds_since_j2000()
                - 60.0)
                .abs()
                < 1.0e-5
        );
    }

    #[test]
    fn high_warp_backlog_is_bounded_without_discarding_simulation_time() {
        let mut sim = SimulationTime::new(0.1);
        sim.set_time_acceleration(100.0);
        sim.accrue_warp(0.1);

        assert_eq!(
            sim.take_pending_fixed_steps(),
            MAX_FIXED_STEPS_PER_RENDER_FRAME
        );
        assert!((sim.pending_simulation_s() - 6.8).abs() < 1e-9);

        let mut integrated_s = MAX_FIXED_STEPS_PER_RENDER_FRAME as f64 * sim.fixed_timestep();
        while sim.pending_simulation_s() >= sim.fixed_timestep() {
            let steps = sim.take_pending_fixed_steps();
            assert!(steps > 0);
            integrated_s += steps as f64 * sim.fixed_timestep();
        }
        assert!((integrated_s - 10.0).abs() < 1e-9);
        assert!(sim.pending_simulation_s() < sim.fixed_timestep());
    }

    #[test]
    fn paused_warp_does_not_create_catch_up_work() {
        let mut sim = SimulationTime::new(0.1);
        sim.paused = true;
        sim.accrue_warp(1.0);
        assert_eq!(sim.pending_simulation_s(), 0.0);
        assert_eq!(sim.take_pending_fixed_steps(), 0);
    }

    #[test]
    fn cancelling_unexecuted_warp_preserves_completed_simulation_time() {
        let mut sim = SimulationTime::new(0.1);
        sim.accrue_warp(10.0);
        sim.advance_fixed_step();
        sim.cancel_pending_simulation();

        assert_eq!(sim.sim_time_s, 0.1);
        assert_eq!(sim.pending_simulation_s(), 0.0);
    }

    #[test]
    fn clamp_time_acceleration() {
        let mut sim = SimulationTime::default();
        sim.set_time_acceleration(-5.0);
        assert_eq!(sim.time_acceleration, TIME_ACCELERATION_MIN);
        sim.set_time_acceleration(20000.0);
        assert_eq!(sim.time_acceleration, 10000.0);
    }

    #[test]
    fn stepped_acceleration_stays_within_usable_range() {
        let up = stepped_time_acceleration(SimulationTime::REALTIME, 1);
        assert_eq!(up, SimulationTime::ACCEL_10X);
        // Decade down from real time stops at the 0.1 floor.
        let down = stepped_time_acceleration(SimulationTime::REALTIME, -1);
        assert_eq!(down, TIME_ACCELERATION_MIN);
        // Ceiling: cannot exceed 10000×.
        let maxed = stepped_time_acceleration(SimulationTime::ACCEL_10000X, 1);
        assert_eq!(maxed, SimulationTime::ACCEL_10000X);
        // Direction 0 clamps the current value without stepping.
        assert_eq!(stepped_time_acceleration(0.05, 0), TIME_ACCELERATION_MIN);
    }
}
