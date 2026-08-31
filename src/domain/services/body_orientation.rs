//! Versioned authority for body-fixed orientation at a TDB epoch.
//!
//! This module owns orientation-model evaluation only. `reference_frames` owns
//! coordinate conversion and will consume this authority when high-fidelity
//! consumers migrate from catalog spin in task 3.3.

use crate::domain::entities::planet::Planet;
use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
use bevy::math::{DQuat, DVec3};
use std::collections::HashSet;
use std::fmt;

/// Inertial axes used by an orientation model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrientationInertialFrame {
    /// The project's existing J2000 ecliptic flight/display axes.
    ProjectSolarInertialJ2000Ecliptic,
    /// ICRF/J2000 axes used by NAIF PCK/BPC models.
    IcrfJ2000,
}

/// The rotating axes an orientation model maps into an inertial frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrientationBodyFixedFrame {
    /// Legacy catalog body-fixed axes, retained only as an explicit approximation.
    CatalogBodyFixed,
    /// NAIF IAU body-fixed axes.
    IauBodyFixed,
}

/// Time scale used to evaluate an orientation model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrientationTimeScale {
    Tdb,
}

/// Source category for an orientation model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrientationDataSource {
    /// Rotation period and axial tilt from the visual/catalog model.
    CatalogApproximation,
    /// A validated local PCK or BPC dataset.
    Kernel,
}

/// Pole definition used by an orientation model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PoleModel {
    CatalogAxialTilt,
    KernelDefined,
}

/// Prime-meridian direction convention.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrimeMeridianConvention {
    /// The +X body-fixed axis is longitude zero and +Z is positive east.
    PositiveEast,
}

/// Provenance that makes an orientation model's scientific status explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrientationModelProvenance {
    pub version: String,
    pub source: OrientationDataSource,
    pub inertial_frame: OrientationInertialFrame,
    pub body_fixed_frame: OrientationBodyFixedFrame,
    pub time_scale: OrientationTimeScale,
    pub pole_model: PoleModel,
    pub prime_meridian: PrimeMeridianConvention,
}

/// Immutable uniform-spin orientation model evaluated from TDB seconds since
/// J2000. It is also the common output shape for a future kernel-backed model.
#[derive(Clone, Debug)]
pub struct BodyOrientationModel {
    pub target: NaifBodyId,
    pub provenance: OrientationModelProvenance,
    body_fixed_to_inertial_at_j2000: DQuat,
    angular_velocity_rad_s: f64,
}

impl BodyOrientationModel {
    /// Build an explicitly approximate model from the existing catalog fields.
    /// This preserves catalog rotation exactly while retaining provenance that
    /// prevents it from being mistaken for PCK/BPC orientation data.
    pub fn from_catalog_approximation(
        target: NaifBodyId,
        planet: &Planet,
        version: impl Into<String>,
    ) -> Result<Self, OrientationError> {
        let period_s = planet.rotation_period_hours as f64 * 3_600.0;
        if !period_s.is_finite() || period_s == 0.0 {
            return Err(OrientationError::InvalidRotationPeriod { target, period_s });
        }

        let axial_tilt_rad = (planet.axial_tilt_deg as f64).to_radians();
        if !axial_tilt_rad.is_finite() {
            return Err(OrientationError::InvalidReferenceRotation { target });
        }

        let version = version.into();
        if version.trim().is_empty() {
            return Err(OrientationError::EmptyVersion);
        }

        Ok(Self {
            target,
            provenance: OrientationModelProvenance {
                version,
                source: OrientationDataSource::CatalogApproximation,
                inertial_frame: OrientationInertialFrame::ProjectSolarInertialJ2000Ecliptic,
                body_fixed_frame: OrientationBodyFixedFrame::CatalogBodyFixed,
                time_scale: OrientationTimeScale::Tdb,
                pole_model: PoleModel::CatalogAxialTilt,
                prime_meridian: PrimeMeridianConvention::PositiveEast,
            },
            body_fixed_to_inertial_at_j2000: DQuat::from_rotation_z(axial_tilt_rad),
            angular_velocity_rad_s: std::f64::consts::TAU / period_s,
        })
    }

    fn evaluate(&self, epoch: TdbEpoch) -> BodyOrientation {
        let phase_rad = (self.angular_velocity_rad_s * epoch.seconds_since_j2000())
            .rem_euclid(std::f64::consts::TAU);
        let body_fixed_to_inertial =
            self.body_fixed_to_inertial_at_j2000 * DQuat::from_rotation_y(phase_rad);
        let spin_axis_inertial = self.body_fixed_to_inertial_at_j2000 * DVec3::Y;

        BodyOrientation {
            target: self.target,
            epoch,
            provenance: self.provenance.clone(),
            inertial_to_body_fixed: body_fixed_to_inertial.inverse(),
            body_fixed_to_inertial,
            angular_velocity_inertial_rad_s: spin_axis_inertial * self.angular_velocity_rad_s,
        }
    }
}

/// Pure, immutable orientation authority for the selected scientific dataset.
#[derive(Clone, Debug)]
pub struct BodyOrientationAuthority {
    version: String,
    models: Vec<BodyOrientationModel>,
}

impl BodyOrientationAuthority {
    pub fn new(
        version: impl Into<String>,
        models: Vec<BodyOrientationModel>,
    ) -> Result<Self, OrientationError> {
        let version = version.into();
        if version.trim().is_empty() {
            return Err(OrientationError::EmptyVersion);
        }

        let mut targets = HashSet::with_capacity(models.len());
        for model in &models {
            if !targets.insert(model.target) {
                return Err(OrientationError::DuplicateModel {
                    target: model.target,
                });
            }
        }

        Ok(Self { version, models })
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub fn evaluate(
        &self,
        target: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<BodyOrientation, OrientationError> {
        let model = self
            .models
            .iter()
            .find(|model| model.target == target)
            .ok_or(OrientationError::UnsupportedBody { target })?;
        Ok(model.evaluate(epoch))
    }
}

/// Orientation at one shared TDB epoch.
#[derive(Clone, Debug)]
pub struct BodyOrientation {
    pub target: NaifBodyId,
    pub epoch: TdbEpoch,
    pub provenance: OrientationModelProvenance,
    /// Rotation from the declared inertial frame into the declared body-fixed frame.
    pub inertial_to_body_fixed: DQuat,
    /// Inverse of [`Self::inertial_to_body_fixed`].
    pub body_fixed_to_inertial: DQuat,
    /// Body angular velocity expressed in the declared inertial frame, radians per second.
    pub angular_velocity_inertial_rad_s: DVec3,
}

impl BodyOrientation {
    pub(crate) fn from_kernel(
        target: NaifBodyId,
        epoch: TdbEpoch,
        version: String,
        inertial_to_body_fixed: DQuat,
        angular_velocity_inertial_rad_s: DVec3,
    ) -> Self {
        Self {
            target,
            epoch,
            provenance: OrientationModelProvenance {
                version,
                source: OrientationDataSource::Kernel,
                inertial_frame: OrientationInertialFrame::IcrfJ2000,
                body_fixed_frame: OrientationBodyFixedFrame::IauBodyFixed,
                time_scale: OrientationTimeScale::Tdb,
                pole_model: PoleModel::KernelDefined,
                prime_meridian: PrimeMeridianConvention::PositiveEast,
            },
            inertial_to_body_fixed,
            body_fixed_to_inertial: inertial_to_body_fixed.inverse(),
            angular_velocity_inertial_rad_s,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum OrientationError {
    EmptyVersion,
    DuplicateModel { target: NaifBodyId },
    UnsupportedBody { target: NaifBodyId },
    InvalidRotationPeriod { target: NaifBodyId, period_s: f64 },
    InvalidReferenceRotation { target: NaifBodyId },
}

impl fmt::Display for OrientationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyVersion => {
                formatter.write_str("orientation authority version must not be empty")
            }
            Self::DuplicateModel { target } => {
                write!(
                    formatter,
                    "duplicate orientation model for NAIF {}",
                    target.value()
                )
            }
            Self::UnsupportedBody { target } => {
                write!(
                    formatter,
                    "no orientation model for NAIF {}",
                    target.value()
                )
            }
            Self::InvalidRotationPeriod { target, period_s } => write!(
                formatter,
                "invalid rotation period {period_s} seconds for NAIF {}",
                target.value()
            ),
            Self::InvalidReferenceRotation { target } => {
                write!(
                    formatter,
                    "invalid reference rotation for NAIF {}",
                    target.value()
                )
            }
        }
    }
}

impl std::error::Error for OrientationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::physics_utils::calculate_planet_rotation_f64;
    use crate::domain::services::planet_factory::PlanetFactory;

    fn earth_model() -> BodyOrientationModel {
        BodyOrientationModel::from_catalog_approximation(
            NaifBodyId::EARTH,
            &PlanetFactory::create_by_name("Earth").unwrap(),
            "catalog-orientation-v1",
        )
        .unwrap()
    }

    #[test]
    fn catalog_model_preserves_the_existing_body_fixed_rotation() {
        let earth = PlanetFactory::create_by_name("Earth").unwrap();
        let authority =
            BodyOrientationAuthority::new("catalog-orientation-v1", vec![earth_model()]).unwrap();
        let epoch = TdbEpoch::from_seconds_since_j2000(12.5 * 86_400.0).unwrap();

        let orientation = authority.evaluate(NaifBodyId::EARTH, epoch).unwrap();
        let expected = DQuat::from_rotation_z((earth.axial_tilt_deg as f64).to_radians())
            * DQuat::from_rotation_y(calculate_planet_rotation_f64(&earth, 12.5));

        assert!(orientation.body_fixed_to_inertial.dot(expected).abs() > 1.0 - 1.0e-12);
        assert_eq!(
            orientation.provenance.source,
            OrientationDataSource::CatalogApproximation
        );
        assert_eq!(
            orientation.provenance.pole_model,
            PoleModel::CatalogAxialTilt
        );
    }

    #[test]
    fn orientation_round_trips_and_reports_angular_velocity_in_inertial_axes() {
        let authority =
            BodyOrientationAuthority::new("catalog-orientation-v1", vec![earth_model()]).unwrap();
        let orientation = authority
            .evaluate(
                NaifBodyId::EARTH,
                TdbEpoch::from_seconds_since_j2000(1_234_567.0).unwrap(),
            )
            .unwrap();
        let body_fixed = DVec3::new(2.0, -3.0, 5.0);

        let round_trip =
            orientation.inertial_to_body_fixed * (orientation.body_fixed_to_inertial * body_fixed);
        assert!((round_trip - body_fixed).length() < 1.0e-12);
        assert!(
            (orientation.angular_velocity_inertial_rad_s.length()
                - std::f64::consts::TAU / (23.934 * 3_600.0))
                .abs()
                < 1.0e-12
        );
        assert!(
            orientation
                .angular_velocity_inertial_rad_s
                .dot(orientation.body_fixed_to_inertial * DVec3::X)
                .abs()
                < 1.0e-16
        );
    }

    #[test]
    fn authority_rejects_duplicate_and_unsupported_models() {
        assert!(matches!(
            BodyOrientationAuthority::new(
                "catalog-orientation-v1",
                vec![earth_model(), earth_model()]
            ),
            Err(OrientationError::DuplicateModel {
                target: NaifBodyId::EARTH
            })
        ));

        let authority =
            BodyOrientationAuthority::new("catalog-orientation-v1", vec![earth_model()]).unwrap();
        assert!(matches!(
            authority.evaluate(NaifBodyId::MOON, TdbEpoch::j2000()),
            Err(OrientationError::UnsupportedBody {
                target: NaifBodyId::MOON
            })
        ));
    }
}
