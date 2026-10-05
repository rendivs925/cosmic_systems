//! Pure, serializable player profile value objects.
//!
//! The profile holds pre-flight game state only: settings, unlocked parts,
//! completed-mission results, and saved vehicle drafts. It never stores an
//! in-progress authoritative simulation state.

use super::mission::{MissionId, MissionRecord, MissionRecordError};
use super::parts::PartId;
use super::vehicle_draft::VehicleDraft;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashSet};
use std::fmt;

/// Current on-disk profile schema version.
pub const PROFILE_SCHEMA_VERSION: u32 = 1;
/// MVP bound on saved vehicle drafts.
pub const MAX_SAVED_DRAFTS: usize = 32;

/// Player-adjustable settings persisted with the profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameSettings {
    pub master_volume: f32,
    pub ui_scale: f32,
    pub motion_reduction: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            ui_scale: 1.0,
            motion_reduction: false,
        }
    }
}

impl GameSettings {
    pub fn validate(&self) -> Result<(), PlayerProfileError> {
        if !self.master_volume.is_finite() || !(0.0..=1.0).contains(&self.master_volume) {
            return Err(PlayerProfileError::InvalidSettings {
                field: "master_volume",
            });
        }
        if !self.ui_scale.is_finite() || !(0.5..=2.0).contains(&self.ui_scale) {
            return Err(PlayerProfileError::InvalidSettings { field: "ui_scale" });
        }
        Ok(())
    }
}

/// Persisted player progress and creations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerProfile {
    pub version: u32,
    pub settings: GameSettings,
    pub unlocked_parts: BTreeSet<PartId>,
    pub completed_missions: Vec<MissionRecord>,
    pub drafts: Vec<VehicleDraft>,
}

impl Default for PlayerProfile {
    fn default() -> Self {
        Self::new_default()
    }
}

impl PlayerProfile {
    /// A fresh profile at the current schema version with default settings.
    pub fn new_default() -> Self {
        Self {
            version: PROFILE_SCHEMA_VERSION,
            settings: GameSettings::default(),
            unlocked_parts: BTreeSet::new(),
            completed_missions: Vec::new(),
            drafts: Vec::new(),
        }
    }

    /// Validates version, settings, draft names, and mission records.
    pub fn validate(&self) -> Result<(), PlayerProfileError> {
        if self.version != PROFILE_SCHEMA_VERSION {
            return Err(PlayerProfileError::UnsupportedVersion {
                found: self.version,
                expected: PROFILE_SCHEMA_VERSION,
            });
        }
        self.settings.validate()?;
        if self.drafts.len() > MAX_SAVED_DRAFTS {
            return Err(PlayerProfileError::TooManyDrafts {
                count: self.drafts.len(),
            });
        }
        let mut names = HashSet::new();
        for draft in &self.drafts {
            let name = draft.name.trim();
            if name.is_empty() {
                return Err(PlayerProfileError::EmptyDraftName);
            }
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(PlayerProfileError::DuplicateDraftName(draft.name.clone()));
            }
        }
        for record in &self.completed_missions {
            record
                .validate()
                .map_err(PlayerProfileError::InvalidRecord)?;
        }
        Ok(())
    }

    pub fn has_completed(&self, mission: &MissionId) -> bool {
        self.completed_missions
            .iter()
            .any(|record| &record.mission == mission)
    }

    pub fn is_unlocked(&self, part: &PartId) -> bool {
        self.unlocked_parts.contains(part)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerProfileError {
    UnsupportedVersion { found: u32, expected: u32 },
    InvalidSettings { field: &'static str },
    TooManyDrafts { count: usize },
    EmptyDraftName,
    DuplicateDraftName(String),
    InvalidRecord(MissionRecordError),
}

impl fmt::Display for PlayerProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found, expected } => write!(
                f,
                "profile version {found} is not supported; expected {expected}"
            ),
            Self::InvalidSettings { field } => write!(f, "setting {field} is invalid"),
            Self::TooManyDrafts { count } => {
                write!(
                    f,
                    "profile has {count} drafts; maximum is {MAX_SAVED_DRAFTS}"
                )
            }
            Self::EmptyDraftName => write!(f, "saved vehicle draft name must not be empty"),
            Self::DuplicateDraftName(name) => write!(f, "duplicate draft name: {name}"),
            Self::InvalidRecord(error) => write!(f, "invalid mission record: {error}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::game::mission::MissionStatus;
    use crate::domain::game::parts::PartId;

    fn draft(name: &str) -> VehicleDraft {
        VehicleDraft::new(name, vec![PartId::new("engine_1").unwrap()])
    }

    #[test]
    fn default_profile_is_valid_and_empty() {
        let profile = PlayerProfile::new_default();
        assert_eq!(profile.validate(), Ok(()));
        assert!(!profile.is_unlocked(&PartId::new("tank_1").unwrap()));
    }

    #[test]
    fn unsupported_version_is_rejected() {
        let mut profile = PlayerProfile::new_default();
        profile.version = 99;
        assert!(matches!(
            profile.validate(),
            Err(PlayerProfileError::UnsupportedVersion { .. })
        ));
    }

    #[test]
    fn duplicate_draft_names_are_rejected_case_insensitively() {
        let mut profile = PlayerProfile::new_default();
        profile.drafts = vec![draft("Ares"), draft("ares")];
        assert!(matches!(
            profile.validate(),
            Err(PlayerProfileError::DuplicateDraftName(_))
        ));
    }

    #[test]
    fn completion_tracking_reads_records() {
        let mut profile = PlayerProfile::new_default();
        let mission = MissionId::new("reach_space").unwrap();
        assert!(!profile.has_completed(&mission));
        profile.completed_missions.push(MissionRecord {
            mission: mission.clone(),
            status: MissionStatus::Completed,
            flight_time_s: 90.0,
            max_altitude_m: 110_000.0,
            achieved_orbit: None,
            payload_deployed: false,
            recovery: None,
            score: 100,
        });
        assert!(profile.has_completed(&mission));
    }
}
