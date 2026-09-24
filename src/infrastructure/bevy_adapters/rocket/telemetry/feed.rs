//! HUD flight-event feed: the shared resource and the system that consumes
//! rocket domain events into it and each vehicle's flight recorder.

use super::super::events::{
    CommsBlackoutEvent, CrashEvent, EngineCutoffEvent, EngineIgnitionEvent, FairingSeparatedEvent,
    LiftoffEvent, MissionPhaseChangedEvent, SplashdownDetectedEvent, StageSeparatedEvent,
    TouchdownEvent,
};
use super::recorder::FlightRecorder;
use crate::domain::services::simulation_time::SimulationTime;
use bevy::prelude::*;

/// How long the latest event stays visible on the HUD (s).
pub const EVENT_FEED_VISIBLE_S: f32 = 5.0;

/// Maximum retained timeline entries for the HUD event panel.
pub const EVENT_FEED_HISTORY: usize = 10;

/// One timestamped entry in the HUD event timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct EventFeedLine {
    pub sim_time_s: f64,
    pub label: String,
}

/// Latest notable flight event plus a bounded newest-first history for HUD
/// display (empty latest = none recent).
#[derive(Resource, Debug, Clone, Default)]
pub struct RocketEventFeed {
    pub latest: String,
    pub visible_for_s: f32,
    pub history: Vec<EventFeedLine>,
}

impl RocketEventFeed {
    pub(super) fn push_at(&mut self, sim_time_s: f64, label: String) {
        self.latest = label.clone();
        self.visible_for_s = EVENT_FEED_VISIBLE_S;
        self.history.insert(0, EventFeedLine { sim_time_s, label });
        self.history.truncate(EVENT_FEED_HISTORY);
    }

    pub(super) fn tick(&mut self, dt: f32) {
        if self.visible_for_s > 0.0 {
            self.visible_for_s -= dt;
            if self.visible_for_s <= 0.0 {
                self.latest.clear();
            }
        }
    }
}

/// Consume rocket domain events (staging, fairing, splashdown, blackout):
/// update the HUD feed and append entries to each vehicle's flight recorder.
/// Runs in Update; physics systems are untouched (AGENTS.md section 29).
#[allow(clippy::too_many_arguments)]
pub fn rocket_event_feed_system(
    time: Res<Time>,
    sim_time: Res<SimulationTime>,
    mut staging_reader: MessageReader<StageSeparatedEvent>,
    mut fairing_reader: MessageReader<FairingSeparatedEvent>,
    mut splashdown_reader: MessageReader<SplashdownDetectedEvent>,
    mut blackout_reader: MessageReader<CommsBlackoutEvent>,
    mut ignition_reader: MessageReader<EngineIgnitionEvent>,
    mut cutoff_reader: MessageReader<EngineCutoffEvent>,
    mut liftoff_reader: MessageReader<LiftoffEvent>,
    mut touchdown_reader: MessageReader<TouchdownEvent>,
    mut crash_reader: MessageReader<CrashEvent>,
    mut mission_phase_reader: MessageReader<MissionPhaseChangedEvent>,
    mut feed: ResMut<RocketEventFeed>,
    mut recorders: Query<&mut FlightRecorder>,
) {
    let now = sim_time.sim_time_s;

    for event in staging_reader.read() {
        let label = format!("STAGE SEPARATED (-{:.0} kg)", event.shed_mass_kg);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in fairing_reader.read() {
        let label = format!("FAIRING JETTISONED (-{:.0} kg)", event.fairing_mass_kg);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in splashdown_reader.read() {
        let label = "SPLASHDOWN".to_string();
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in blackout_reader.read() {
        let label = if event.blackout_active {
            "COMMS BLACKOUT STARTED".to_string()
        } else {
            "COMMS REACQUIRED".to_string()
        };
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in ignition_reader.read() {
        let label = format!("STAGE {} IGNITION", event.stage_index + 1);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in cutoff_reader.read() {
        let label = format!("STAGE {} CUTOFF", event.stage_index + 1);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in liftoff_reader.read() {
        let label = "LIFTOFF".to_string();
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in touchdown_reader.read() {
        let label = format!("TOUCHDOWN ({:.1} m/s)", event.vertical_speed_mps);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in crash_reader.read() {
        let label = format!("CRASH ({:.1} m/s)", event.vertical_speed_mps);
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }
    for event in mission_phase_reader.read() {
        let label = format!("PHASE {:?}", event.current).to_uppercase();
        feed.push_at(now, label.clone());
        if let Ok(mut recorder) = recorders.get_mut(event.rocket) {
            recorder.note_event(now, label);
        }
    }

    feed.tick(time.delta_secs());
}
