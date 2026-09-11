//! Versioned, presentation-free recorded simulation artifacts.

use crate::domain::services::regression::RocketStateSample;
use crate::domain::services::simulation_run::SimulationRunIdentity;
use ron::de::from_str;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};

pub const SIMULATION_ARTIFACT_SCHEMA_VERSION: u32 = 2;

/// One authoritative fixed-tick engineering sample. SI units and the
/// planet-centered inertial frame are declared by the enclosing run identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationTelemetryFrame {
    pub simulation_time_s: f64,
    pub epoch_tdb_seconds_since_j2000: f64,
    pub state: RocketStateSample,
    pub active_stage: u32,
    pub propellant_remaining_kg: f64,
    pub throttle_unit: f64,
    pub mach_number: f64,
    pub dynamic_pressure_pa: f64,
    pub atmospheric_density_kg_m3: f64,
    pub terrain_altitude_m: f64,
    pub ground_contact: bool,
    /// Body-frame angle of attack, radians, from the authoritative aerodynamic
    /// telemetry calculation. Absent in v1 artifacts.
    #[serde(default)]
    pub angle_of_attack_rad: Option<f64>,
    /// Authoritative total aerodynamic/entry heat flux, W/m².
    #[serde(default)]
    pub total_heat_flux_w_m2: Option<f64>,
    /// Bound central body's validated gravitational parameter, m³/s².
    #[serde(default)]
    pub gravitational_parameter_m3_s2: Option<f64>,
}

/// Structured simulation occurrence. The stable kind is for consumers; detail
/// remains diagnostic rather than becoming a UI-string contract.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationTelemetryEvent {
    pub simulation_time_s: f64,
    pub kind: String,
    pub detail: String,
    #[serde(default)]
    pub event_type: Option<SimulationEventType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationEventType {
    StageSeparation,
    FairingSeparation,
    Splashdown,
    BlackoutStarted,
    BlackoutEnded,
}

/// A complete recorded-data artifact. Replay consumes these samples directly;
/// it does not re-run physics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationArtifact {
    pub schema_version: u32,
    pub run_identity: SimulationRunIdentity,
    pub telemetry: Vec<SimulationTelemetryFrame>,
    pub events: Vec<SimulationTelemetryEvent>,
}

impl SimulationArtifact {
    pub fn new(run_identity: SimulationRunIdentity) -> Self {
        Self {
            schema_version: SIMULATION_ARTIFACT_SCHEMA_VERSION,
            run_identity,
            telemetry: Vec::new(),
            events: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 && self.schema_version != SIMULATION_ARTIFACT_SCHEMA_VERSION {
            return Err(format!(
                "unsupported simulation artifact schema {}",
                self.schema_version
            ));
        }
        self.run_identity.validate()?;
        let mut previous_time = None;
        for frame in &self.telemetry {
            if !frame.simulation_time_s.is_finite()
                || !frame.epoch_tdb_seconds_since_j2000.is_finite()
            {
                return Err("telemetry frame time must be finite".to_string());
            }
            if previous_time.is_some_and(|time| frame.simulation_time_s < time) {
                return Err("telemetry frame times must be monotonic".to_string());
            }
            previous_time = Some(frame.simulation_time_s);
        }
        Ok(())
    }

    pub fn to_ron(&self) -> Result<String, String> {
        self.validate()?;
        to_string_pretty(self, PrettyConfig::new()).map_err(|error| error.to_string())
    }

    pub fn from_ron(text: &str) -> Result<Self, String> {
        let artifact: Self = from_str(text).map_err(|error| error.to_string())?;
        artifact.validate()?;
        Ok(artifact)
    }
}

/// Presentation-free recorded-data cursor. Its playback rate is consumer
/// policy; stepping and seeking never invoke simulation code.
#[derive(Debug, Clone)]
pub struct ReplayTimeline {
    artifact: SimulationArtifact,
    index: usize,
    paused: bool,
    playback_rate: f64,
}

impl ReplayTimeline {
    pub fn new(artifact: SimulationArtifact) -> Result<Self, String> {
        artifact.validate()?;
        Ok(Self {
            artifact,
            index: 0,
            paused: true,
            playback_rate: 1.0,
        })
    }

    pub fn start(&mut self) {
        self.paused = false;
    }
    pub fn pause(&mut self) {
        self.paused = true;
    }
    pub fn is_paused(&self) -> bool {
        self.paused
    }
    pub fn set_playback_rate(&mut self, rate: f64) -> Result<(), String> {
        if !rate.is_finite() || rate <= 0.0 {
            return Err("playback rate must be positive".to_string());
        }
        self.playback_rate = rate;
        Ok(())
    }
    pub fn playback_rate(&self) -> f64 {
        self.playback_rate
    }
    pub fn current(&self) -> Option<&SimulationTelemetryFrame> {
        self.artifact.telemetry.get(self.index)
    }
    pub fn seek(&mut self, index: usize) -> Result<(), String> {
        if index >= self.artifact.telemetry.len() {
            return Err("replay seek is outside recorded telemetry".to_string());
        }
        self.index = index;
        Ok(())
    }
    pub fn step(&mut self) -> Option<&SimulationTelemetryFrame> {
        if self.index + 1 < self.artifact.telemetry.len() {
            self.index += 1;
        }
        self.current()
    }
    pub fn artifact(&self) -> &SimulationArtifact {
        &self.artifact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn artifact() -> SimulationArtifact {
        let identity = SimulationRunIdentity {
            scenario_id: "test".into(),
            vehicle_model_id: "vehicle".into(),
            vehicle_configuration_sha256: None,
            launch_site_id: "site".into(),
            environment_model_id: "env".into(),
            terrain_source_id: "terrain".into(),
            ephemeris_authority_id: "ephemeris".into(),
            start_epoch_tdb_seconds_since_j2000: 0.0,
            state_reference_frame: "planet-inertial-meters".into(),
            numerical_integrator: "semi-implicit-euler".into(),
            fixed_timestep_s: 1.0,
            random_seed: None,
            software_revision: "test".into(),
        };
        let mut artifact = SimulationArtifact::new(identity);
        artifact.telemetry.push(SimulationTelemetryFrame {
            simulation_time_s: 0.0,
            epoch_tdb_seconds_since_j2000: 0.0,
            state: RocketStateSample::new(
                [0.0; 3],
                [0.0; 3],
                [0.0, 0.0, 0.0, 1.0],
                [0.0; 3],
                1.0,
                0,
            ),
            active_stage: 0,
            propellant_remaining_kg: 0.0,
            throttle_unit: 0.0,
            mach_number: 0.0,
            dynamic_pressure_pa: 0.0,
            atmospheric_density_kg_m3: 0.0,
            terrain_altitude_m: 0.0,
            ground_contact: true,
            angle_of_attack_rad: None,
            total_heat_flux_w_m2: None,
            gravitational_parameter_m3_s2: None,
        });
        artifact
    }
    #[test]
    fn ron_round_trip_and_replay_step() {
        let artifact = artifact();
        let decoded = SimulationArtifact::from_ron(&artifact.to_ron().unwrap()).unwrap();
        let mut replay = ReplayTimeline::new(decoded).unwrap();
        replay.start();
        assert!(!replay.is_paused());
        assert_eq!(replay.current().unwrap().state.mass_kg, 1.0);
        replay.pause();
    }
    #[test]
    fn rejects_unsupported_schema() {
        let mut artifact = artifact();
        artifact.schema_version = 3;
        assert!(artifact.validate().is_err());
    }
}
