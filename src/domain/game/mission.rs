//! Pure, serializable MVP mission, objective, and reward value objects.
//!
//! Objective predicates are data. Evaluation against authoritative flight state
//! lives in a later pure domain service so mission results stay deterministic
//! and testable without launching Bevy.

use super::parts::PartId;
use serde::{Deserialize, Serialize};
use std::fmt;

const MAX_MISSION_ID_LEN: usize = 64;

/// A validated, stable mission identifier (lowercase slug).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MissionId(String);

impl MissionId {
    pub fn new(raw: &str) -> Result<Self, MissionIdError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(MissionIdError::Empty);
        }
        if trimmed.len() > MAX_MISSION_ID_LEN {
            return Err(MissionIdError::TooLong);
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(MissionIdError::InvalidCharacters);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for MissionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for MissionId {
    type Error = MissionIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<MissionId> for String {
    fn from(value: MissionId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionIdError {
    Empty,
    TooLong,
    InvalidCharacters,
}

impl fmt::Display for MissionIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "mission id must not be empty"),
            Self::TooLong => write!(f, "mission id is too long"),
            Self::InvalidCharacters => write!(f, "mission id contains invalid characters"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MissionStatus {
    Completed,
    Failed,
}

/// One data-driven success predicate evaluated from authoritative state.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Objective {
    ReachAltitude {
        min_altitude_m: f64,
    },
    AchieveOrbit {
        min_periapsis_m: f64,
        max_apoapsis_m: f64,
        min_orbital_period_s: f64,
    },
    DeployPayload {
        min_periapsis_m: f64,
        max_apoapsis_m: f64,
    },
    RecoverCapsule {
        max_descent_mps: f64,
        max_tilt_deg: f64,
    },
}

impl Objective {
    pub fn validate(&self) -> Result<(), MissionDefError> {
        match *self {
            Self::ReachAltitude { min_altitude_m } => {
                require_positive(min_altitude_m, "min_altitude_m")
            }
            Self::AchieveOrbit {
                min_periapsis_m,
                max_apoapsis_m,
                min_orbital_period_s,
            } => {
                require_positive(min_periapsis_m, "min_periapsis_m")?;
                require_positive(max_apoapsis_m, "max_apoapsis_m")?;
                require_positive(min_orbital_period_s, "min_orbital_period_s")?;
                if min_periapsis_m >= max_apoapsis_m {
                    return Err(MissionDefError::InvalidObjective {
                        field: "orbit bounds",
                    });
                }
                Ok(())
            }
            Self::DeployPayload {
                min_periapsis_m,
                max_apoapsis_m,
            } => {
                require_positive(min_periapsis_m, "min_periapsis_m")?;
                require_positive(max_apoapsis_m, "max_apoapsis_m")?;
                if min_periapsis_m >= max_apoapsis_m {
                    return Err(MissionDefError::InvalidObjective {
                        field: "payload orbit bounds",
                    });
                }
                Ok(())
            }
            Self::RecoverCapsule {
                max_descent_mps,
                max_tilt_deg,
            } => {
                require_positive(max_descent_mps, "max_descent_mps")?;
                require_non_negative(max_tilt_deg, "max_tilt_deg")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reward {
    pub unlocked_parts: Vec<PartId>,
    pub score: u32,
}

/// One mission definition. `allowed_parts` bounds the assembly palette.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionDef {
    pub id: MissionId,
    pub name: String,
    pub prerequisite: Option<MissionId>,
    pub allowed_parts: Vec<PartId>,
    pub objectives: Vec<Objective>,
    pub reward: Reward,
}

impl MissionDef {
    pub fn validate(&self) -> Result<(), MissionDefError> {
        if self.name.trim().is_empty() {
            return Err(MissionDefError::EmptyName);
        }
        if self.objectives.is_empty() {
            return Err(MissionDefError::NoObjectives);
        }
        if let Some(prerequisite) = &self.prerequisite {
            if prerequisite == &self.id {
                return Err(MissionDefError::SelfPrerequisite);
            }
        }
        for objective in &self.objectives {
            objective.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionDefError {
    EmptyName,
    NoObjectives,
    SelfPrerequisite,
    InvalidObjective { field: &'static str },
}

impl fmt::Display for MissionDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "mission name must not be empty"),
            Self::NoObjectives => write!(f, "mission must define at least one objective"),
            Self::SelfPrerequisite => write!(f, "mission cannot require itself"),
            Self::InvalidObjective { field } => write!(f, "invalid objective {field}"),
        }
    }
}

/// Debrief summary of an achieved orbit.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct OrbitSummary {
    pub apoapsis_m: f64,
    pub periapsis_m: f64,
    pub period_s: f64,
}

/// Debrief summary of a recovery outcome.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RecoverySummary {
    pub touchdown_speed_mps: f64,
    pub tilt_deg: f64,
    pub on_land: bool,
}

/// Persisted result of one finished mission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionRecord {
    pub mission: MissionId,
    pub status: MissionStatus,
    pub flight_time_s: f64,
    pub max_altitude_m: f64,
    pub achieved_orbit: Option<OrbitSummary>,
    pub payload_deployed: bool,
    pub recovery: Option<RecoverySummary>,
    pub score: u32,
}

impl MissionRecord {
    pub fn validate(&self) -> Result<(), MissionRecordError> {
        if !self.flight_time_s.is_finite() || self.flight_time_s < 0.0 {
            return Err(MissionRecordError::InvalidNumber {
                field: "flight_time_s",
            });
        }
        if !self.max_altitude_m.is_finite() {
            return Err(MissionRecordError::InvalidNumber {
                field: "max_altitude_m",
            });
        }
        if let Some(orbit) = self.achieved_orbit {
            if !orbit.apoapsis_m.is_finite()
                || !orbit.periapsis_m.is_finite()
                || !orbit.period_s.is_finite()
                || orbit.period_s < 0.0
            {
                return Err(MissionRecordError::InvalidNumber {
                    field: "achieved_orbit",
                });
            }
        }
        if let Some(recovery) = self.recovery {
            if !recovery.touchdown_speed_mps.is_finite() || !recovery.tilt_deg.is_finite() {
                return Err(MissionRecordError::InvalidNumber { field: "recovery" });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissionRecordError {
    InvalidNumber { field: &'static str },
}

impl fmt::Display for MissionRecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNumber { field } => write!(f, "mission record {field} is invalid"),
        }
    }
}

fn require_positive(value: f64, field: &'static str) -> Result<(), MissionDefError> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(MissionDefError::InvalidObjective { field })
    }
}

fn require_non_negative(value: f64, field: &'static str) -> Result<(), MissionDefError> {
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(MissionDefError::InvalidObjective { field })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mission() -> MissionDef {
        MissionDef {
            id: MissionId::new("reach_space").unwrap(),
            name: "Reach Space".to_string(),
            prerequisite: None,
            allowed_parts: vec![PartId::new("tank_1").unwrap()],
            objectives: vec![Objective::ReachAltitude {
                min_altitude_m: 100_000.0,
            }],
            reward: Reward {
                unlocked_parts: vec![],
                score: 100,
            },
        }
    }

    #[test]
    fn valid_mission_passes() {
        assert_eq!(mission().validate(), Ok(()));
    }

    #[test]
    fn orbit_objective_requires_ordered_bounds() {
        let mut m = mission();
        m.objectives = vec![Objective::AchieveOrbit {
            min_periapsis_m: 300_000.0,
            max_apoapsis_m: 200_000.0,
            min_orbital_period_s: 5_400.0,
        }];
        assert!(matches!(
            m.validate(),
            Err(MissionDefError::InvalidObjective { .. })
        ));
    }

    #[test]
    fn empty_objectives_and_self_prerequisite_are_rejected() {
        let mut m = mission();
        m.objectives.clear();
        assert_eq!(m.validate(), Err(MissionDefError::NoObjectives));

        let mut m = mission();
        m.prerequisite = Some(m.id.clone());
        assert_eq!(m.validate(), Err(MissionDefError::SelfPrerequisite));
    }

    #[test]
    fn mission_record_validates_numbers() {
        let record = MissionRecord {
            mission: MissionId::new("reach_space").unwrap(),
            status: MissionStatus::Completed,
            flight_time_s: 120.0,
            max_altitude_m: 120_000.0,
            achieved_orbit: None,
            payload_deployed: false,
            recovery: None,
            score: 100,
        };
        assert_eq!(record.validate(), Ok(()));

        let mut bad = record.clone();
        bad.flight_time_s = f64::NAN;
        assert!(bad.validate().is_err());
    }
}
