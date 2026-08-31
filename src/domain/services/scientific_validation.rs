//! Versioned, offline scientific-reference case contracts.
//!
//! Cases record externally generated values and their provenance. This module
//! only validates the data contract; evaluating cases belongs to the offline
//! scientific-validation runner.

use std::collections::HashSet;
use std::path::Path;

use bevy::math::DVec3;
use serde::{Deserialize, Serialize};

use crate::domain::services::ephemeris::{NaifBodyId, SpiceEphemeris, TdbEpoch};
use crate::domain::services::gravity::{
    gravitational_acceleration_from_mu, ForceModelConfig, ForceModelTier,
};
use crate::domain::services::long_arc_propagation::{
    LongArcIntegrationSettings, LongArcPropagationRequest, LongArcState, TwoBodyAccelerationModel,
};
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::{
    geodetic_to_body_fixed, terrain_body_fixed_to_iau_body_fixed,
};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;

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
/// offline ephemeris owner.
pub trait ScientificStateAuthority {
    fn state(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<ReferenceStateVector, String>;

    fn orientation(
        &self,
        _: NaifBodyId,
        _: TdbEpoch,
    ) -> Result<(ReferenceQuaternion, ReferenceVector3), String> {
        Err("the configured authority cannot evaluate body orientation".to_string())
    }

    fn gravitational_parameter_m3_s2(&self, _: NaifBodyId) -> Result<f64, String> {
        Err("the configured authority cannot evaluate gravitational parameters".to_string())
    }
}

impl ScientificStateAuthority for SpiceEphemeris {
    fn state(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<ReferenceStateVector, String> {
        let state = SpiceEphemeris::state(self, target, center, epoch)
            .map_err(|error| error.to_string())?;
        Ok(ReferenceStateVector {
            position_m: ReferenceVector3 {
                x: state.position_m.x,
                y: state.position_m.y,
                z: state.position_m.z,
            },
            velocity_mps: ReferenceVector3 {
                x: state.velocity_mps.x,
                y: state.velocity_mps.y,
                z: state.velocity_mps.z,
            },
        })
    }

    fn orientation(
        &self,
        target: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<(ReferenceQuaternion, ReferenceVector3), String> {
        let orientation =
            SpiceEphemeris::orientation(self, target, epoch).map_err(|error| error.to_string())?;
        Ok((
            ReferenceQuaternion {
                x: orientation.inertial_to_body_fixed.x,
                y: orientation.inertial_to_body_fixed.y,
                z: orientation.inertial_to_body_fixed.z,
                w: orientation.inertial_to_body_fixed.w,
            },
            ReferenceVector3 {
                x: orientation.angular_velocity_inertial_rad_s.x,
                y: orientation.angular_velocity_inertial_rad_s.y,
                z: orientation.angular_velocity_inertial_rad_s.z,
            },
        ))
    }

    fn gravitational_parameter_m3_s2(&self, target: NaifBodyId) -> Result<f64, String> {
        SpiceEphemeris::gravitational_parameter_m3_s2(self, target)
            .map_err(|error| error.to_string())
    }
}

/// Offline evaluator for one versioned scientific-reference case set.
pub struct ScientificValidationRunner<Authority> {
    authority: Authority,
}

impl ScientificValidationRunner<SpiceEphemeris> {
    pub fn load_ephemeris_manifest(manifest_path: impl AsRef<Path>) -> Result<Self, String> {
        SpiceEphemeris::load(manifest_path)
            .map(Self::new)
            .map_err(|error| error.to_string())
    }
}

impl<Authority> ScientificValidationRunner<Authority>
where
    Authority: ScientificStateAuthority,
{
    pub const fn new(authority: Authority) -> Self {
        Self { authority }
    }

    pub fn validate(
        &self,
        cases: &ScientificReferenceCaseSet,
    ) -> Result<ScientificValidationReport, ScientificReferenceCaseError> {
        cases.validate()?;
        Ok(ScientificValidationReport {
            cases: cases
                .cases
                .iter()
                .map(|case| self.validate_case(case))
                .collect(),
        })
    }

    fn validate_case(&self, case: &ScientificReferenceCase) -> ScientificValidationCaseResult {
        let case_id = case.header().metadata.id.clone();
        match case {
            ScientificReferenceCase::BodyState(case) => self.validate_body_state(case_id, case),
            ScientificReferenceCase::Orientation(case) => self.validate_orientation(case_id, case),
            ScientificReferenceCase::LaunchSite(case) => self.validate_launch_site(case_id, case),
            ScientificReferenceCase::SunDirection(case) => {
                self.validate_sun_direction(case_id, case)
            }
            ScientificReferenceCase::Gravity(case) => self.validate_gravity(case_id, case),
            ScientificReferenceCase::Propagation(case) => self.validate_propagation(case_id, case),
        }
    }

    fn scalar_residual(
        case_id: ScientificReferenceCaseId,
        actual: f64,
        maximum: f64,
        detail: impl Into<String>,
    ) -> ScientificValidationCaseResult {
        let residual = ScientificValidationResidual {
            position_m: actual,
            velocity_mps: 0.0,
            budget: StateResidualBudget {
                position_m: maximum,
                velocity_mps: f64::MAX,
            },
        };
        ScientificValidationCaseResult {
            case_id,
            status: if actual <= maximum {
                ScientificValidationStatus::Passed
            } else {
                ScientificValidationStatus::Failed
            },
            residual: Some(residual),
            detail: detail.into(),
        }
    }

    fn validate_orientation(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &OrientationReferenceCase,
    ) -> ScientificValidationCaseResult {
        let Ok(epoch) = TdbEpoch::from_julian_date(case.header.julian_date) else {
            return ScientificValidationCaseResult::unverified(case_id, "the TDB epoch is invalid");
        };
        let actual = match self
            .authority
            .orientation(NaifBodyId::new(case.target_naif_id), epoch)
        {
            Ok(actual) => actual,
            Err(error) => return ScientificValidationCaseResult::unverified(case_id, error),
        };
        let expected = case.inertial_to_body_fixed;
        let quaternion_dot = (actual.0.x * expected.x
            + actual.0.y * expected.y
            + actual.0.z * expected.z
            + actual.0.w * expected.w)
            .abs()
            .clamp(-1.0, 1.0);
        let rotation_residual_rad = 2.0 * quaternion_dot.acos();
        let angular_velocity_residual_rad_s = actual
            .1
            .as_dvec3()
            .distance(case.angular_velocity_inertial_rad_s.as_dvec3());
        Self::scalar_residual(
            case_id,
            rotation_residual_rad.max(angular_velocity_residual_rad_s),
            case.maximum_angular_residual_rad,
            "orientation residual (max of rotation radians and angular-velocity rad/s)",
        )
    }

    fn validate_launch_site(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &LaunchSiteReferenceCase,
    ) -> ScientificValidationCaseResult {
        if case.header.coordinate_system.frame != ScientificReferenceFrame::EarthFixed {
            return ScientificValidationCaseResult::unverified(
                case_id,
                "the launch-site case must use Earth-fixed coordinates",
            );
        }
        let Some(earth) = PlanetFactory::create_by_name("Earth") else {
            return ScientificValidationCaseResult::unverified(
                case_id,
                "Earth is unavailable in the planet catalog",
            );
        };
        let site = LaunchSiteCoordinates::new(
            CelestialBodyId::earth(),
            case.latitude_rad.to_degrees() as f32,
            case.longitude_rad.to_degrees() as f32,
            case.ellipsoidal_height_m as f32,
        );
        let actual = terrain_body_fixed_to_iau_body_fixed(geodetic_to_body_fixed(&site, &earth));
        Self::scalar_residual(
            case_id,
            actual.distance(case.expected_position_m.as_dvec3()),
            case.maximum_position_residual_m,
            "Earth-fixed launch-site position residual in meters",
        )
    }

    fn validate_sun_direction(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &SunDirectionReferenceCase,
    ) -> ScientificValidationCaseResult {
        let Ok(epoch) = TdbEpoch::from_julian_date(case.header.julian_date) else {
            return ScientificValidationCaseResult::unverified(case_id, "the TDB epoch is invalid");
        };
        let observer = NaifBodyId::new(case.observer_naif_id);
        let state = match self.authority.state(NaifBodyId::SUN, observer, epoch) {
            Ok(state) => state,
            Err(error) => return ScientificValidationCaseResult::unverified(case_id, error),
        };
        let actual = state.position_m.as_dvec3().normalize();
        let expected = case.expected_direction.as_dvec3().normalize();
        Self::scalar_residual(
            case_id,
            actual.dot(expected).clamp(-1.0, 1.0).acos(),
            case.maximum_angular_residual_rad,
            "Sun-direction angular residual in radians",
        )
    }

    fn validate_gravity(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &GravityReferenceCase,
    ) -> ScientificValidationCaseResult {
        if case.force_model != ScientificReferenceForceModel::TwoBody {
            return ScientificValidationCaseResult::unverified(
                case_id,
                "the configured runner only evaluates independent two-body gravity cases",
            );
        }
        let mu_m3_s2 = match self
            .authority
            .gravitational_parameter_m3_s2(NaifBodyId::EARTH)
        {
            Ok(value) => value,
            Err(error) => return ScientificValidationCaseResult::unverified(case_id, error),
        };
        let actual = gravitational_acceleration_from_mu(
            mu_m3_s2,
            case.vehicle_position_m.as_dvec3(),
            DVec3::ZERO,
        );
        Self::scalar_residual(
            case_id,
            actual.distance(case.expected_acceleration_mps2.as_dvec3()),
            case.maximum_acceleration_residual_mps2,
            "two-body gravitational-acceleration residual in m/s^2",
        )
    }

    fn validate_propagation(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &PropagationReferenceCase,
    ) -> ScientificValidationCaseResult {
        if case.force_model != ScientificReferenceForceModel::TwoBody {
            return ScientificValidationCaseResult::unverified(
                case_id,
                "the configured runner only evaluates independent two-body propagation cases",
            );
        }
        let Ok(epoch) = TdbEpoch::from_julian_date(case.header.julian_date) else {
            return ScientificValidationCaseResult::unverified(case_id, "the TDB epoch is invalid");
        };
        let mu_m3_s2 = match self
            .authority
            .gravitational_parameter_m3_s2(NaifBodyId::EARTH)
        {
            Ok(value) => value,
            Err(error) => return ScientificValidationCaseResult::unverified(case_id, error),
        };
        let offsets: Vec<_> = case
            .checkpoints
            .iter()
            .map(|checkpoint| checkpoint.offset_s)
            .collect();
        let horizon_s = offsets.last().copied().unwrap_or_default();
        let request = match LongArcPropagationRequest::new(
            LongArcState::new(
                case.initial_state.position_m.as_dvec3(),
                case.initial_state.velocity_mps.as_dvec3(),
            ),
            epoch,
            NaifBodyId::EARTH,
            ForceModelConfig::new(ForceModelTier::TwoBody),
            LongArcIntegrationSettings {
                relative_tolerance: 1.0e-12,
                absolute_position_tolerance_m: 1.0e-6,
                absolute_velocity_tolerance_mps: 1.0e-9,
                maximum_step_s: 1.0,
                ..LongArcIntegrationSettings::default()
            },
            horizon_s,
            offsets,
        ) {
            Ok(request) => request,
            Err(error) => {
                return ScientificValidationCaseResult::unverified(case_id, format!("{error:?}"))
            }
        };
        let model = match TwoBodyAccelerationModel::new(mu_m3_s2) {
            Ok(model) => model,
            Err(error) => {
                return ScientificValidationCaseResult::unverified(case_id, format!("{error:?}"))
            }
        };
        let result = match request.propagate_with(&model) {
            Ok(result) => result,
            Err(error) => {
                return ScientificValidationCaseResult::unverified(case_id, format!("{error:?}"))
            }
        };
        let (position_m, velocity_mps, budget) =
            result.checkpoints.iter().zip(&case.checkpoints).fold(
                (
                    0.0_f64,
                    0.0_f64,
                    StateResidualBudget {
                        position_m: f64::MAX,
                        velocity_mps: f64::MAX,
                    },
                ),
                |(max_position, max_velocity, _), (actual, expected)| {
                    (
                        max_position.max(
                            actual
                                .state
                                .position_m
                                .distance(expected.expected.position_m.as_dvec3()),
                        ),
                        max_velocity.max(
                            actual
                                .state
                                .velocity_mps
                                .distance(expected.expected.velocity_mps.as_dvec3()),
                        ),
                        expected.budget,
                    )
                },
            );
        let residual = ScientificValidationResidual {
            position_m,
            velocity_mps,
            budget,
        };
        ScientificValidationCaseResult {
            case_id,
            status: if residual.within_budget() {
                ScientificValidationStatus::Passed
            } else {
                ScientificValidationStatus::Failed
            },
            residual: Some(residual),
            detail: "two-body long-arc maximum checkpoint residual".to_string(),
        }
    }

    fn validate_body_state(
        &self,
        case_id: ScientificReferenceCaseId,
        case: &BodyStateReferenceCase,
    ) -> ScientificValidationCaseResult {
        let coordinate_system = case.header.coordinate_system;
        if coordinate_system.frame != ScientificReferenceFrame::SsbIcrfJ2000
            || coordinate_system.time_scale != ScientificReferenceTimeScale::Tdb
            || coordinate_system.units != ScientificReferenceUnits::SiMetersSeconds
        {
            return ScientificValidationCaseResult::unverified(
                case_id,
                "the body-state case is outside the runner's SSB/ICRF/TDB/SI contract",
            );
        }
        let center = match coordinate_system.center {
            ScientificReferenceCenter::SolarSystemBarycenter => NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            ScientificReferenceCenter::NaifBody(center_naif_id) => NaifBodyId::new(center_naif_id),
            ScientificReferenceCenter::NotApplicable => {
                return ScientificValidationCaseResult::unverified(
                    case_id,
                    "the body-state case must identify a NAIF center",
                );
            }
        };
        let Ok(epoch) = TdbEpoch::from_julian_date(case.header.julian_date) else {
            return ScientificValidationCaseResult::unverified(case_id, "the TDB epoch is invalid");
        };
        let actual = match self
            .authority
            .state(NaifBodyId::new(case.target_naif_id), center, epoch)
        {
            Ok(state) => state,
            Err(error) => return ScientificValidationCaseResult::unverified(case_id, error),
        };
        let residual = ScientificValidationResidual {
            position_m: actual
                .position_m
                .as_dvec3()
                .distance(case.expected.position_m.as_dvec3()),
            velocity_mps: actual
                .velocity_mps
                .as_dvec3()
                .distance(case.expected.velocity_mps.as_dvec3()),
            budget: case.budget,
        };
        let status = if residual.within_budget() {
            ScientificValidationStatus::Passed
        } else {
            ScientificValidationStatus::Failed
        };
        ScientificValidationCaseResult {
            case_id,
            status,
            residual: Some(residual),
            detail: "body-state residual evaluated against the declared external case".to_string(),
        }
    }
}

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
    fn unverified(case_id: ScientificReferenceCaseId, detail: impl Into<String>) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn header(id: &str) -> ScientificReferenceCaseHeader {
        ScientificReferenceCaseHeader {
            metadata: ScientificReferenceMetadata {
                id: ScientificReferenceCaseId::new(id),
                source: ScientificReferenceSource {
                    provider: ScientificReferenceProvider::JplHorizons,
                    url: "https://ssd.jpl.nasa.gov/horizons/".to_string(),
                    source_version: "DE441".to_string(),
                },
                generation_command: "horizons_batch --vectors".to_string(),
                datasets: vec![ScientificReferenceDataset {
                    role: ScientificReferenceDatasetRole::Ephemeris,
                    identifier: "de441".to_string(),
                    version: "DE441".to_string(),
                    sha256: None,
                }],
            },
            coordinate_system: ScientificReferenceCoordinateSystem {
                frame: ScientificReferenceFrame::SsbIcrfJ2000,
                center: ScientificReferenceCenter::SolarSystemBarycenter,
                time_scale: ScientificReferenceTimeScale::Tdb,
                units: ScientificReferenceUnits::SiMetersSeconds,
            },
            julian_date: 2_451_545.0,
        }
    }

    fn body_state_case(id: &str) -> ScientificReferenceCase {
        ScientificReferenceCase::BodyState(BodyStateReferenceCase {
            header: header(id),
            target_naif_id: 399,
            expected: ReferenceStateVector {
                position_m: ReferenceVector3 {
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                velocity_mps: ReferenceVector3 {
                    x: 4.0,
                    y: 5.0,
                    z: 6.0,
                },
            },
            budget: StateResidualBudget {
                position_m: 1.0,
                velocity_mps: 1.0e-3,
            },
        })
    }

    struct MockStateAuthority {
        state: ReferenceStateVector,
    }

    impl ScientificStateAuthority for MockStateAuthority {
        fn state(
            &self,
            _: NaifBodyId,
            _: NaifBodyId,
            _: TdbEpoch,
        ) -> Result<ReferenceStateVector, String> {
            Ok(self.state)
        }
    }

    #[test]
    fn versioned_case_set_accepts_complete_typed_provenance() {
        let cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![body_state_case("earth-ssb-j2000")],
        };

        assert_eq!(cases.validate(), Ok(()));
        let encoded = ron::ser::to_string(&cases).expect("reference cases should serialize to RON");
        let decoded: ScientificReferenceCaseSet =
            ron::from_str(&encoded).expect("serialized reference cases should deserialize");
        assert_eq!(decoded, cases);
    }

    #[test]
    fn case_set_rejects_unknown_versions_duplicate_ids_and_incomplete_provenance() {
        let unknown_version = ScientificReferenceCaseSet {
            format_version: 2,
            cases: vec![],
        };
        assert_eq!(
            unknown_version.validate(),
            Err(ScientificReferenceCaseError::UnsupportedFormatVersion { actual: 2 })
        );

        let duplicate_ids = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![body_state_case("same"), body_state_case("same")],
        };
        assert_eq!(
            duplicate_ids.validate(),
            Err(ScientificReferenceCaseError::DuplicateCaseId)
        );

        let mut incomplete = body_state_case("incomplete");
        let ScientificReferenceCase::BodyState(incomplete_state) = &mut incomplete else {
            unreachable!("fixture is a body-state case");
        };
        incomplete_state.header.metadata.id = ScientificReferenceCaseId::new("");
        let incomplete = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![incomplete],
        };
        assert_eq!(
            incomplete.validate(),
            Err(ScientificReferenceCaseError::InvalidMetadata)
        );
    }

    #[test]
    fn recorded_cases_are_machine_readable_and_validated() {
        let source = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/configs/scientific_validation/reference_cases_v1.ron"
        ))
        .expect("recorded reference cases should exist");
        let cases: ScientificReferenceCaseSet =
            ron::from_str(&source).expect("recorded reference cases should deserialize");

        assert_eq!(cases.validate(), Ok(()));
        assert_eq!(cases.cases.len(), 6);
        assert!(matches!(
            cases.cases[0],
            ScientificReferenceCase::BodyState(_)
        ));
        assert!(matches!(
            cases.cases[1],
            ScientificReferenceCase::Orientation(_)
        ));
        assert!(matches!(
            cases.cases[2],
            ScientificReferenceCase::LaunchSite(_)
        ));
        assert!(matches!(
            cases.cases[3],
            ScientificReferenceCase::SunDirection(_)
        ));
        assert!(matches!(
            cases.cases[4],
            ScientificReferenceCase::Gravity(_)
        ));
        assert!(matches!(
            cases.cases[5],
            ScientificReferenceCase::Propagation(_)
        ));
    }

    #[test]
    fn runner_reports_body_state_passes_and_failures_in_physical_units() {
        let passing_case = body_state_case("passing");
        let ScientificReferenceCase::BodyState(expected_case) = &passing_case else {
            unreachable!("fixture is a body-state case");
        };
        let runner = ScientificValidationRunner::new(MockStateAuthority {
            state: expected_case.expected,
        });
        let passing_cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![passing_case],
        };
        let passing_report = runner.validate(&passing_cases).unwrap();
        assert_eq!(passing_report.passed(), 1);
        assert!(passing_report.is_verified());

        let mut failing_case = body_state_case("failing");
        let ScientificReferenceCase::BodyState(failing_state) = &mut failing_case else {
            unreachable!("fixture is a body-state case");
        };
        failing_state.expected.position_m.x += failing_state.budget.position_m * 2.0;
        let failing_cases = ScientificReferenceCaseSet {
            format_version: SCIENTIFIC_REFERENCE_FORMAT_VERSION,
            cases: vec![failing_case],
        };
        let failing_report = runner.validate(&failing_cases).unwrap();
        assert_eq!(failing_report.failed(), 1);
        assert!(!failing_report.is_verified());
    }
}
