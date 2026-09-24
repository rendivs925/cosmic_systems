//! Flight-log recording: the per-vehicle ring buffer, its entry/event records,
//! and the input action that toggles or clears it.

use super::super::components::RocketMissionState;
use super::TelemetryContext;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

/// Flight log entry for replay and analysis.
#[derive(Debug, Clone)]
pub struct FlightLogEntry {
    pub time_s: f64,
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
    pub orientation: DQuat,
    pub angular_velocity_radps: DVec3,
    pub mass_kg: f64,
    pub altitude_agl_m: f64,
    pub altitude_msl_m: f64,
    pub velocity_total_mps: f64,
    pub mach_number: f64,
    pub dynamic_pressure_pa: f64,
    pub g_load: f64,
    pub total_thrust_n: f64,
    pub throttle: f32,
    pub mission_phase: RocketMissionState,
    pub active_stage: usize,
    pub propellant_fraction: f64,
    pub apoapsis_altitude_m: f64,
    pub periapsis_altitude_m: f64,
    pub convective_heat_flux_w_m2: f64,
    pub plasma_blackout: bool,
    pub drogue_deployed: bool,
    pub main_deployed: bool,
}

/// A notable flight event captured by the flight recorder.
#[derive(Debug, Clone)]
pub struct FlightEventRecord {
    pub time_s: f64,
    pub label: String,
}

/// Ring buffer flight recorder.
#[derive(Component, Debug)]
pub struct FlightRecorder {
    entries: Vec<FlightLogEntry>,
    events: Vec<FlightEventRecord>,
    max_entries: usize,
    record_interval_s: f64,
    last_record_time_s: f64,
    recording: bool,
}

impl FlightRecorder {
    pub fn new(max_entries: usize, record_interval_s: f64) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries),
            events: Vec::new(),
            max_entries,
            record_interval_s,
            last_record_time_s: 0.0,
            recording: true,
        }
    }

    pub fn entries(&self) -> &[FlightLogEntry] {
        &self.entries
    }

    /// Record a notable flight event (staging, fairing, splashdown, blackout).
    /// Capped at [`MAX_RECORDED_EVENTS`] entries.
    pub fn note_event(&mut self, time_s: f64, label: String) {
        if self.events.len() >= MAX_RECORDED_EVENTS {
            self.events.remove(0);
        }
        self.events.push(FlightEventRecord { time_s, label });
    }

    pub fn events(&self) -> &[FlightEventRecord] {
        &self.events
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    pub fn clear(&mut self) {
        self.entries.clear();
        self.events.clear();
        self.last_record_time_s = 0.0;
    }

    pub(super) fn should_record(&self, current_time: f64) -> bool {
        self.recording && current_time - self.last_record_time_s >= self.record_interval_s
    }

    pub(super) fn record(&mut self, entry: FlightLogEntry, current_time: f64) {
        if self.entries.len() >= self.max_entries {
            self.entries.remove(0);
        }
        self.entries.push(entry);
        self.last_record_time_s = current_time;
    }
}

pub const MAX_RECORDED_EVENTS: usize = 100;

/// Build a FlightLogEntry from TelemetryContext.
pub(crate) fn build_flight_log_entry<'a>(
    ctx: &TelemetryContext<'a>,
    current_time: f64,
) -> FlightLogEntry {
    let d = ctx.derived();

    FlightLogEntry {
        time_s: current_time,
        position_m: ctx.position_m,
        velocity_mps: ctx.velocity_mps,
        orientation: ctx.orientation,
        angular_velocity_radps: ctx.angular_velocity_radps,
        mass_kg: ctx.mass_kg,
        altitude_agl_m: ctx.collision.radar_altitude_m,
        altitude_msl_m: d.altitude_m,
        velocity_total_mps: d.speed,
        mach_number: d.mach,
        dynamic_pressure_pa: d.q,
        g_load: d.g_load,
        total_thrust_n: d.total_thrust_n,
        throttle: ctx.propulsion.throttle,
        mission_phase: *ctx.mission_state,
        active_stage: ctx.propulsion.active_stage,
        propellant_fraction: d.propellant_fraction,
        apoapsis_altitude_m: ctx.orbital.apoapsis_m - ctx.planet_radius_m,
        periapsis_altitude_m: ctx.orbital.periapsis_m - ctx.planet_radius_m,
        convective_heat_flux_w_m2: ctx.thermal.convective_heat_flux_w_m2,
        plasma_blackout: d.plasma_blackout,
        drogue_deployed: ctx.parachute.deployment.drogue_deployed,
        main_deployed: ctx.parachute.deployment.main_deployed,
    }
}

/// Flight recorder input actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightRecorderAction {
    ToggleRecording,
    ClearLog,
}

/// System: handle flight recorder input.
pub fn handle_flight_recorder_input_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut rocket_query: Query<&mut FlightRecorder>,
) {
    let action = if keyboard.just_pressed(KeyCode::F9) {
        Some(FlightRecorderAction::ToggleRecording)
    } else if keyboard.just_pressed(KeyCode::F10) {
        Some(FlightRecorderAction::ClearLog)
    } else {
        None
    };

    let Some(action) = action else {
        return;
    };

    for mut recorder in rocket_query.iter_mut() {
        match action {
            FlightRecorderAction::ToggleRecording => {
                let was_recording = recorder.is_recording();
                recorder.set_recording(!was_recording);
                bevy::log::info!(
                    "Flight recording {}",
                    if !was_recording { "STARTED" } else { "STOPPED" }
                );
            }
            FlightRecorderAction::ClearLog => {
                recorder.clear();
                bevy::log::info!("Flight log cleared");
            }
        }
    }
}
