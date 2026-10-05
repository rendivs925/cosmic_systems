//! Pure, serializable MVP part-catalog value objects.
//!
//! These describe the deliberately small set of stack parts a player may use in
//! the assembly screen. They are configuration data only: compilation into the
//! authoritative `VehicleDef` happens in the application layer, and no value
//! here carries Bevy state or simulation behaviour.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fmt;

/// Upper bound on a part identifier so save data and RON stay bounded.
const MAX_PART_ID_LEN: usize = 64;
/// Diameters within this tolerance are treated as stack-compatible, meters.
pub const STACK_DIAMETER_TOLERANCE_M: f64 = 0.05;

/// A validated, stable part identifier (lowercase slug).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PartId(String);

impl PartId {
    pub fn new(raw: &str) -> Result<Self, PartIdError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(PartIdError::Empty);
        }
        if trimmed.len() > MAX_PART_ID_LEN {
            return Err(PartIdError::TooLong { len: trimmed.len() });
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
        {
            return Err(PartIdError::InvalidCharacters);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for PartId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for PartId {
    type Error = PartIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<PartId> for String {
    fn from(value: PartId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartIdError {
    Empty,
    TooLong { len: usize },
    InvalidCharacters,
}

impl fmt::Display for PartIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "part id must not be empty"),
            Self::TooLong { len } => {
                write!(
                    f,
                    "part id is {len} characters; maximum is {MAX_PART_ID_LEN}"
                )
            }
            Self::InvalidCharacters => write!(
                f,
                "part id may only contain lowercase letters, digits, '_' and '-'"
            ),
        }
    }
}

/// Role a part plays in the stack. Compilation groups these into stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartCategory {
    CommandCapsule,
    Payload,
    FuelTank,
    Engine,
    Separator,
    Fairing,
    LandingLegs,
}

/// Engine performance embedded in an [`PartCategory::Engine`] part.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EnginePartSpec {
    pub rated_thrust_n: f64,
    pub isp_sea_level_s: f64,
    pub isp_vacuum_s: f64,
    pub gimbal_range_deg: f64,
    pub throttle_min: f64,
    pub throttle_max: f64,
    pub max_ignitions: u32,
}

/// Deployable landing gear embedded in a [`PartCategory::LandingLegs`] part.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LandingLegsPartSpec {
    pub count: u32,
    pub base_radius_m: f64,
    pub stroke_m: f64,
    pub deploy_altitude_m: f64,
}

/// One catalogue entry. All quantities use explicit SI units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartDef {
    pub id: PartId,
    pub name: String,
    pub category: PartCategory,
    pub dry_mass_kg: f64,
    pub diameter_m: f64,
    pub height_m: f64,
    /// Usable propellant for [`PartCategory::FuelTank`] parts; zero otherwise.
    #[serde(default)]
    pub propellant_mass_kg: f64,
    #[serde(default)]
    pub engine: Option<EnginePartSpec>,
    #[serde(default)]
    pub landing_legs: Option<LandingLegsPartSpec>,
}

impl PartDef {
    /// Validates units, signs, and category/field consistency.
    pub fn validate(&self) -> Result<(), PartDefError> {
        if self.name.trim().is_empty() {
            return Err(PartDefError::EmptyName);
        }
        for (label, value) in [
            ("dry_mass_kg", self.dry_mass_kg),
            ("propellant_mass_kg", self.propellant_mass_kg),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(PartDefError::InvalidMass { field: label });
            }
        }
        for (label, value) in [("diameter_m", self.diameter_m), ("height_m", self.height_m)] {
            if !value.is_finite() || value <= 0.0 {
                return Err(PartDefError::InvalidDimension { field: label });
            }
        }
        match self.category {
            PartCategory::FuelTank if self.propellant_mass_kg <= 0.0 => {
                return Err(PartDefError::TankWithoutPropellant);
            }
            PartCategory::FuelTank if self.engine.is_some() => {
                return Err(PartDefError::UnexpectedField {
                    field: "engine",
                    category: self.category,
                });
            }
            PartCategory::Engine if self.engine.is_none() => {
                return Err(PartDefError::EngineWithoutSpec);
            }
            PartCategory::Engine if self.propellant_mass_kg > 0.0 => {
                return Err(PartDefError::UnexpectedField {
                    field: "propellant_mass_kg",
                    category: self.category,
                });
            }
            PartCategory::LandingLegs if self.landing_legs.is_none() => {
                return Err(PartDefError::MissingField {
                    field: "landing_legs",
                    category: self.category,
                });
            }
            _ => {}
        }
        if let Some(engine) = self.engine {
            engine.validate()?;
        }
        Ok(())
    }
}

impl EnginePartSpec {
    pub fn validate(&self) -> Result<(), PartDefError> {
        if !self.rated_thrust_n.is_finite() || self.rated_thrust_n <= 0.0 {
            return Err(PartDefError::InvalidEngine {
                field: "rated_thrust_n",
            });
        }
        for (label, value) in [
            ("isp_sea_level_s", self.isp_sea_level_s),
            ("isp_vacuum_s", self.isp_vacuum_s),
            ("gimbal_range_deg", self.gimbal_range_deg),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(PartDefError::InvalidEngine { field: label });
            }
        }
        if !self.throttle_min.is_finite()
            || !self.throttle_max.is_finite()
            || self.throttle_min < 0.0
            || self.throttle_max > 1.0
            || self.throttle_min >= self.throttle_max
        {
            return Err(PartDefError::InvalidEngine {
                field: "throttle range",
            });
        }
        if self.max_ignitions == 0 {
            return Err(PartDefError::InvalidEngine {
                field: "max_ignitions",
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartDefError {
    EmptyName,
    InvalidMass {
        field: &'static str,
    },
    InvalidDimension {
        field: &'static str,
    },
    TankWithoutPropellant,
    EngineWithoutSpec,
    MissingField {
        field: &'static str,
        category: PartCategory,
    },
    UnexpectedField {
        field: &'static str,
        category: PartCategory,
    },
    InvalidEngine {
        field: &'static str,
    },
}

impl fmt::Display for PartDefError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyName => write!(f, "part name must not be empty"),
            Self::InvalidMass { field } => write!(f, "{field} must be finite and non-negative"),
            Self::InvalidDimension { field } => write!(f, "{field} must be finite and positive"),
            Self::TankWithoutPropellant => {
                write!(f, "fuel tank must carry positive propellant_mass_kg")
            }
            Self::EngineWithoutSpec => write!(f, "engine part requires an engine spec"),
            Self::MissingField { field, category } => {
                write!(f, "{category:?} part requires field {field}")
            }
            Self::UnexpectedField { field, category } => {
                write!(f, "{category:?} part must not set field {field}")
            }
            Self::InvalidEngine { field } => write!(f, "engine {field} is invalid"),
        }
    }
}

/// A bounded, validated set of parts. Order is catalogue order and stable.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "Vec<PartDef>", into = "Vec<PartDef>")]
pub struct PartCatalog {
    parts: Vec<PartDef>,
}

impl PartCatalog {
    /// Builds a catalogue, rejecting duplicate ids and invalid part definitions.
    pub fn new(parts: Vec<PartDef>) -> Result<Self, PartCatalogError> {
        let mut seen = BTreeSet::new();
        for part in &parts {
            part.validate().map_err(PartCatalogError::InvalidPart)?;
            if !seen.insert(part.id.clone()) {
                return Err(PartCatalogError::DuplicateId(part.id.clone()));
            }
        }
        Ok(Self { parts })
    }

    pub fn get(&self, id: &PartId) -> Option<&PartDef> {
        self.parts.iter().find(|part| &part.id == id)
    }

    pub fn contains(&self, id: &PartId) -> bool {
        self.get(id).is_some()
    }

    pub fn iter(&self) -> impl Iterator<Item = &PartDef> {
        self.parts.iter()
    }

    pub fn len(&self) -> usize {
        self.parts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
    }
}

impl TryFrom<Vec<PartDef>> for PartCatalog {
    type Error = PartCatalogError;

    fn try_from(value: Vec<PartDef>) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PartCatalog> for Vec<PartDef> {
    fn from(value: PartCatalog) -> Self {
        value.parts
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartCatalogError {
    InvalidPart(PartDefError),
    DuplicateId(PartId),
}

impl fmt::Display for PartCatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPart(error) => write!(f, "invalid part: {error}"),
            Self::DuplicateId(id) => write!(f, "duplicate part id: {id}"),
        }
    }
}

impl std::error::Error for PartIdError {}
impl std::error::Error for PartDefError {}
impl std::error::Error for PartCatalogError {}

/// Diameters are stack-compatible when they match within tolerance.
pub fn diameters_are_compatible(lower_m: f64, upper_m: f64) -> bool {
    (lower_m - upper_m).abs() <= STACK_DIAMETER_TOLERANCE_M
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_spec() -> EnginePartSpec {
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

    fn part(id: &str, category: PartCategory) -> PartDef {
        PartDef {
            id: PartId::new(id).unwrap(),
            name: id.to_string(),
            category,
            dry_mass_kg: 1_000.0,
            diameter_m: 3.7,
            height_m: 5.0,
            propellant_mass_kg: if category == PartCategory::FuelTank {
                10_000.0
            } else {
                0.0
            },
            engine: (category == PartCategory::Engine).then(engine_spec),
            landing_legs: None,
        }
    }

    #[test]
    fn part_ids_are_validated_and_normalized() {
        assert_eq!(PartId::new("  tank_1  ").unwrap().as_str(), "tank_1");
        assert_eq!(PartId::new(""), Err(PartIdError::Empty));
        assert_eq!(PartId::new("Bad Id"), Err(PartIdError::InvalidCharacters));
        assert!(matches!(
            PartId::new(&"x".repeat(65)),
            Err(PartIdError::TooLong { .. })
        ));
    }

    #[test]
    fn catalog_rejects_duplicates_and_invalid_parts() {
        let good = part("tank", PartCategory::FuelTank);
        assert!(PartCatalog::new(vec![good.clone()]).is_ok());
        assert_eq!(
            PartCatalog::new(vec![good.clone(), good.clone()]),
            Err(PartCatalogError::DuplicateId(good.id.clone()))
        );
        let mut bad = part("engine", PartCategory::Engine);
        bad.engine = None;
        assert!(matches!(
            PartCatalog::new(vec![bad]),
            Err(PartCatalogError::InvalidPart(
                PartDefError::EngineWithoutSpec
            ))
        ));
    }

    #[test]
    fn tank_and_engine_category_rules_are_enforced() {
        let mut tank = part("tank", PartCategory::FuelTank);
        tank.propellant_mass_kg = 0.0;
        assert_eq!(tank.validate(), Err(PartDefError::TankWithoutPropellant));
        let mut engine = part("engine", PartCategory::Engine);
        engine.propellant_mass_kg = 5.0;
        assert!(matches!(
            engine.validate(),
            Err(PartDefError::UnexpectedField { .. })
        ));
    }

    #[test]
    fn diameter_compatibility_uses_tolerance() {
        assert!(diameters_are_compatible(3.7, 3.7));
        assert!(diameters_are_compatible(3.7, 3.74));
        assert!(!diameters_are_compatible(3.7, 5.2));
    }
}
