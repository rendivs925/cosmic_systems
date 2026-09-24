//! Versioned, offline scientific-reference case contracts.
//!
//! Cases record externally generated values and their provenance. This module
//! only validates the data contract; evaluating cases belongs to the offline
//! scientific-validation runner.

use crate::domain::math::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const SCIENTIFIC_REFERENCE_FORMAT_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScientificReferenceCaseSet {
    pub format_version: u32,
    pub cases: Vec<ScientificReferenceCase>,
}

impl ScientificReferenceCaseSet {
    pub fn validate(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.format_version != SCIENTIFIC_REFERENCE_FORMAT_VERSION {
            return Err(ScientificReferenceCaseError::UnsupportedFormatVersion {
                actual: self.format_version,
            });
        }

        let mut case_ids = HashSet::with_capacity(self.cases.len());
        for case in &self.cases {
            case.validate()?;
            if !case_ids.insert(case.header().metadata.id.clone()) {
                return Err(ScientificReferenceCaseError::DuplicateCaseId);
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub enum ScientificReferenceCase {
    BodyState(BodyStateReferenceCase),
    Orientation(OrientationReferenceCase),
    LaunchSite(LaunchSiteReferenceCase),
    SunDirection(SunDirectionReferenceCase),
    Gravity(GravityReferenceCase),
    Propagation(PropagationReferenceCase),
}

impl ScientificReferenceCase {
    pub fn header(&self) -> &ScientificReferenceCaseHeader {
        match self {
            Self::BodyState(case) => &case.header,
            Self::Orientation(case) => &case.header,
            Self::LaunchSite(case) => &case.header,
            Self::SunDirection(case) => &case.header,
            Self::Gravity(case) => &case.header,
            Self::Propagation(case) => &case.header,
        }
    }

    fn validate(&self) -> Result<(), ScientificReferenceCaseError> {
        self.header().validate()?;
        match self {
            Self::BodyState(case) => case.validate_payload(),
            Self::Orientation(case) => case.validate_payload(),
            Self::LaunchSite(case) => case.validate_payload(),
            Self::SunDirection(case) => case.validate_payload(),
            Self::Gravity(case) => case.validate_payload(),
            Self::Propagation(case) => case.validate_payload(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScientificReferenceCaseHeader {
    pub metadata: ScientificReferenceMetadata,
    pub coordinate_system: ScientificReferenceCoordinateSystem,
    /// Julian date in the coordinate system's declared time scale.
    pub julian_date: f64,
}

impl ScientificReferenceCaseHeader {
    fn validate(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.metadata.id.0.trim().is_empty()
            || self.metadata.generation_command.trim().is_empty()
            || self.metadata.datasets.is_empty()
            || !self.julian_date.is_finite()
        {
            return Err(ScientificReferenceCaseError::InvalidMetadata);
        }
        if !self.metadata.source.url.starts_with("https://") {
            return Err(ScientificReferenceCaseError::InvalidSourceUrl);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub struct ScientificReferenceCaseId(String);

impl ScientificReferenceCaseId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScientificReferenceMetadata {
    pub id: ScientificReferenceCaseId,
    pub source: ScientificReferenceSource,
    pub generation_command: String,
    pub datasets: Vec<ScientificReferenceDataset>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScientificReferenceSource {
    pub provider: ScientificReferenceProvider,
    pub url: String,
    pub source_version: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceProvider {
    JplHorizons,
    NaifSpice,
    Iers,
    Nasa,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct ScientificReferenceDataset {
    pub role: ScientificReferenceDatasetRole,
    pub identifier: String,
    pub version: String,
    pub sha256: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceDatasetRole {
    Ephemeris,
    Orientation,
    EarthOrientation,
    GravityModel,
    LeapSeconds,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ScientificReferenceCoordinateSystem {
    pub frame: ScientificReferenceFrame,
    pub center: ScientificReferenceCenter,
    pub time_scale: ScientificReferenceTimeScale,
    pub units: ScientificReferenceUnits,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceFrame {
    SsbIcrfJ2000,
    PlanetCenteredIcrfJ2000,
    IauBodyFixed,
    EarthFixed,
    LocalTangentEnu,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceCenter {
    SolarSystemBarycenter,
    NaifBody(i32),
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceTimeScale {
    Tdb,
    Utc,
    Tai,
    Tt,
    Ut1,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceUnits {
    SiMetersSeconds,
    Radians,
    UnitVector,
    MixedSiAndRadians,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReferenceVector3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl ReferenceVector3 {
    pub fn as_dvec3(self) -> DVec3 {
        DVec3::new(self.x, self.y, self.z)
    }

    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

/// External state authority consumed by scientific-reference validation.
///
/// The runner receives an implementation rather than loading kernels itself so
/// tests can use a deterministic authority and runtime composition retains one

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScientificValidationStatus {
    Passed,
    Failed,
    Unverified,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScientificValidationResidual {
    pub position_m: f64,
    pub velocity_mps: f64,
    pub budget: StateResidualBudget,
}

impl ScientificValidationResidual {
    pub fn within_budget(self) -> bool {
        self.position_m <= self.budget.position_m && self.velocity_mps <= self.budget.velocity_mps
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScientificValidationCaseResult {
    pub case_id: ScientificReferenceCaseId,
    pub status: ScientificValidationStatus,
    pub residual: Option<ScientificValidationResidual>,
    pub detail: String,
}

impl ScientificValidationCaseResult {
    pub(crate) fn unverified(
        case_id: ScientificReferenceCaseId,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            case_id,
            status: ScientificValidationStatus::Unverified,
            residual: None,
            detail: detail.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ScientificValidationReport {
    pub cases: Vec<ScientificValidationCaseResult>,
}

impl ScientificValidationReport {
    pub fn passed(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.status == ScientificValidationStatus::Passed)
            .count()
    }

    pub fn failed(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.status == ScientificValidationStatus::Failed)
            .count()
    }

    pub fn unverified(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.status == ScientificValidationStatus::Unverified)
            .count()
    }

    pub fn is_verified(&self) -> bool {
        self.failed() == 0 && self.unverified() == 0
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReferenceStateVector {
    pub position_m: ReferenceVector3,
    pub velocity_mps: ReferenceVector3,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct StateResidualBudget {
    pub position_m: f64,
    pub velocity_mps: f64,
}

impl StateResidualBudget {
    fn is_valid(self) -> bool {
        self.position_m.is_finite()
            && self.position_m > 0.0
            && self.velocity_mps.is_finite()
            && self.velocity_mps > 0.0
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct BodyStateReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub target_naif_id: i32,
    pub expected: ReferenceStateVector,
    pub budget: StateResidualBudget,
}

impl BodyStateReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.expected.position_m.is_finite()
            && self.expected.velocity_mps.is_finite()
            && self.budget.is_valid()
        {
            return Ok(());
        }
        Err(ScientificReferenceCaseError::InvalidPayload)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct ReferenceQuaternion {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub w: f64,
}

impl ReferenceQuaternion {
    fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite() && self.w.is_finite()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OrientationReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub target_naif_id: i32,
    pub inertial_to_body_fixed: ReferenceQuaternion,
    pub angular_velocity_inertial_rad_s: ReferenceVector3,
    pub maximum_angular_residual_rad: f64,
}

impl OrientationReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.inertial_to_body_fixed.is_finite()
            && self.angular_velocity_inertial_rad_s.is_finite()
            && self.maximum_angular_residual_rad.is_finite()
            && self.maximum_angular_residual_rad > 0.0
        {
            return Ok(());
        }
        Err(ScientificReferenceCaseError::InvalidPayload)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct LaunchSiteReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub latitude_rad: f64,
    pub longitude_rad: f64,
    pub ellipsoidal_height_m: f64,
    pub expected_position_m: ReferenceVector3,
    pub maximum_position_residual_m: f64,
}

impl LaunchSiteReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.latitude_rad.is_finite()
            && self.longitude_rad.is_finite()
            && self.ellipsoidal_height_m.is_finite()
            && self.expected_position_m.is_finite()
            && self.maximum_position_residual_m.is_finite()
            && self.maximum_position_residual_m > 0.0
        {
            return Ok(());
        }
        Err(ScientificReferenceCaseError::InvalidPayload)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct SunDirectionReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub observer_naif_id: i32,
    pub expected_direction: ReferenceVector3,
    pub maximum_angular_residual_rad: f64,
}

impl SunDirectionReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.expected_direction.is_finite()
            && self.maximum_angular_residual_rad.is_finite()
            && self.maximum_angular_residual_rad > 0.0
        {
            return Ok(());
        }
        Err(ScientificReferenceCaseError::InvalidPayload)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct GravityReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub force_model: ScientificReferenceForceModel,
    pub vehicle_position_m: ReferenceVector3,
    pub expected_acceleration_mps2: ReferenceVector3,
    pub maximum_acceleration_residual_mps2: f64,
}

impl GravityReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if self.vehicle_position_m.is_finite()
            && self.expected_acceleration_mps2.is_finite()
            && self.maximum_acceleration_residual_mps2.is_finite()
            && self.maximum_acceleration_residual_mps2 > 0.0
        {
            return Ok(());
        }
        Err(ScientificReferenceCaseError::InvalidPayload)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum ScientificReferenceForceModel {
    TwoBody,
    EarthJ2,
    EarthMoonSun,
    PlanetSun,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct PropagationReferenceCase {
    pub header: ScientificReferenceCaseHeader,
    pub force_model: ScientificReferenceForceModel,
    pub initial_state: ReferenceStateVector,
    pub checkpoints: Vec<PropagationReferenceCheckpoint>,
}

impl PropagationReferenceCase {
    fn validate_payload(&self) -> Result<(), ScientificReferenceCaseError> {
        if !self.initial_state.position_m.is_finite()
            || !self.initial_state.velocity_mps.is_finite()
            || self.checkpoints.is_empty()
        {
            return Err(ScientificReferenceCaseError::InvalidPayload);
        }
        let mut previous_offset_s = 0.0;
        for checkpoint in &self.checkpoints {
            if !checkpoint.is_valid() || checkpoint.offset_s <= previous_offset_s {
                return Err(ScientificReferenceCaseError::InvalidPayload);
            }
            previous_offset_s = checkpoint.offset_s;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
pub struct PropagationReferenceCheckpoint {
    pub offset_s: f64,
    pub expected: ReferenceStateVector,
    pub budget: StateResidualBudget,
}

impl PropagationReferenceCheckpoint {
    fn is_valid(self) -> bool {
        self.offset_s.is_finite()
            && self.offset_s > 0.0
            && self.expected.position_m.is_finite()
            && self.expected.velocity_mps.is_finite()
            && self.budget.is_valid()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScientificReferenceCaseError {
    UnsupportedFormatVersion { actual: u32 },
    DuplicateCaseId,
    InvalidMetadata,
    InvalidSourceUrl,
    InvalidPayload,
}
