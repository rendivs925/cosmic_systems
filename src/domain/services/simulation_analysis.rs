//! Deterministic engineering analysis of recorded simulation artifacts.

use crate::domain::services::simulation_artifact::SimulationArtifact;
use crate::domain::services::simulation_run::SimulationRunIdentity;
use ron::de::from_str;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SIMULATION_ANALYSIS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EngineeringConstraint {
    MaximumDynamicPressurePa { limit_pa: f64 },
    MaximumMach { limit: f64 },
    MinimumTerrainClearanceM { minimum_m: f64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConstraintResult {
    pub constraint: EngineeringConstraint,
    pub observed: f64,
    pub limit: f64,
    pub margin: f64,
    pub passed: bool,
    pub time_s: f64,
}

/// Deterministic derivation, not an alternate simulation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationAnalysisResult {
    pub schema_version: u32,
    pub source_artifact_schema_version: u32,
    pub source_artifact_sha256: String,
    pub run_identity: SimulationRunIdentity,
    pub duration_s: f64,
    pub maximum_terrain_altitude_m: f64,
    pub maximum_speed_mps: f64,
    pub maximum_acceleration_mps2: Option<f64>,
    pub maximum_mach: f64,
    pub maximum_dynamic_pressure_pa: f64,
    pub minimum_terrain_clearance_m: f64,
    pub propellant_consumed_kg: f64,
    pub final_position_m: [f64; 3],
    pub final_velocity_mps: [f64; 3],
    pub final_mass_kg: f64,
    pub constraints: Vec<ConstraintResult>,
    pub warnings: Vec<String>,
}

impl SimulationAnalysisResult {
    pub fn to_ron(&self) -> Result<String, String> {
        to_string_pretty(self, PrettyConfig::new()).map_err(|error| error.to_string())
    }
    pub fn from_ron(text: &str) -> Result<Self, String> {
        let result: Self = from_str(text).map_err(|error| error.to_string())?;
        if result.schema_version != SIMULATION_ANALYSIS_SCHEMA_VERSION {
            return Err(format!(
                "unsupported simulation analysis schema {}",
                result.schema_version
            ));
        }
        result.run_identity.validate()?;
        Ok(result)
    }

    pub fn engineering_summary(&self) -> String {
        format!(
            "RUN\nscenario={} vehicle={} terrain={} ephemeris={}\n\nFLIGHT\nduration_s={:.3}\nmax_altitude_m={:.3}\nmax_speed_mps={:.3}\nmax_mach={:.3}\nmax_q_pa={:.3}\nfinal_mass_kg={:.3}\n\nCONSTRAINTS\npassed={}/{}\n\nPROVENANCE\nartifact_sha256={}\n",
            self.run_identity.scenario_id, self.run_identity.vehicle_model_id,
            self.run_identity.terrain_source_id, self.run_identity.ephemeris_authority_id,
            self.duration_s, self.maximum_terrain_altitude_m, self.maximum_speed_mps,
            self.maximum_mach, self.maximum_dynamic_pressure_pa, self.final_mass_kg,
            self.constraints.iter().filter(|result| result.passed).count(), self.constraints.len(),
            self.source_artifact_sha256,
        )
    }
}

pub fn analyze_simulation_artifact(
    artifact: &SimulationArtifact,
    constraints: &[EngineeringConstraint],
) -> Result<SimulationAnalysisResult, String> {
    artifact.validate()?;
    if artifact.run_identity.state_reference_frame != "planet-inertial-meters" {
        return Err(format!(
            "analysis does not support state reference frame '{}'",
            artifact.run_identity.state_reference_frame
        ));
    }
    let first = artifact
        .telemetry
        .first()
        .ok_or_else(|| "cannot analyze an artifact with zero telemetry frames".to_string())?;
    let last = artifact
        .telemetry
        .last()
        .expect("non-empty telemetry has a final frame");
    let mut maximum_altitude = first.terrain_altitude_m;
    let mut maximum_speed = 0.0_f64;
    let mut maximum_mach = first.mach_number;
    let mut maximum_q = first.dynamic_pressure_pa;
    let mut minimum_clearance = first.terrain_altitude_m;
    let mut maximum_acceleration = None;
    let mut max_q_time = first.simulation_time_s;
    let mut max_mach_time = first.simulation_time_s;
    let mut min_clearance_time = first.simulation_time_s;
    for (index, frame) in artifact.telemetry.iter().enumerate() {
        maximum_altitude = maximum_altitude.max(frame.terrain_altitude_m);
        let speed = frame
            .state
            .velocity_mps
            .iter()
            .map(|value| value * value)
            .sum::<f64>()
            .sqrt();
        maximum_speed = maximum_speed.max(speed);
        if frame.mach_number > maximum_mach {
            maximum_mach = frame.mach_number;
            max_mach_time = frame.simulation_time_s;
        }
        if frame.dynamic_pressure_pa > maximum_q {
            maximum_q = frame.dynamic_pressure_pa;
            max_q_time = frame.simulation_time_s;
        }
        if frame.terrain_altitude_m < minimum_clearance {
            minimum_clearance = frame.terrain_altitude_m;
            min_clearance_time = frame.simulation_time_s;
        }
        if index > 0 {
            let previous = &artifact.telemetry[index - 1];
            let dt = frame.simulation_time_s - previous.simulation_time_s;
            if dt > 0.0 {
                let acceleration = frame
                    .state
                    .velocity_mps
                    .iter()
                    .zip(previous.state.velocity_mps)
                    .map(|(current, prior)| (current - prior) / dt)
                    .map(|component| component * component)
                    .sum::<f64>()
                    .sqrt();
                maximum_acceleration =
                    Some(maximum_acceleration.unwrap_or(0.0_f64).max(acceleration));
            }
        }
    }
    let mut results = Vec::with_capacity(constraints.len());
    for constraint in constraints {
        let (observed, limit, margin, time_s) = match constraint {
            EngineeringConstraint::MaximumDynamicPressurePa { limit_pa } => {
                (maximum_q, *limit_pa, limit_pa - maximum_q, max_q_time)
            }
            EngineeringConstraint::MaximumMach { limit } => {
                (maximum_mach, *limit, limit - maximum_mach, max_mach_time)
            }
            EngineeringConstraint::MinimumTerrainClearanceM { minimum_m } => (
                minimum_clearance,
                *minimum_m,
                minimum_clearance - minimum_m,
                min_clearance_time,
            ),
        };
        results.push(ConstraintResult {
            constraint: constraint.clone(),
            observed,
            limit,
            margin,
            passed: margin >= 0.0,
            time_s,
        });
    }
    let mut warnings = Vec::new();
    if artifact.events.is_empty() {
        warnings.push(
            "no structured events were recorded; staging and ignition timing are unavailable"
                .into(),
        );
    }
    warnings.push("orbital elements are unavailable because artifact telemetry does not carry a gravitational parameter".into());
    warnings.push("angle of attack and heating are unavailable because artifact telemetry does not carry those channels".into());
    let artifact_text = artifact.to_ron()?;
    Ok(SimulationAnalysisResult {
        schema_version: SIMULATION_ANALYSIS_SCHEMA_VERSION,
        source_artifact_schema_version: artifact.schema_version,
        source_artifact_sha256: format!("{:x}", Sha256::digest(artifact_text)),
        run_identity: artifact.run_identity.clone(),
        duration_s: last.simulation_time_s - first.simulation_time_s,
        maximum_terrain_altitude_m: maximum_altitude,
        maximum_speed_mps: maximum_speed,
        maximum_acceleration_mps2: maximum_acceleration,
        maximum_mach,
        maximum_dynamic_pressure_pa: maximum_q,
        minimum_terrain_clearance_m: minimum_clearance,
        propellant_consumed_kg: (first.propellant_remaining_kg - last.propellant_remaining_kg)
            .max(0.0),
        final_position_m: last.state.position_m,
        final_velocity_mps: last.state.velocity_mps,
        final_mass_kg: last.state.mass_kg,
        constraints: results,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::regression::RocketStateSample;
    use crate::domain::services::simulation_artifact::{
        SimulationArtifact, SimulationTelemetryFrame,
    };
    use crate::domain::services::simulation_run::SimulationRunIdentity;
    fn artifact() -> SimulationArtifact {
        let identity = SimulationRunIdentity {
            scenario_id: "test".into(),
            vehicle_model_id: "vehicle".into(),
            vehicle_configuration_sha256: None,
            launch_site_id: "site".into(),
            environment_model_id: "environment".into(),
            terrain_source_id: "terrain".into(),
            ephemeris_authority_id: "ephemeris".into(),
            start_epoch_tdb_seconds_since_j2000: 0.0,
            state_reference_frame: "planet-inertial-meters".into(),
            numerical_integrator: "euler".into(),
            fixed_timestep_s: 1.0,
            random_seed: None,
            software_revision: "test".into(),
        };
        let mut artifact = SimulationArtifact::new(identity);
        for (time, velocity, q, mach, altitude, propellant) in [
            (0.0, 0.0, 0.0, 0.0, 2.0, 10.0),
            (1.0, 10.0, 50.0, 2.0, 5.0, 4.0),
        ] {
            artifact.telemetry.push(SimulationTelemetryFrame {
                simulation_time_s: time,
                epoch_tdb_seconds_since_j2000: time,
                state: RocketStateSample::new(
                    [time, 0.0, 0.0],
                    [velocity, 0.0, 0.0],
                    [0.0, 0.0, 0.0, 1.0],
                    [0.0; 3],
                    100.0,
                    1,
                ),
                active_stage: 0,
                propellant_remaining_kg: propellant,
                throttle_unit: 1.0,
                mach_number: mach,
                dynamic_pressure_pa: q,
                atmospheric_density_kg_m3: 1.0,
                terrain_altitude_m: altitude,
                ground_contact: false,
            });
        }
        artifact
    }
    #[test]
    fn derives_metrics_constraints_and_round_trips() {
        let result = analyze_simulation_artifact(
            &artifact(),
            &[
                EngineeringConstraint::MaximumDynamicPressurePa { limit_pa: 40.0 },
                EngineeringConstraint::MinimumTerrainClearanceM { minimum_m: 1.0 },
            ],
        )
        .unwrap();
        assert_eq!(result.maximum_speed_mps, 10.0);
        assert_eq!(result.propellant_consumed_kg, 6.0);
        assert!(!result.constraints[0].passed);
        assert!(result.constraints[1].passed);
        assert_eq!(
            SimulationAnalysisResult::from_ron(&result.to_ron().unwrap()).unwrap(),
            result
        );
    }
    #[test]
    fn rejects_empty_artifact() {
        assert!(analyze_simulation_artifact(
            &SimulationArtifact::new(artifact().run_identity),
            &[]
        )
        .is_err());
    }
}
