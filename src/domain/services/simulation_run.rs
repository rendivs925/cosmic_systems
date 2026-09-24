//! Reproducible identity for an engineering simulation run.
//!
//! This is deliberately a compact, Bevy-free record of the inputs that make a
//! deterministic result meaningful. It is not a second scenario authority:
//! application-level scenario/configuration owners provide these values.

use serde::{Deserialize, Serialize};

/// Immutable identity of a simulation run or recorded baseline.
///
/// Units and frames are explicit: `start_epoch_tdb_seconds_since_j2000` is TDB
/// seconds from J2000, and `state_reference_frame` names the authoritative
/// frame of recorded state vectors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationRunIdentity {
    pub scenario_id: String,
    pub vehicle_model_id: String,
    /// SHA-256 of the resolved vehicle configuration bytes when file-backed.
    /// `None` is explicit for in-memory test fixtures until they gain a
    /// canonical serialized configuration.
    pub vehicle_configuration_sha256: Option<String>,
    pub launch_site_id: String,
    pub environment_model_id: String,
    pub terrain_source_id: String,
    pub ephemeris_authority_id: String,
    pub start_epoch_tdb_seconds_since_j2000: f64,
    pub state_reference_frame: String,
    pub numerical_integrator: String,
    pub fixed_timestep_s: f64,
    /// `None` explicitly records that this run has no stochastic input.
    pub random_seed: Option<u64>,
    pub software_revision: String,
}

impl SimulationRunIdentity {
    /// Reject incomplete or ambiguous run records before they become evidence.
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("scenario_id", &self.scenario_id),
            ("vehicle_model_id", &self.vehicle_model_id),
            ("launch_site_id", &self.launch_site_id),
            ("environment_model_id", &self.environment_model_id),
            ("terrain_source_id", &self.terrain_source_id),
            ("ephemeris_authority_id", &self.ephemeris_authority_id),
            ("state_reference_frame", &self.state_reference_frame),
            ("numerical_integrator", &self.numerical_integrator),
            ("software_revision", &self.software_revision),
        ] {
            if value.trim().is_empty() {
                return Err(format!("simulation run identity {field} must not be blank"));
            }
        }
        if !self.start_epoch_tdb_seconds_since_j2000.is_finite() {
            return Err(
                "simulation run identity start_epoch_tdb_seconds_since_j2000 must be finite"
                    .to_string(),
            );
        }
        if !self.fixed_timestep_s.is_finite() || self.fixed_timestep_s <= 0.0 {
            return Err("simulation run identity fixed_timestep_s must be positive".to_string());
        }
        if self
            .vehicle_configuration_sha256
            .as_deref()
            .is_some_and(|checksum| !is_lowercase_sha256(checksum))
        {
            return Err(
                "simulation run identity vehicle_configuration_sha256 must be lowercase SHA-256"
                    .to_string(),
            );
        }
        Ok(())
    }
}

pub fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SimulationRunIdentity {
        SimulationRunIdentity {
            scenario_id: "ascent".into(),
            vehicle_model_id: "test-vehicle".into(),
            vehicle_configuration_sha256: None,
            launch_site_id: "test-site".into(),
            environment_model_id: "test-environment".into(),
            terrain_source_id: "test-terrain".into(),
            ephemeris_authority_id: "test-ephemeris".into(),
            start_epoch_tdb_seconds_since_j2000: 0.0,
            state_reference_frame: "Earth-centered inertial".into(),
            numerical_integrator: "semi-implicit Euler".into(),
            fixed_timestep_s: 1.0 / 64.0,
            random_seed: None,
            software_revision: "abc123".into(),
        }
    }

    #[test]
    fn accepts_explicit_non_file_backed_identity() {
        assert!(identity().validate().is_ok());
    }

    #[test]
    fn rejects_invalid_configuration_checksum() {
        let mut identity = identity();
        identity.vehicle_configuration_sha256 = Some("not-a-checksum".into());
        assert!(identity.validate().is_err());
    }
}
