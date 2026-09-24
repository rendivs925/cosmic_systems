//! Deterministic engineering analysis of recorded simulation artifacts.

use crate::domain::math::DVec3;
use crate::domain::services::physics_orbital::{
    orbital_elements_from_state, specific_orbital_energy,
};
use crate::domain::services::simulation_artifact::{
    SimulationArtifact, SimulationEventType, SimulationTelemetryEvent,
};
use crate::domain::services::simulation_run::{
    SimulationRunIdentity, STATE_REFERENCE_FRAME_EARTH_CENTERED_INERTIAL,
};
use ron::de::from_str;
use ron::ser::{to_string_pretty, PrettyConfig};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SIMULATION_ANALYSIS_SCHEMA_VERSION: u32 = 3;

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
    #[serde(default)]
    pub maximum_angle_of_attack_rad: Option<f64>,
    #[serde(default)]
    pub maximum_heat_flux_w_m2: Option<f64>,
    #[serde(default)]
    pub maximum_applied_thrust_n: Option<f64>,
    #[serde(default)]
    pub maximum_active_engine_count: Option<u32>,
    #[serde(default)]
    pub final_specific_orbital_energy_j_kg: Option<f64>,
    #[serde(default)]
    pub final_orbital_semi_major_axis_m: Option<f64>,
    #[serde(default)]
    pub final_orbital_eccentricity: Option<f64>,
    #[serde(default)]
    pub stage_separation_count: usize,
    #[serde(default)]
    pub fairing_separation_count: usize,
    #[serde(default)]
    pub splashdown_count: usize,
    #[serde(default)]
    pub ignition_count: usize,
    #[serde(default)]
    pub cutoff_count: usize,
    #[serde(default)]
    pub liftoff_count: usize,
    #[serde(default)]
    pub touchdown_count: usize,
    #[serde(default)]
    pub crash_count: usize,
    #[serde(default)]
    pub mission_phase_change_count: usize,
    #[serde(default)]
    pub event_timeline: Vec<SimulationTelemetryEvent>,
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
        if !(1..=SIMULATION_ANALYSIS_SCHEMA_VERSION).contains(&result.schema_version) {
            return Err(format!(
                "unsupported simulation analysis schema {}",
                result.schema_version
            ));
        }
        result.run_identity.validate()?;
        Ok(result)
    }

    pub fn engineering_summary(&self) -> String {
        use std::fmt::Write;

        let mut summary = format!(
            "RUN\nscenario={} vehicle={} terrain={} ephemeris={}\n\nFLIGHT\nduration_s={:.3}\nmax_altitude_m={:.3}\nmax_speed_mps={:.3}\nmax_mach={:.3}\nmax_q_pa={:.3}\nfinal_mass_kg={:.3}\n",
            self.run_identity.scenario_id, self.run_identity.vehicle_model_id,
            self.run_identity.terrain_source_id, self.run_identity.ephemeris_authority_id,
            self.duration_s, self.maximum_terrain_altitude_m, self.maximum_speed_mps,
            self.maximum_mach, self.maximum_dynamic_pressure_pa, self.final_mass_kg,
        );
        if let Some(value) = self.maximum_angle_of_attack_rad {
            let _ = writeln!(summary, "max_abs_aoa_rad={value:.6}");
        }
        if let Some(value) = self.maximum_heat_flux_w_m2 {
            let _ = writeln!(summary, "max_heat_flux_w_m2={value:.3}");
        }
        if let Some(value) = self.maximum_applied_thrust_n {
            let _ = writeln!(summary, "max_applied_thrust_n={value:.3}");
        }
        if let Some(value) = self.maximum_active_engine_count {
            let _ = writeln!(summary, "max_active_engine_count={value}");
        }
        if let (Some(energy), Some(axis), Some(eccentricity)) = (
            self.final_specific_orbital_energy_j_kg,
            self.final_orbital_semi_major_axis_m,
            self.final_orbital_eccentricity,
        ) {
            let _ = writeln!(
                summary,
                "final_specific_orbital_energy_j_kg={energy:.3}\nfinal_orbital_semi_major_axis_m={axis:.3}\nfinal_orbital_eccentricity={eccentricity:.8}"
            );
        }
        let _ = writeln!(
            summary,
            "\nEVENTS\nstage_separations={} fairing_separations={} splashdowns={} ignitions={} cutoffs={} liftoffs={} touchdowns={} crashes={} phase_changes={} total={}",
            self.stage_separation_count,
            self.fairing_separation_count,
            self.splashdown_count,
            self.ignition_count,
            self.cutoff_count,
            self.liftoff_count,
            self.touchdown_count,
            self.crash_count,
            self.mission_phase_change_count,
            self.event_timeline.len(),
        );
        if !self.event_timeline.is_empty() {
            let _ = writeln!(summary, "\nEVENT TIMELINE");
            for event in &self.event_timeline {
                let _ = writeln!(
                    summary,
                    "t+{:.3} {} {}",
                    event.simulation_time_s, event.kind, event.detail
                );
            }
        }
        let _ = writeln!(
            summary,
            "\nCONSTRAINTS\npassed={}/{}\n\nPROVENANCE\nartifact_sha256={}",
            self.constraints
                .iter()
                .filter(|result| result.passed)
                .count(),
            self.constraints.len(),
            self.source_artifact_sha256,
        );
        summary
    }
}

pub fn analyze_simulation_artifact(
    artifact: &SimulationArtifact,
    constraints: &[EngineeringConstraint],
) -> Result<SimulationAnalysisResult, String> {
    artifact.validate()?;
    if artifact.run_identity.state_reference_frame != STATE_REFERENCE_FRAME_EARTH_CENTERED_INERTIAL
    {
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
    let mut maximum_aoa = None;
    let mut maximum_heat = None;
    let mut maximum_thrust = None;
    let mut maximum_engine_count = None;
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
        if let Some(value) = frame.angle_of_attack_rad {
            maximum_aoa = Some(maximum_aoa.unwrap_or(0.0_f64).max(value.abs()));
        }
        if let Some(value) = frame.total_heat_flux_w_m2 {
            maximum_heat = Some(maximum_heat.unwrap_or(0.0_f64).max(value));
        }
        if let Some(value) = frame.applied_thrust_n {
            maximum_thrust = Some(maximum_thrust.unwrap_or(0.0_f64).max(value));
        }
        if let Some(value) = frame.active_engine_count {
            maximum_engine_count = Some(maximum_engine_count.unwrap_or(0).max(value));
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
    let final_position_m = DVec3::from_array(last.state.position_m);
    let final_velocity_mps = DVec3::from_array(last.state.velocity_mps);
    let final_orbit = last
        .gravitational_parameter_m3_s2
        .filter(|mu| mu.is_finite() && *mu > 0.0)
        .and_then(|mu| {
            let energy = specific_orbital_energy(final_position_m, final_velocity_mps, mu)?;
            let elements = orbital_elements_from_state(final_position_m, final_velocity_mps, mu);
            (elements.semi_major_axis_m.is_finite() && elements.eccentricity.is_finite())
                .then_some((energy, elements.semi_major_axis_m, elements.eccentricity))
        });
    if final_orbit.is_none() {
        warnings.push("orbital elements are unavailable because the final artifact frame lacks a valid gravitational parameter or inertial state".into());
    }
    if maximum_aoa.is_none() {
        warnings.push(
            "angle of attack is unavailable because artifact telemetry does not carry that channel"
                .into(),
        );
    }
    if maximum_heat.is_none() {
        warnings.push(
            "heating is unavailable because artifact telemetry does not carry that channel".into(),
        );
    }
    if maximum_thrust.is_none() || maximum_engine_count.is_none() {
        warnings.push("propulsion output is unavailable because artifact telemetry does not carry applied thrust and engine-count channels".into());
    }
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
        maximum_angle_of_attack_rad: maximum_aoa,
        maximum_heat_flux_w_m2: maximum_heat,
        maximum_applied_thrust_n: maximum_thrust,
        maximum_active_engine_count: maximum_engine_count,
        final_specific_orbital_energy_j_kg: final_orbit.map(|orbit| orbit.0),
        final_orbital_semi_major_axis_m: final_orbit.map(|orbit| orbit.1),
        final_orbital_eccentricity: final_orbit.map(|orbit| orbit.2),
        stage_separation_count: count_typed(&artifact.events, SimulationEventType::StageSeparation),
        fairing_separation_count: count_typed(
            &artifact.events,
            SimulationEventType::FairingSeparation,
        ),
        splashdown_count: count_typed(&artifact.events, SimulationEventType::Splashdown),
        ignition_count: count_typed(&artifact.events, SimulationEventType::Ignition),
        cutoff_count: count_typed(&artifact.events, SimulationEventType::Cutoff),
        liftoff_count: count_typed(&artifact.events, SimulationEventType::Liftoff),
        touchdown_count: count_typed(&artifact.events, SimulationEventType::Touchdown),
        crash_count: count_typed(&artifact.events, SimulationEventType::Crash),
        mission_phase_change_count: count_typed(
            &artifact.events,
            SimulationEventType::MissionPhaseChange,
        ),
        event_timeline: artifact.events.clone(),
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

fn count_typed(events: &[SimulationTelemetryEvent], kind: SimulationEventType) -> usize {
    events
        .iter()
        .filter(|event| event.event_type.as_ref() == Some(&kind))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::regression::RocketStateSample;
    use crate::domain::services::simulation_artifact::{
        SimulationArtifact, SimulationEventType, SimulationTelemetryEvent, SimulationTelemetryFrame,
    };
    use crate::domain::services::simulation_run::{
        SimulationRunIdentity, NUMERICAL_INTEGRATOR_SEMI_IMPLICIT_EULER,
        STATE_REFERENCE_FRAME_EARTH_CENTERED_INERTIAL,
    };
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
            state_reference_frame: STATE_REFERENCE_FRAME_EARTH_CENTERED_INERTIAL.into(),
            numerical_integrator: NUMERICAL_INTEGRATOR_SEMI_IMPLICIT_EULER.into(),
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
                angle_of_attack_rad: None,
                total_heat_flux_w_m2: None,
                gravitational_parameter_m3_s2: None,
                applied_thrust_n: Some(velocity * 1_000.0),
                active_engine_count: Some(9),
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
        assert_eq!(result.maximum_applied_thrust_n, Some(10_000.0));
        assert_eq!(result.maximum_active_engine_count, Some(9));
        assert!(!result
            .warnings
            .iter()
            .any(|warning| warning.contains("propulsion output is unavailable")));
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

    #[test]
    fn derives_final_orbit_and_typed_event_counts_when_recorded() {
        let mut artifact = artifact();
        let final_frame = artifact.telemetry.last_mut().unwrap();
        final_frame.state.position_m = [7_000_000.0, 0.0, 0.0];
        final_frame.state.velocity_mps = [0.0, 7_546.053_290_107_542, 0.0];
        final_frame.gravitational_parameter_m3_s2 = Some(3.986_004_418e14);
        artifact.events = vec![
            SimulationTelemetryEvent {
                simulation_time_s: 1.0,
                kind: "stage_separation".into(),
                detail: String::new(),
                event_type: Some(SimulationEventType::StageSeparation),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 1.0,
                kind: "fairing_separation".into(),
                detail: String::new(),
                event_type: Some(SimulationEventType::FairingSeparation),
            },
        ];

        let result = analyze_simulation_artifact(&artifact, &[]).unwrap();
        assert!(result.final_specific_orbital_energy_j_kg.is_some());
        assert!((result.final_orbital_semi_major_axis_m.unwrap() - 7_000_000.0).abs() < 1.0);
        assert!(result.final_orbital_eccentricity.unwrap() < 1e-6);
        assert_eq!(result.stage_separation_count, 1);
        assert_eq!(result.fairing_separation_count, 1);
        assert_eq!(result.splashdown_count, 0);
        assert_eq!(result.event_timeline, artifact.events);
        assert!(result.engineering_summary().contains("stage_separations=1"));
    }

    #[test]
    fn reports_typed_lifecycle_events_with_timeline_section() {
        let mut artifact = artifact();
        artifact.events = vec![
            SimulationTelemetryEvent {
                simulation_time_s: 0.5,
                kind: "ignition".into(),
                detail: "stage=0 engines_started=9".into(),
                event_type: Some(SimulationEventType::Ignition),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 1.5,
                kind: "liftoff".into(),
                detail: "upward_thrust_n=1 weight_n=0".into(),
                event_type: Some(SimulationEventType::Liftoff),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 2.0,
                kind: "mission_phase".into(),
                detail: "previous=Launch current=Ascent".into(),
                event_type: Some(SimulationEventType::MissionPhaseChange),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 3.0,
                kind: "cutoff".into(),
                detail: "stage=0 engines_stopped=9".into(),
                event_type: Some(SimulationEventType::Cutoff),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 4.0,
                kind: "touchdown".into(),
                detail: "vertical_speed_mps=-1".into(),
                event_type: Some(SimulationEventType::Touchdown),
            },
            SimulationTelemetryEvent {
                simulation_time_s: 5.0,
                kind: "crash".into(),
                detail: "vertical_speed_mps=-80".into(),
                event_type: Some(SimulationEventType::Crash),
            },
        ];
        let result = analyze_simulation_artifact(&artifact, &[]).unwrap();
        assert_eq!(result.ignition_count, 1);
        assert_eq!(result.cutoff_count, 1);
        assert_eq!(result.liftoff_count, 1);
        assert_eq!(result.touchdown_count, 1);
        assert_eq!(result.crash_count, 1);
        assert_eq!(result.mission_phase_change_count, 1);
        let summary = result.engineering_summary();
        assert!(summary.contains("ignitions=1"));
        assert!(summary.contains("phase_changes=1"));
        assert!(summary.contains("EVENT TIMELINE"));
        assert!(summary.contains("t+0.500 ignition"));
        assert!(summary.contains("t+5.000 crash"));
    }

    #[test]
    fn accepts_legacy_analysis_schema_two() {
        let mut result = analyze_simulation_artifact(&artifact(), &[]).unwrap();
        result.schema_version = 2;
        let decoded = SimulationAnalysisResult::from_ron(&result.to_ron().unwrap()).unwrap();
        assert_eq!(decoded.schema_version, 2);
    }

    #[test]
    fn legacy_frames_without_optional_channels_report_unavailable_warnings() {
        let mut artifact = artifact();
        for frame in &mut artifact.telemetry {
            frame.angle_of_attack_rad = None;
            frame.total_heat_flux_w_m2 = None;
            frame.applied_thrust_n = None;
            frame.active_engine_count = None;
            frame.gravitational_parameter_m3_s2 = None;
        }
        let result = analyze_simulation_artifact(&artifact, &[]).unwrap();
        assert!(result.maximum_angle_of_attack_rad.is_none());
        assert!(result.maximum_heat_flux_w_m2.is_none());
        assert!(result.maximum_applied_thrust_n.is_none());
        assert!(result.maximum_active_engine_count.is_none());
        assert!(result.final_specific_orbital_energy_j_kg.is_none());
        let summary = result.engineering_summary();
        assert!(!summary.contains("max_applied_thrust_n"));
        assert!(!summary.contains("max_abs_aoa_rad"));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("propulsion output is unavailable")));
        assert!(result
            .warnings
            .iter()
            .any(|warning| warning.contains("orbital elements are unavailable")));
    }
}
