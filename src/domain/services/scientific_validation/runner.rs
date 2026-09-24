//! Offline scientific-validation evaluator: the deterministic state-authority
//! contract and the runner that checks reference cases against it.

use super::cases::*;
use crate::domain::math::DVec3;
use std::path::Path;

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
