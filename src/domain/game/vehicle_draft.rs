//! Pure, serializable player vehicle drafts and their validation rules.
//!
//! A draft is an ordered bottom-to-top stack of catalogue part ids plus user
//! metadata. It is not a flight configuration: a validated draft is compiled
//! into the authoritative `VehicleDef` by the application layer.

use super::parts::{diameters_are_compatible, PartCatalog, PartCategory, PartId};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// MVP bound on serial stages (separators + 1).
pub const MAX_STAGES: usize = 3;
const MAX_DRAFT_NAME_LEN: usize = 64;

/// A player-authored stack, ordered from the bottom of the vehicle upward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VehicleDraft {
    pub name: String,
    pub parts: Vec<PartId>,
}

impl VehicleDraft {
    pub fn new(name: impl Into<String>, parts: Vec<PartId>) -> Self {
        Self {
            name: name.into(),
            parts,
        }
    }

    /// Splits the ordered stack into stages at separator boundaries. The first
    /// group is the launch stage.
    pub fn stage_groups(&self, catalog: &PartCatalog) -> Vec<Vec<PartId>> {
        let mut groups: Vec<Vec<PartId>> = vec![Vec::new()];
        for id in &self.parts {
            let is_separator = catalog
                .get(id)
                .is_some_and(|def| def.category == PartCategory::Separator);
            if is_separator {
                groups.push(Vec::new());
            } else {
                groups
                    .last_mut()
                    .expect("at least one group exists")
                    .push(id.clone());
            }
        }
        groups
    }

    /// Validates the draft against the catalogue and the player's unlocked set,
    /// collecting every diagnostic rather than stopping at the first.
    pub fn validate(
        &self,
        catalog: &PartCatalog,
        unlocked: &BTreeSet<PartId>,
    ) -> Result<(), Vec<DraftValidationError>> {
        let mut errors = Vec::new();

        if self.name.trim().is_empty() {
            errors.push(DraftValidationError::EmptyName);
        } else if self.name.trim().len() > MAX_DRAFT_NAME_LEN {
            errors.push(DraftValidationError::NameTooLong);
        }
        if self.parts.is_empty() {
            errors.push(DraftValidationError::EmptyStack);
            return Err(errors);
        }

        // Resolve every part and check the unlocked set.
        let mut resolved = Vec::with_capacity(self.parts.len());
        for id in &self.parts {
            match catalog.get(id) {
                None => errors.push(DraftValidationError::UnknownPart(id.clone())),
                Some(def) => {
                    if !unlocked.contains(id) {
                        errors.push(DraftValidationError::PartNotUnlocked(id.clone()));
                    }
                    resolved.push(def);
                }
            }
        }
        if resolved.len() != self.parts.len() {
            // Unknown parts prevent reliable stack analysis.
            return Err(errors);
        }

        // Adjacent stack compatibility.
        for window in resolved.windows(2) {
            if !diameters_are_compatible(window[0].diameter_m, window[1].diameter_m) {
                errors.push(DraftValidationError::IncompatibleAttachment {
                    lower: window[0].id.clone(),
                    upper: window[1].id.clone(),
                });
            }
        }

        // Separators may not terminate the stack.
        let last = resolved.len() - 1;
        for (index, def) in resolved.iter().enumerate() {
            if def.category == PartCategory::Separator {
                if index == 0 {
                    errors.push(DraftValidationError::SeparatorAtBottom);
                }
                if index == last {
                    errors.push(DraftValidationError::SeparatorAtTop);
                }
            }
        }

        // Group into stages at separators.
        let mut stages: Vec<Vec<&super::parts::PartDef>> = vec![Vec::new()];
        for def in &resolved {
            if def.category == PartCategory::Separator {
                stages.push(Vec::new());
            } else {
                stages
                    .last_mut()
                    .expect("at least one stage exists")
                    .push(def);
            }
        }
        if stages.len() > MAX_STAGES {
            errors.push(DraftValidationError::TooManyStages {
                count: stages.len(),
            });
        }
        for (index, stage) in stages.iter().enumerate() {
            let has_engine = stage.iter().any(|d| d.category == PartCategory::Engine);
            let has_tank = stage.iter().any(|d| d.category == PartCategory::FuelTank);
            if !has_engine {
                errors.push(DraftValidationError::StageWithoutEngine { stage_index: index });
            }
            if !has_tank {
                errors.push(DraftValidationError::StageWithoutTank { stage_index: index });
            }
        }

        if !resolved
            .iter()
            .any(|d| d.category == PartCategory::CommandCapsule)
        {
            errors.push(DraftValidationError::MissingControlPath);
        }
        if !resolved.iter().any(|d| d.category == PartCategory::Engine) {
            errors.push(DraftValidationError::MissingLaunchEngine);
        }

        // The fairing is only valid on the top serial stage.
        if let Some(top) = stages.last() {
            for def in &resolved {
                if def.category == PartCategory::Fairing && !top.iter().any(|d| d.id == def.id) {
                    errors.push(DraftValidationError::FairingNotTopStage);
                    break;
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DraftValidationError {
    EmptyName,
    NameTooLong,
    EmptyStack,
    UnknownPart(PartId),
    PartNotUnlocked(PartId),
    IncompatibleAttachment { lower: PartId, upper: PartId },
    SeparatorAtBottom,
    SeparatorAtTop,
    TooManyStages { count: usize },
    StageWithoutEngine { stage_index: usize },
    StageWithoutTank { stage_index: usize },
    MissingControlPath,
    MissingLaunchEngine,
    FairingNotTopStage,
}

impl fmt::Display for DraftValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "vehicle name must not be empty"),
            Self::NameTooLong => write!(f, "vehicle name is too long"),
            Self::EmptyStack => write!(f, "vehicle stack must contain at least one part"),
            Self::UnknownPart(id) => write!(f, "unknown part: {id}"),
            Self::PartNotUnlocked(id) => write!(f, "part is not unlocked: {id}"),
            Self::IncompatibleAttachment { lower, upper } => {
                write!(f, "cannot attach {upper} above {lower}: diameter mismatch")
            }
            Self::SeparatorAtBottom => write!(f, "stack cannot begin with a separator"),
            Self::SeparatorAtTop => write!(f, "stack cannot end with a separator"),
            Self::TooManyStages { count } => {
                write!(
                    f,
                    "vehicle has {count} stages; MVP supports at most {MAX_STAGES}"
                )
            }
            Self::StageWithoutEngine { stage_index } => {
                write!(f, "stage {stage_index} has no engine")
            }
            Self::StageWithoutTank { stage_index } => {
                write!(f, "stage {stage_index} has no fuel tank")
            }
            Self::MissingControlPath => write!(f, "vehicle has no command capsule"),
            Self::MissingLaunchEngine => write!(f, "vehicle has no powered launch stage"),
            Self::FairingNotTopStage => write!(f, "fairing is only valid on the top stage"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::game::parts::{EnginePartSpec, LandingLegsPartSpec, PartDef};

    fn id(value: &str) -> PartId {
        PartId::new(value).unwrap()
    }

    fn engine() -> EnginePartSpec {
        EnginePartSpec {
            rated_thrust_n: 845_000.0,
            isp_sea_level_s: 282.0,
            isp_vacuum_s: 311.0,
            gimbal_range_deg: 5.0,
            throttle_min: 0.4,
            throttle_max: 1.0,
            max_ignitions: 3,
        }
    }

    fn def(id_value: &str, category: PartCategory, diameter_m: f64) -> PartDef {
        PartDef {
            id: id(id_value),
            name: id_value.to_string(),
            category,
            dry_mass_kg: 1_000.0,
            diameter_m,
            height_m: 5.0,
            propellant_mass_kg: if category == PartCategory::FuelTank {
                10_000.0
            } else {
                0.0
            },
            engine: (category == PartCategory::Engine).then(engine),
            landing_legs: (category == PartCategory::LandingLegs).then_some(LandingLegsPartSpec {
                count: 4,
                base_radius_m: 4.0,
                stroke_m: 2.0,
                deploy_altitude_m: 2_000.0,
            }),
        }
    }

    fn catalog() -> PartCatalog {
        PartCatalog::new(vec![
            def("engine_1", PartCategory::Engine, 3.7),
            def("tank_1", PartCategory::FuelTank, 3.7),
            def("separator_1", PartCategory::Separator, 3.7),
            def("capsule", PartCategory::CommandCapsule, 3.7),
            def("fairing", PartCategory::Fairing, 3.7),
        ])
        .unwrap()
    }

    fn all_unlocked(catalog: &PartCatalog) -> BTreeSet<PartId> {
        catalog.iter().map(|p| p.id.clone()).collect()
    }

    #[test]
    fn valid_single_stage_draft_passes() {
        let catalog = catalog();
        let draft = VehicleDraft::new("Hopper", vec![id("engine_1"), id("tank_1"), id("capsule")]);
        assert_eq!(draft.validate(&catalog, &all_unlocked(&catalog)), Ok(()));
    }

    #[test]
    fn missing_control_or_engine_is_reported() {
        let catalog = catalog();
        let unlocked = all_unlocked(&catalog);
        let draft = VehicleDraft::new("NoControl", vec![id("engine_1"), id("tank_1")]);
        let errors = draft.validate(&catalog, &unlocked).unwrap_err();
        assert!(errors.contains(&DraftValidationError::MissingControlPath));
    }

    #[test]
    fn unknown_and_locked_parts_are_reported() {
        let catalog = catalog();
        let mut unlocked = all_unlocked(&catalog);
        unlocked.remove(&id("tank_1"));
        let draft = VehicleDraft::new("Locked", vec![id("engine_1"), id("tank_1"), id("capsule")]);
        let errors = draft.validate(&catalog, &unlocked).unwrap_err();
        assert!(errors.contains(&DraftValidationError::PartNotUnlocked(id("tank_1"))));

        let draft = VehicleDraft::new("Missing", vec![id("engine_1"), id("ghost")]);
        let errors = draft
            .validate(&catalog, &all_unlocked(&catalog))
            .unwrap_err();
        assert!(errors.contains(&DraftValidationError::UnknownPart(id("ghost"))));
    }

    #[test]
    fn separator_splits_stages_and_must_be_interior() {
        let catalog = catalog();
        let unlocked = all_unlocked(&catalog);
        let draft = VehicleDraft::new(
            "TwoStage",
            vec![
                id("engine_1"),
                id("tank_1"),
                id("separator_1"),
                id("engine_1"),
                id("tank_1"),
                id("capsule"),
            ],
        );
        assert_eq!(draft.stage_groups(&catalog).len(), 2);
        assert_eq!(draft.validate(&catalog, &unlocked), Ok(()));

        let bad = VehicleDraft::new(
            "BadSep",
            vec![
                id("separator_1"),
                id("engine_1"),
                id("tank_1"),
                id("capsule"),
            ],
        );
        let errors = bad.validate(&catalog, &unlocked).unwrap_err();
        assert!(errors.contains(&DraftValidationError::SeparatorAtBottom));
    }

    #[test]
    fn stage_without_engine_or_tank_is_reported() {
        let catalog = catalog();
        let unlocked = all_unlocked(&catalog);
        let draft = VehicleDraft::new(
            "NoUpperEngine",
            vec![
                id("engine_1"),
                id("tank_1"),
                id("separator_1"),
                id("tank_1"),
                id("capsule"),
            ],
        );
        let errors = draft.validate(&catalog, &unlocked).unwrap_err();
        assert!(errors.contains(&DraftValidationError::StageWithoutEngine { stage_index: 1 }));
    }

    #[test]
    fn incompatible_diameters_are_reported() {
        let catalog = PartCatalog::new(vec![
            def("engine_1", PartCategory::Engine, 3.7),
            def("tank_wide", PartCategory::FuelTank, 5.2),
            def("capsule", PartCategory::CommandCapsule, 3.7),
        ])
        .unwrap();
        let unlocked = all_unlocked(&catalog);
        let draft = VehicleDraft::new(
            "Mismatch",
            vec![id("engine_1"), id("tank_wide"), id("capsule")],
        );
        let errors = draft.validate(&catalog, &unlocked).unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, DraftValidationError::IncompatibleAttachment { .. })));
    }
}
