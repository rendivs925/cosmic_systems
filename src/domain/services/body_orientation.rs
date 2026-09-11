//! Kernel-backed body-fixed orientation at a TDB epoch.

use crate::domain::math::{DQuat, DVec3};
use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};

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
