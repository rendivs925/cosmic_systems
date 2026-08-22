//! Simulation time resource for time acceleration and fixed timestep management.
//!
//! Centralizes simulation time control (AGENTS.md section 12).
//! Physics systems should consume the simulation timestep from this resource.

use bevy::prelude::*;

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
    /// Fixed physics timestep in seconds (e.g., 1/60 = 60 Hz physics).
    pub fixed_timestep_s: f64,
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
        }
    }

    /// Get the fixed physics timestep (simulation seconds per physics step).
    pub fn fixed_timestep(&self) -> f64 {
        self.fixed_timestep_s * self.time_acceleration
    }

    /// Get the fixed physics timestep as f32.
    pub fn fixed_timestep_f32(&self) -> f32 {
        self.fixed_timestep() as f32
    }

    /// Set time acceleration factor. Valid range: 0.0 to 10000.0.
    pub fn set_time_acceleration(&mut self, factor: f64) {
        self.time_acceleration = factor.clamp(0.0, 10000.0);
    }

    /// Toggle pause state.
    pub fn toggle_pause(&mut self) {
        self.paused = !self.paused;
    }

    /// Advance simulation time by the given real delta time.
    pub fn advance(&mut self, real_dt_s: f64) {
        self.real_time_s += real_dt_s;
        if !self.paused {
            self.sim_time_s += real_dt_s * self.time_acceleration;
        }
    }

    /// Predefined time acceleration presets.
    pub const REALTIME: f64 = 1.0;
    pub const ACCEL_10X: f64 = 10.0;
    pub const ACCEL_100X: f64 = 100.0;
    pub const ACCEL_1000X: f64 = 1000.0;
    pub const ACCEL_10000X: f64 = 10000.0;
}

impl Default for SimulationTime {
    fn default() -> Self {
        Self::new(1.0 / 60.0) // 60 Hz physics
    }
}

/// System to advance SimulationTime from real time (runs in Update schedule).
pub fn advance_simulation_time(time: Res<Time>, mut sim_time: ResMut<SimulationTime>) {
    sim_time.advance(time.delta_secs_f64());
}

/// System to sync Bevy's fixed timestep with SimulationTime (runs in FixedUpdate schedule).
pub fn sync_fixed_timestep(mut fixed_time: ResMut<Time<Fixed>>, sim_time: Res<SimulationTime>) {
    fixed_time.set_timestep_hz(1.0 / sim_time.fixed_timestep());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_60hz() {
        let sim = SimulationTime::default();
        assert!((sim.fixed_timestep() - 1.0 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn time_acceleration_scales_fixed_timestep() {
        let mut sim = SimulationTime::new(1.0 / 60.0);
        sim.set_time_acceleration(10.0);
        assert!((sim.fixed_timestep() - 10.0 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn pause_stops_sim_time() {
        let mut sim = SimulationTime::default();
        sim.paused = true;
        sim.advance(1.0);
        assert_eq!(sim.sim_time_s, 0.0);
        assert_eq!(sim.real_time_s, 1.0);
    }

    #[test]
    fn clamp_time_acceleration() {
        let mut sim = SimulationTime::default();
        sim.set_time_acceleration(-5.0);
        assert_eq!(sim.time_acceleration, 0.0);
        sim.set_time_acceleration(20000.0);
        assert_eq!(sim.time_acceleration, 10000.0);
    }
}
