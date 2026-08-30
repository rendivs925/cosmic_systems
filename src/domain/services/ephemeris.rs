//! Offline, kernel-backed solar-system body states.
//!
//! This module is the only domain boundary allowed to load and evaluate the
//! curated NAIF SPICE/JPL DE kernels. It exposes geometric f64 SI states in the
//! J2000 frame; rendering and local-flight conversions remain elsewhere.

use anise::constants::frames::SSB_J2000;
use anise::frames::Frame;
use anise::naif::kpl::parser::convert_tpc;
#[cfg(any(target_arch = "wasm32", test))]
use anise::naif::kpl::parser::{convert_tpc_items, parse_bytes};
#[cfg(any(target_arch = "wasm32", test))]
use anise::naif::kpl::tpc::TPCItem;
#[cfg(any(target_arch = "wasm32", test))]
use anise::naif::SPK;
use anise::prelude::Almanac;
use anise::time::Epoch;
use bevy::math::{DMat3, DQuat, DVec3};
use serde::Deserialize;
use sha2::{Digest, Sha256};
#[cfg(any(target_arch = "wasm32", test))]
use std::collections::HashMap;
use std::fmt;
use std::fs;
#[cfg(any(target_arch = "wasm32", test))]
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};

use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::gravity::EarthJ2GravityModel;

pub const J2000_JULIAN_DATE_TDB: f64 = 2_451_545.0;
const KILOMETERS_TO_METERS: f64 = 1_000.0;
const KILOMETERS_CUBED_TO_METERS_CUBED: f64 =
    KILOMETERS_TO_METERS * KILOMETERS_TO_METERS * KILOMETERS_TO_METERS;

/// A NAIF celestial-body identifier used by the ephemeris contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct NaifBodyId(i32);

impl NaifBodyId {
    pub const SOLAR_SYSTEM_BARYCENTER: Self = Self(0);
    pub const SUN: Self = Self(10);
    pub const MERCURY_BARYCENTER: Self = Self(1);
    pub const VENUS_BARYCENTER: Self = Self(2);
    pub const EARTH_MOON_BARYCENTER: Self = Self(3);
    pub const MARS_BARYCENTER: Self = Self(4);
    pub const JUPITER_BARYCENTER: Self = Self(5);
    pub const SATURN_BARYCENTER: Self = Self(6);
    pub const URANUS_BARYCENTER: Self = Self(7);
    pub const NEPTUNE_BARYCENTER: Self = Self(8);
    pub const PLUTO_BARYCENTER: Self = Self(9);
    pub const EARTH: Self = Self(399);
    pub const MOON: Self = Self(301);

    /// Catalog bodies whose translations are available from the provisioned
    /// DE440s authority. Other catalog moons remain explicit approximations
    /// until their required satellite kernels are added to the manifest.
    pub const KERNEL_BACKED_CATALOG_BODIES: [(&str, Self); 10] = [
        ("Sun", Self::SUN),
        ("Mercury", Self::MERCURY_BARYCENTER),
        ("Venus", Self::VENUS_BARYCENTER),
        ("Earth", Self::EARTH),
        ("Mars", Self::MARS_BARYCENTER),
        ("Jupiter", Self::JUPITER_BARYCENTER),
        ("Saturn", Self::SATURN_BARYCENTER),
        ("Uranus", Self::URANUS_BARYCENTER),
        ("Neptune", Self::NEPTUNE_BARYCENTER),
        ("Moon", Self::MOON),
    ];

    /// Iterate the translation targets for every kernel-backed catalog body.
    pub fn kernel_backed_catalog_targets() -> impl Iterator<Item = Self> {
        Self::KERNEL_BACKED_CATALOG_BODIES
            .into_iter()
            .map(|(_, target)| target)
    }

    /// Maps translation targets to the physical body's IAU orientation target.
    /// Major-planet translations use barycenters while PCK orientation uses the
    /// body's conventional NAIF identifier.
    pub const fn orientation_target(self) -> Option<Self> {
        let target = match self {
            Self::MERCURY_BARYCENTER => Self(199),
            Self::VENUS_BARYCENTER => Self(299),
            Self::MARS_BARYCENTER => Self(499),
            Self::JUPITER_BARYCENTER => Self(599),
            Self::SATURN_BARYCENTER => Self(699),
            Self::URANUS_BARYCENTER => Self(799),
            Self::NEPTUNE_BARYCENTER => Self(899),
            Self::PLUTO_BARYCENTER => Self(999),
            Self::SUN | Self::EARTH | Self::MOON => self,
            _ => return None,
        };
        Some(target)
    }

    /// The physical body's NAIF identifier used by `gm_de440.tpc`. Translation
    /// barycenters intentionally map to their corresponding physical bodies.
    pub const fn gravitational_parameter_target(self) -> Option<Self> {
        self.orientation_target()
    }

    /// Map the current celestial catalog's kernel-backed bodies to their NAIF
    /// identifiers. Unmapped catalog moons remain presentation-only until
    /// their required kernel coverage is added to the curated manifest.
    pub fn for_catalog_name(name: &str) -> Option<Self> {
        Self::KERNEL_BACKED_CATALOG_BODIES
            .iter()
            .find_map(|(catalog_name, target)| (*catalog_name == name).then_some(*target))
    }

    pub const fn new(value: i32) -> Self {
        Self(value)
    }

    pub const fn value(self) -> i32 {
        self.0
    }

    fn j2000_frame(self) -> Frame {
        Frame::from_ephem_j2000(self.0)
    }
}

/// A TDB Julian date used to evaluate an ephemeris state.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TdbEpoch {
    julian_date_tdb: f64,
}

impl TdbEpoch {
    pub fn from_julian_date(julian_date_tdb: f64) -> Result<Self, EphemerisError> {
        if !julian_date_tdb.is_finite() {
            return Err(EphemerisError::InvalidEpoch(julian_date_tdb));
        }
        Ok(Self { julian_date_tdb })
    }

    pub fn from_seconds_since_j2000(seconds: f64) -> Result<Self, EphemerisError> {
        if !seconds.is_finite() {
            return Err(EphemerisError::InvalidEpoch(seconds));
        }
        Self::from_julian_date(J2000_JULIAN_DATE_TDB + seconds / 86_400.0)
    }

    pub const fn j2000() -> Self {
        Self {
            julian_date_tdb: J2000_JULIAN_DATE_TDB,
        }
    }

    pub const fn julian_date(self) -> f64 {
        self.julian_date_tdb
    }

    pub fn seconds_since_j2000(self) -> f64 {
        (self.julian_date_tdb - J2000_JULIAN_DATE_TDB) * 86_400.0
    }

    fn anise_epoch(self) -> Epoch {
        // `from_jde_tdb` preserves the JD TDB convention at this boundary;
        // `from_tdb_seconds` has a distinct documented zero-time convention.
        Epoch::from_jde_tdb(self.julian_date_tdb)
    }
}

/// A geometric ICRF/J2000 state in SI units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyState {
    pub target: NaifBodyId,
    pub center: NaifBodyId,
    pub epoch: TdbEpoch,
    pub position_m: DVec3,
    pub velocity_mps: DVec3,
}

impl BodyState {
    fn from_anise(
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
        position_km: [f64; 3],
        velocity_km_s: [f64; 3],
    ) -> Result<Self, EphemerisError> {
        let position_m = DVec3::from_array(position_km) * KILOMETERS_TO_METERS;
        let velocity_mps = DVec3::from_array(velocity_km_s) * KILOMETERS_TO_METERS;
        if !position_m.is_finite() || !velocity_mps.is_finite() {
            return Err(EphemerisError::NonFiniteState { target, center });
        }
        Ok(Self {
            target,
            center,
            epoch,
            position_m,
            velocity_mps,
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct KernelManifest {
    pub id: String,
    pub kernel_root: PathBuf,
    pub coverage: KernelCoverage,
    pub kernels: Vec<KernelFile>,
    #[serde(default)]
    pub unavailable_roles: Vec<ScientificDatasetRole>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct KernelCoverage {
    pub start_julian_date_tdb: f64,
    pub end_julian_date_tdb: f64,
}

impl KernelCoverage {
    pub fn contains(self, epoch: TdbEpoch) -> bool {
        (self.start_julian_date_tdb..=self.end_julian_date_tdb).contains(&epoch.julian_date())
    }

    fn is_valid(self) -> bool {
        self.start_julian_date_tdb.is_finite()
            && self.end_julian_date_tdb.is_finite()
            && self.start_julian_date_tdb <= self.end_julian_date_tdb
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct KernelFile {
    pub file_name: String,
    pub role: ScientificDatasetRole,
    pub kind: KernelKind,
    pub sha256: String,
    pub expected_size_bytes: u64,
    pub source_url: String,
    pub coverage: Option<ScientificDatasetCoverage>,
    pub frame: ScientificDatasetFrame,
    pub time_scale: ScientificDatasetTimeScale,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Hash)]
pub enum ScientificDatasetRole {
    Translation,
    LeapSeconds,
    Orientation,
    GravitationalParameters,
    GravityHarmonics,
    EarthOrientation,
}

impl fmt::Display for ScientificDatasetRole {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role = match self {
            Self::Translation => "translation",
            Self::LeapSeconds => "leap-second",
            Self::Orientation => "orientation",
            Self::GravitationalParameters => "gravitational-parameter",
            Self::GravityHarmonics => "gravity-harmonic",
            Self::EarthOrientation => "Earth-orientation",
        };
        formatter.write_str(role)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum KernelKind {
    Spk,
    TextPck,
    LeapSeconds,
    EarthOrientation,
    GravityModel,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct ScientificDatasetCoverage {
    pub start_julian_date: f64,
    pub end_julian_date: f64,
}

impl ScientificDatasetCoverage {
    fn is_valid(self) -> bool {
        self.start_julian_date.is_finite()
            && self.end_julian_date.is_finite()
            && self.start_julian_date <= self.end_julian_date
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ScientificDatasetFrame {
    SsbIcrfJ2000,
    IauBodyFixed,
    EarthFixed,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum ScientificDatasetTimeScale {
    Tdb,
    Utc,
    Tai,
    Tt,
    Ut1,
    NotApplicable,
}

impl KernelFile {
    fn metadata_is_valid(&self) -> bool {
        if self.coverage.is_some_and(|coverage| !coverage.is_valid()) {
            return false;
        }

        match self.role {
            ScientificDatasetRole::Translation => {
                self.kind == KernelKind::Spk
                    && self.coverage.is_some()
                    && self.frame == ScientificDatasetFrame::SsbIcrfJ2000
                    && self.time_scale == ScientificDatasetTimeScale::Tdb
            }
            ScientificDatasetRole::LeapSeconds => {
                self.kind == KernelKind::LeapSeconds
                    && self.coverage.is_some()
                    && self.frame == ScientificDatasetFrame::NotApplicable
                    && self.time_scale == ScientificDatasetTimeScale::Utc
            }
            ScientificDatasetRole::Orientation => {
                self.kind == KernelKind::TextPck
                    && self.coverage.is_some()
                    && self.frame == ScientificDatasetFrame::IauBodyFixed
                    && self.time_scale == ScientificDatasetTimeScale::Tdb
            }
            ScientificDatasetRole::GravitationalParameters => {
                self.kind == KernelKind::TextPck
                    && self.coverage.is_none()
                    && self.frame == ScientificDatasetFrame::NotApplicable
                    && self.time_scale == ScientificDatasetTimeScale::NotApplicable
            }
            ScientificDatasetRole::GravityHarmonics => {
                self.kind == KernelKind::GravityModel
                    && self.coverage.is_none()
                    && self.frame == ScientificDatasetFrame::NotApplicable
                    && self.time_scale == ScientificDatasetTimeScale::NotApplicable
            }
            ScientificDatasetRole::EarthOrientation => {
                self.kind == KernelKind::EarthOrientation
                    && self.coverage.is_some()
                    && self.frame == ScientificDatasetFrame::EarthFixed
                    && self.time_scale == ScientificDatasetTimeScale::Ut1
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KernelProvenance {
    pub manifest_id: String,
    pub manifest_path: PathBuf,
    pub coverage: KernelCoverage,
    pub validated_kernels: Vec<ValidatedKernel>,
    pub unavailable_roles: Vec<ScientificDatasetRole>,
}

impl KernelProvenance {
    pub fn dataset_statuses_at_tdb(&self, epoch: TdbEpoch) -> Vec<ScientificDatasetStatus> {
        let mut statuses: Vec<_> = self
            .validated_kernels
            .iter()
            .map(|dataset| ScientificDatasetStatus {
                role: dataset.role,
                file_name: Some(dataset.file_name.clone()),
                availability: match (dataset.time_scale, dataset.coverage) {
                    (ScientificDatasetTimeScale::Tdb, Some(coverage))
                        if !(coverage.start_julian_date..=coverage.end_julian_date)
                            .contains(&epoch.julian_date()) =>
                    {
                        ScientificDatasetAvailability::OutOfCoverage
                    }
                    _ => ScientificDatasetAvailability::Validated,
                },
            })
            .collect();
        statuses.extend(self.unavailable_roles.iter().copied().map(|role| {
            ScientificDatasetStatus {
                role,
                file_name: None,
                availability: ScientificDatasetAvailability::Unavailable,
            }
        }));
        statuses
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScientificDatasetAvailability {
    Validated,
    Unavailable,
    OutOfCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScientificDatasetStatus {
    pub role: ScientificDatasetRole,
    pub file_name: Option<String>,
    pub availability: ScientificDatasetAvailability,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValidatedKernel {
    pub file_name: String,
    pub role: ScientificDatasetRole,
    pub kind: KernelKind,
    pub sha256: String,
    pub path: PathBuf,
    pub expected_size_bytes: u64,
    pub source_url: String,
    pub coverage: Option<ScientificDatasetCoverage>,
    pub frame: ScientificDatasetFrame,
    pub time_scale: ScientificDatasetTimeScale,
}

/// Immutable evaluator loaded from one validated local manifest.
pub struct SpiceEphemeris {
    almanac: Almanac,
    provenance: KernelProvenance,
    earth_j2_model: EarthJ2GravityModel,
    leap_seconds_lsk: String,
}

impl SpiceEphemeris {
    pub fn load(manifest_path: impl AsRef<Path>) -> Result<Self, EphemerisError> {
        let manifest_path = manifest_path.as_ref();
        let manifest = load_manifest(manifest_path)?;
        let provenance = validate_manifest(manifest_path, &manifest)?;
        let mut almanac = Almanac::default();

        for kernel in &provenance.validated_kernels {
            if kernel.kind == KernelKind::Spk {
                almanac = almanac
                    .load(kernel.path.to_string_lossy().as_ref())
                    .map_err(|error| EphemerisError::KernelLoad {
                        role: kernel.role,
                        path: kernel.path.clone(),
                        message: error.to_string(),
                    })?;
            }
        }

        match (
            provenance
                .validated_kernels
                .iter()
                .find(|kernel| kernel.role == ScientificDatasetRole::Orientation),
            provenance
                .validated_kernels
                .iter()
                .find(|kernel| kernel.role == ScientificDatasetRole::GravitationalParameters),
        ) {
            (Some(orientation), Some(gravitational_parameters)) => {
                let planetary_data = convert_tpc(&orientation.path, &gravitational_parameters.path)
                    .map_err(|error| EphemerisError::KernelLoad {
                        role: ScientificDatasetRole::Orientation,
                        path: orientation.path.clone(),
                        message: error.to_string(),
                    })?;
                almanac = almanac.with_planetary_data(planetary_data);
            }
            (None, None) => {}
            _ => return Err(EphemerisError::IncompleteOrientationDatasets),
        }

        let earth_j2_dataset = provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::GravityHarmonics)
            .ok_or(EphemerisError::GravityHarmonicsUnavailable)?;
        let earth_j2_contents = fs::read_to_string(&earth_j2_dataset.path).map_err(|error| {
            EphemerisError::KernelLoad {
                role: ScientificDatasetRole::GravityHarmonics,
                path: earth_j2_dataset.path.clone(),
                message: error.to_string(),
            }
        })?;
        let earth_j2_model =
            ron::from_str::<EarthJ2GravityModel>(&earth_j2_contents).map_err(|error| {
                EphemerisError::GravityHarmonicsParse {
                    path: earth_j2_dataset.path.clone(),
                    message: error.to_string(),
                }
            })?;
        if !earth_j2_model.is_valid() {
            return Err(EphemerisError::InvalidGravityHarmonics {
                path: earth_j2_dataset.path.clone(),
            });
        }
        let leap_seconds_dataset = provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::LeapSeconds)
            .ok_or(EphemerisError::LeapSecondsUnavailable)?;
        let leap_seconds_lsk = fs::read_to_string(&leap_seconds_dataset.path).map_err(|error| {
            EphemerisError::KernelLoad {
                role: ScientificDatasetRole::LeapSeconds,
                path: leap_seconds_dataset.path.clone(),
                message: error.to_string(),
            }
        })?;

        Ok(Self {
            almanac,
            provenance,
            earth_j2_model,
            leap_seconds_lsk,
        })
    }

    /// Load the same reviewed local kernel set from bytes embedded in the WASM
    /// artifact. Browser environments have no synchronous filesystem, so this
    /// preserves the single DE440 authority without a network fallback.
    #[cfg(any(target_arch = "wasm32", test))]
    pub fn load_embedded() -> Result<Self, EphemerisError> {
        const MANIFEST_PATH: &str = "embedded:assets/configs/ephemeris/de440.ron";
        let manifest = ron::from_str::<KernelManifest>(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/configs/ephemeris/de440.ron"
        )))
        .map_err(|error| EphemerisError::ManifestParse {
            path: PathBuf::from(MANIFEST_PATH),
            message: error.to_string(),
        })?;
        if !manifest_is_valid(&manifest) {
            return Err(EphemerisError::InvalidManifest {
                path: PathBuf::from(MANIFEST_PATH),
            });
        }

        let mut validated_kernels = Vec::with_capacity(manifest.kernels.len());
        for kernel in &manifest.kernels {
            let path = PathBuf::from(format!("embedded:{}", kernel.file_name));
            let bytes = embedded_kernel_bytes(&kernel.file_name).ok_or_else(|| {
                EphemerisError::EmbeddedKernelMissing {
                    file_name: kernel.file_name.clone(),
                }
            })?;
            if bytes.len() as u64 != kernel.expected_size_bytes {
                return Err(EphemerisError::KernelSize {
                    role: kernel.role,
                    path,
                    expected: kernel.expected_size_bytes,
                    actual: bytes.len() as u64,
                });
            }
            let actual_sha256 = format!("{:x}", Sha256::digest(bytes));
            if actual_sha256 != kernel.sha256 {
                return Err(EphemerisError::KernelChecksum {
                    role: kernel.role,
                    path,
                    expected: kernel.sha256.clone(),
                    actual: actual_sha256,
                });
            }
            validated_kernels.push(ValidatedKernel {
                file_name: kernel.file_name.clone(),
                role: kernel.role,
                kind: kernel.kind,
                sha256: kernel.sha256.clone(),
                path: PathBuf::from(format!("embedded:{}", kernel.file_name)),
                expected_size_bytes: kernel.expected_size_bytes,
                source_url: kernel.source_url.clone(),
                coverage: kernel.coverage,
                frame: kernel.frame,
                time_scale: kernel.time_scale,
            });
        }
        let provenance = KernelProvenance {
            manifest_id: manifest.id,
            manifest_path: PathBuf::from(MANIFEST_PATH),
            coverage: manifest.coverage,
            validated_kernels,
            unavailable_roles: manifest.unavailable_roles,
        };

        let translation_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::Translation)?;
        let translation = embedded_dataset_bytes(&provenance, ScientificDatasetRole::Translation)?;
        let spk = SPK::parse(translation).map_err(|error| EphemerisError::KernelLoad {
            role: ScientificDatasetRole::Translation,
            path: translation_path,
            message: error.to_string(),
        })?;
        let orientation_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::Orientation)?;
        let orientation = parse_embedded_tpc(&provenance, ScientificDatasetRole::Orientation)?;
        let gravitational_parameters =
            parse_embedded_tpc(&provenance, ScientificDatasetRole::GravitationalParameters)?;
        let planetary_data =
            convert_tpc_items(orientation, gravitational_parameters).map_err(|error| {
                EphemerisError::KernelLoad {
                    role: ScientificDatasetRole::Orientation,
                    path: orientation_path,
                    message: error.to_string(),
                }
            })?;
        let earth_j2_dataset =
            embedded_dataset_bytes(&provenance, ScientificDatasetRole::GravityHarmonics)?;
        let earth_j2_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::GravityHarmonics)?;
        let earth_j2_contents =
            std::str::from_utf8(earth_j2_dataset).map_err(|error| EphemerisError::KernelLoad {
                role: ScientificDatasetRole::GravityHarmonics,
                path: earth_j2_path.clone(),
                message: error.to_string(),
            })?;
        let earth_j2_model =
            ron::from_str::<EarthJ2GravityModel>(earth_j2_contents).map_err(|error| {
                EphemerisError::GravityHarmonicsParse {
                    path: earth_j2_path,
                    message: error.to_string(),
                }
            })?;
        if !earth_j2_model.is_valid() {
            return Err(EphemerisError::InvalidGravityHarmonics {
                path: embedded_dataset_path(&provenance, ScientificDatasetRole::GravityHarmonics)?,
            });
        }
        let leap_seconds_bytes =
            embedded_dataset_bytes(&provenance, ScientificDatasetRole::LeapSeconds)?;
        let leap_seconds_path =
            embedded_dataset_path(&provenance, ScientificDatasetRole::LeapSeconds)?;
        let leap_seconds_lsk = std::str::from_utf8(leap_seconds_bytes)
            .map_err(|error| EphemerisError::KernelLoad {
                role: ScientificDatasetRole::LeapSeconds,
                path: leap_seconds_path,
                message: error.to_string(),
            })?
            .to_owned();

        Ok(Self {
            almanac: Almanac::from_spk(spk).with_planetary_data(planetary_data),
            provenance,
            earth_j2_model,
            leap_seconds_lsk,
        })
    }

    pub fn provenance(&self) -> &KernelProvenance {
        &self.provenance
    }

    /// Validated EGM2008 degree-two Earth gravity model. The coefficient and
    /// reference radius remain distinct from DE440's gravitational parameter.
    pub fn earth_j2_model(&self) -> &EarthJ2GravityModel {
        &self.earth_j2_model
    }

    /// Pinned NAIF LSK text validated with the active kernel manifest.
    pub fn leap_seconds_lsk(&self) -> &str {
        &self.leap_seconds_lsk
    }

    pub fn state(
        &self,
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<BodyState, EphemerisError> {
        if !self.provenance.coverage.contains(epoch) {
            return Err(EphemerisError::EpochOutsideCoverage {
                epoch,
                coverage: self.provenance.coverage,
            });
        }

        let center_frame = if center == NaifBodyId::SOLAR_SYSTEM_BARYCENTER {
            SSB_J2000
        } else {
            center.j2000_frame()
        };
        let state = self
            .almanac
            .translate(
                target.j2000_frame(),
                center_frame,
                epoch.anise_epoch(),
                None,
            )
            .map_err(|error| EphemerisError::StateEvaluation {
                target,
                center,
                epoch,
                message: error.to_string(),
            })?;

        BodyState::from_anise(
            target,
            center,
            epoch,
            [state.radius_km.x, state.radius_km.y, state.radius_km.z],
            [
                state.velocity_km_s.x,
                state.velocity_km_s.y,
                state.velocity_km_s.z,
            ],
        )
    }

    /// Evaluate an IAU body-fixed orientation from the validated local PCK at
    /// one TDB epoch. There is no catalog fallback on this scientific path.
    pub fn orientation(
        &self,
        target: NaifBodyId,
        epoch: TdbEpoch,
    ) -> Result<BodyOrientation, EphemerisError> {
        if !self.provenance.coverage.contains(epoch) {
            return Err(EphemerisError::EpochOutsideCoverage {
                epoch,
                coverage: self.provenance.coverage,
            });
        }
        let orientation_dataset = self
            .provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::Orientation)
            .ok_or(EphemerisError::OrientationUnavailable)?;
        let orientation_target = target
            .orientation_target()
            .ok_or(EphemerisError::OrientationUnsupportedBody { target })?;
        let inertial = Frame::from_ephem_j2000(orientation_target.value());
        let body_fixed = Frame::new(orientation_target.value(), orientation_target.value());
        let dcm = self
            .almanac
            .rotate(inertial, body_fixed, epoch.anise_epoch())
            .map_err(|error| EphemerisError::OrientationEvaluation {
                target,
                epoch,
                message: error.to_string(),
            })?;
        let angular_velocity = self
            .almanac
            .angular_velocity_wrt_j2000_rad_s(body_fixed, epoch.anise_epoch())
            .map_err(|error| EphemerisError::OrientationEvaluation {
                target,
                epoch,
                message: error.to_string(),
            })?;
        let matrix = dcm.rot_mat;
        let inertial_to_body_fixed = DQuat::from_mat3(&DMat3::from_cols(
            DVec3::new(matrix[(0, 0)], matrix[(1, 0)], matrix[(2, 0)]),
            DVec3::new(matrix[(0, 1)], matrix[(1, 1)], matrix[(2, 1)]),
            DVec3::new(matrix[(0, 2)], matrix[(1, 2)], matrix[(2, 2)]),
        ));

        Ok(BodyOrientation::from_kernel(
            target,
            epoch,
            format!(
                "{}:{}#{}",
                self.provenance.manifest_id,
                orientation_dataset.file_name,
                orientation_dataset.sha256
            ),
            inertial_to_body_fixed,
            // ANISE reports the passive ICRF-to-body-fixed transform's frame
            // angular velocity. `BodyOrientation` owns the body's active spin
            // in ICRF, which has the opposite sign.
            DVec3::new(
                -angular_velocity.x,
                -angular_velocity.y,
                -angular_velocity.z,
            ),
        ))
    }

    /// Return a validated `gm_de440.tpc` standard gravitational parameter in
    /// SI m³/s². Unlike catalog mass times a universal G, this is the exact
    /// body constant supplied by the active scientific dataset.
    pub fn gravitational_parameter_m3_s2(&self, target: NaifBodyId) -> Result<f64, EphemerisError> {
        let gm_dataset = self
            .provenance
            .validated_kernels
            .iter()
            .find(|kernel| kernel.role == ScientificDatasetRole::GravitationalParameters)
            .ok_or(EphemerisError::GravitationalParametersUnavailable)?;
        let physical_target = target
            .gravitational_parameter_target()
            .ok_or(EphemerisError::GravitationalParametersUnsupportedBody { target })?;
        let planetary_data = self
            .almanac
            .get_planetary_data_from_id(physical_target.value())
            .map_err(|error| EphemerisError::GravitationalParameterEvaluation {
                target,
                message: error.to_string(),
            })?;
        let mu_m3_s2 = planetary_data.mu_km3_s2 * KILOMETERS_CUBED_TO_METERS_CUBED;
        if !mu_m3_s2.is_finite() || mu_m3_s2 <= 0.0 {
            return Err(EphemerisError::InvalidGravitationalParameter {
                target,
                file_name: gm_dataset.file_name.clone(),
                value_m3_s2: mu_m3_s2,
            });
        }
        Ok(mu_m3_s2)
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_kernel_bytes(file_name: &str) -> Option<&'static [u8]> {
    match file_name {
        "de440s.bsp" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/de440s.bsp"
        ))),
        "pck00011.tpc" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/pck00011.tpc"
        ))),
        "gm_de440.tpc" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/gm_de440.tpc"
        ))),
        "egm2008_earth_j2.ron" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/egm2008_earth_j2.ron"
        ))),
        "naif0012.tls" => Some(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/large_files/kernels/de440/naif0012.tls"
        ))),
        _ => None,
    }
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_dataset_path(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<PathBuf, EphemerisError> {
    provenance
        .validated_kernels
        .iter()
        .find(|kernel| kernel.role == role)
        .map(|kernel| kernel.path.clone())
        .ok_or(EphemerisError::RequiredDatasetMissing { role })
}

#[cfg(any(target_arch = "wasm32", test))]
fn embedded_dataset_bytes(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<&'static [u8], EphemerisError> {
    let dataset = provenance
        .validated_kernels
        .iter()
        .find(|kernel| kernel.role == role)
        .ok_or(EphemerisError::RequiredDatasetMissing { role })?;
    embedded_kernel_bytes(&dataset.file_name).ok_or_else(|| EphemerisError::EmbeddedKernelMissing {
        file_name: dataset.file_name.clone(),
    })
}

#[cfg(any(target_arch = "wasm32", test))]
fn parse_embedded_tpc(
    provenance: &KernelProvenance,
    role: ScientificDatasetRole,
) -> Result<HashMap<i32, TPCItem>, EphemerisError> {
    let path = embedded_dataset_path(provenance, role)?;
    let bytes = embedded_dataset_bytes(provenance, role)?;
    let mut reader = BufReader::new(Cursor::new(bytes));
    parse_bytes::<_, TPCItem>(&mut reader, false).map_err(|error| EphemerisError::KernelLoad {
        role,
        path,
        message: error.to_string(),
    })
}

pub fn load_manifest(manifest_path: impl AsRef<Path>) -> Result<KernelManifest, EphemerisError> {
    let manifest_path = manifest_path.as_ref();
    let contents =
        fs::read_to_string(manifest_path).map_err(|source| EphemerisError::ManifestRead {
            path: manifest_path.to_path_buf(),
            source,
        })?;
    let manifest = ron::from_str::<KernelManifest>(&contents).map_err(|error| {
        EphemerisError::ManifestParse {
            path: manifest_path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    if !manifest_is_valid(&manifest) {
        return Err(EphemerisError::InvalidManifest {
            path: manifest_path.to_path_buf(),
        });
    }
    Ok(manifest)
}

fn manifest_is_valid(manifest: &KernelManifest) -> bool {
    if manifest.id.trim().is_empty() || !manifest.coverage.is_valid() || manifest.kernels.is_empty()
    {
        return false;
    }

    let Some(translation_dataset) = manifest
        .kernels
        .iter()
        .find(|dataset| dataset.role == ScientificDatasetRole::Translation)
    else {
        return false;
    };
    let Some(translation_coverage) = translation_dataset.coverage else {
        return false;
    };
    if translation_coverage.start_julian_date != manifest.coverage.start_julian_date_tdb
        || translation_coverage.end_julian_date != manifest.coverage.end_julian_date_tdb
    {
        return false;
    }

    let mut roles = std::collections::HashSet::with_capacity(
        manifest.kernels.len() + manifest.unavailable_roles.len(),
    );
    if !manifest
        .kernels
        .iter()
        .all(|dataset| roles.insert(dataset.role) && dataset.metadata_is_valid())
    {
        return false;
    }

    manifest
        .unavailable_roles
        .iter()
        .all(|role| *role != ScientificDatasetRole::Translation && roles.insert(*role))
}

pub fn validate_manifest(
    manifest_path: impl AsRef<Path>,
    manifest: &KernelManifest,
) -> Result<KernelProvenance, EphemerisError> {
    let manifest_path = manifest_path.as_ref();
    let manifest_directory =
        manifest_path
            .parent()
            .ok_or_else(|| EphemerisError::InvalidManifest {
                path: manifest_path.to_path_buf(),
            })?;
    let kernel_root = manifest_directory.join(&manifest.kernel_root);
    let mut validated_kernels = Vec::with_capacity(manifest.kernels.len());

    for kernel in &manifest.kernels {
        if kernel.file_name.trim().is_empty()
            || kernel.sha256.len() != 64
            || !kernel.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
            || kernel.source_url.trim().is_empty()
            || !kernel.metadata_is_valid()
        {
            return Err(EphemerisError::InvalidManifest {
                path: manifest_path.to_path_buf(),
            });
        }
        let path = kernel_root.join(&kernel.file_name);
        let bytes = fs::read(&path).map_err(|source| EphemerisError::KernelRead {
            role: kernel.role,
            path: path.clone(),
            source,
        })?;
        if bytes.len() as u64 != kernel.expected_size_bytes {
            return Err(EphemerisError::KernelSize {
                role: kernel.role,
                path,
                expected: kernel.expected_size_bytes,
                actual: bytes.len() as u64,
            });
        }
        let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
        if actual_sha256 != kernel.sha256 {
            return Err(EphemerisError::KernelChecksum {
                role: kernel.role,
                path,
                expected: kernel.sha256.clone(),
                actual: actual_sha256,
            });
        }
        validated_kernels.push(ValidatedKernel {
            file_name: kernel.file_name.clone(),
            role: kernel.role,
            kind: kernel.kind,
            sha256: kernel.sha256.clone(),
            path,
            expected_size_bytes: kernel.expected_size_bytes,
            source_url: kernel.source_url.clone(),
            coverage: kernel.coverage,
            frame: kernel.frame,
            time_scale: kernel.time_scale,
        });
    }

    Ok(KernelProvenance {
        manifest_id: manifest.id.clone(),
        manifest_path: manifest_path.to_path_buf(),
        coverage: manifest.coverage,
        validated_kernels,
        unavailable_roles: manifest.unavailable_roles.clone(),
    })
}

#[derive(Debug)]
pub enum EphemerisError {
    InvalidEpoch(f64),
    InvalidManifest {
        path: PathBuf,
    },
    ManifestRead {
        path: PathBuf,
        source: std::io::Error,
    },
    ManifestParse {
        path: PathBuf,
        message: String,
    },
    KernelRead {
        role: ScientificDatasetRole,
        path: PathBuf,
        source: std::io::Error,
    },
    KernelSize {
        role: ScientificDatasetRole,
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    KernelChecksum {
        role: ScientificDatasetRole,
        path: PathBuf,
        expected: String,
        actual: String,
    },
    KernelLoad {
        role: ScientificDatasetRole,
        path: PathBuf,
        message: String,
    },
    IncompleteOrientationDatasets,
    OrientationUnavailable,
    GravitationalParametersUnavailable,
    GravityHarmonicsUnavailable,
    LeapSecondsUnavailable,
    RequiredDatasetMissing {
        role: ScientificDatasetRole,
    },
    EmbeddedKernelMissing {
        file_name: String,
    },
    GravitationalParametersUnsupportedBody {
        target: NaifBodyId,
    },
    GravitationalParameterEvaluation {
        target: NaifBodyId,
        message: String,
    },
    InvalidGravitationalParameter {
        target: NaifBodyId,
        file_name: String,
        value_m3_s2: f64,
    },
    GravityHarmonicsParse {
        path: PathBuf,
        message: String,
    },
    InvalidGravityHarmonics {
        path: PathBuf,
    },
    OrientationUnsupportedBody {
        target: NaifBodyId,
    },
    OrientationEvaluation {
        target: NaifBodyId,
        epoch: TdbEpoch,
        message: String,
    },
    EpochOutsideCoverage {
        epoch: TdbEpoch,
        coverage: KernelCoverage,
    },
    StateEvaluation {
        target: NaifBodyId,
        center: NaifBodyId,
        epoch: TdbEpoch,
        message: String,
    },
    NonFiniteState {
        target: NaifBodyId,
        center: NaifBodyId,
    },
}

impl fmt::Display for EphemerisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEpoch(value) => write!(formatter, "invalid TDB epoch: {value}"),
            Self::InvalidManifest { path } => {
                write!(formatter, "invalid kernel manifest: {}", path.display())
            }
            Self::ManifestRead { path, source } => {
                write!(
                    formatter,
                    "cannot read kernel manifest {}: {source}",
                    path.display()
                )
            }
            Self::ManifestParse { path, message } => {
                write!(
                    formatter,
                    "cannot parse kernel manifest {}: {message}",
                    path.display()
                )
            }
            Self::KernelRead { role, path, source } => {
                write!(
                    formatter,
                    "cannot read {role} dataset {}: {source}",
                    path.display()
                )
            }
            Self::KernelSize {
                role,
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "{role} dataset {} has {actual} bytes; expected {expected}",
                path.display()
            ),
            Self::KernelChecksum {
                role,
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "{role} dataset {} checksum mismatch: expected {expected}, got {actual}",
                path.display()
            ),
            Self::KernelLoad {
                role,
                path,
                message,
            } => {
                write!(
                    formatter,
                    "cannot load {role} dataset {}: {message}",
                    path.display()
                )
            }
            Self::IncompleteOrientationDatasets => formatter.write_str(
                "orientation requires both validated orientation and gravitational-parameter datasets",
            ),
            Self::OrientationUnavailable => {
                formatter.write_str("no validated orientation dataset is configured")
            }
            Self::GravitationalParametersUnavailable => {
                formatter.write_str("no validated gravitational-parameter dataset is configured")
            }
            Self::GravityHarmonicsUnavailable => {
                formatter.write_str("no validated gravity-harmonic dataset is configured")
            }
            Self::LeapSecondsUnavailable => {
                formatter.write_str("no validated leap-second dataset is configured")
            }
            Self::RequiredDatasetMissing { role } => {
                write!(formatter, "no validated {role} dataset is configured")
            }
            Self::EmbeddedKernelMissing { file_name } => {
                write!(formatter, "embedded kernel bytes are missing for {file_name}")
            }
            Self::GravitationalParametersUnsupportedBody { target } => write!(
                formatter,
                "no gravitational-parameter mapping for NAIF {}",
                target.value()
            ),
            Self::GravitationalParameterEvaluation { target, message } => write!(
                formatter,
                "cannot evaluate gravitational parameter for NAIF {}: {message}",
                target.value()
            ),
            Self::InvalidGravitationalParameter {
                target,
                file_name,
                value_m3_s2,
            } => write!(
                formatter,
                "gravitational-parameter dataset {file_name} has invalid mu {value_m3_s2} m^3/s^2 for NAIF {}",
                target.value()
            ),
            Self::GravityHarmonicsParse { path, message } => write!(
                formatter,
                "cannot parse gravity-harmonic dataset {}: {message}",
                path.display()
            ),
            Self::InvalidGravityHarmonics { path } => write!(
                formatter,
                "gravity-harmonic dataset {} has invalid Earth J2 parameters",
                path.display()
            ),
            Self::OrientationUnsupportedBody { target } => {
                write!(formatter, "no IAU orientation mapping for NAIF {}", target.value())
            }
            Self::OrientationEvaluation {
                target,
                epoch,
                message,
            } => write!(
                formatter,
                "cannot evaluate IAU orientation for NAIF {} at TDB JD {}: {message}",
                target.value(),
                epoch.julian_date()
            ),
            Self::EpochOutsideCoverage { epoch, coverage } => write!(
                formatter,
                "TDB JD {} is outside kernel coverage {}..={}",
                epoch.julian_date(),
                coverage.start_julian_date_tdb,
                coverage.end_julian_date_tdb
            ),
            Self::StateEvaluation {
                target,
                center,
                epoch,
                message,
            } => write!(
                formatter,
                "cannot evaluate NAIF {} relative to {} at TDB JD {}: {message}",
                target.value(),
                center.value(),
                epoch.julian_date()
            ),
            Self::NonFiniteState { target, center } => write!(
                formatter,
                "NAIF {} relative to {} produced a non-finite state",
                target.value(),
                center.value()
            ),
        }
    }
}

impl std::error::Error for EphemerisError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::body_orientation::{
        OrientationBodyFixedFrame, OrientationDataSource, OrientationInertialFrame,
    };
    use crate::domain::services::simulation_epoch::LeapSecondTable;

    fn fixture_manifest(kernel_root: &str, sha256: &str) -> String {
        format!(
            r#"(
                id: "test-kernel-set",
                kernel_root: "{kernel_root}",
                coverage: (
                    start_julian_date_tdb: 2451545.0,
                    end_julian_date_tdb: 2451546.0,
                ),
                kernels: [(
                    file_name: "fixture.bsp",
                    role: Translation,
                    kind: Spk,
                    sha256: "{sha256}",
                    expected_size_bytes: 3,
                    source_url: "https://example.invalid/fixture.bsp",
                    coverage: Some((
                        start_julian_date: 2451545.0,
                        end_julian_date: 2451546.0,
                    )),
                    frame: SsbIcrfJ2000,
                    time_scale: Tdb,
                )],
            )"#
        )
    }

    fn fixture_root() -> std::path::PathBuf {
        let unique_suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "cosmic-ephemeris-{}-{unique_suffix}",
            std::process::id()
        ))
    }

    #[test]
    fn tdb_epoch_preserves_j2000_julian_date() {
        let epoch = TdbEpoch::from_seconds_since_j2000(86_400.0).unwrap();

        assert_eq!(TdbEpoch::j2000().julian_date(), J2000_JULIAN_DATE_TDB);
        assert_eq!(epoch.julian_date(), J2000_JULIAN_DATE_TDB + 1.0);
        assert_eq!(epoch.seconds_since_j2000(), 86_400.0);
    }

    #[test]
    fn body_state_converts_kernel_kilometers_to_si() {
        let state = BodyState::from_anise(
            NaifBodyId::EARTH,
            NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
            TdbEpoch::j2000(),
            [1.0, -2.0, 0.5],
            [3.0, -4.0, 0.25],
        )
        .unwrap();

        assert_eq!(state.position_m, DVec3::new(1_000.0, -2_000.0, 500.0));
        assert_eq!(state.velocity_mps, DVec3::new(3_000.0, -4_000.0, 250.0));
    }

    #[test]
    fn embedded_kernel_set_matches_the_manifest_backed_authority() {
        let ephemeris = SpiceEphemeris::load_embedded().unwrap();
        let earth = ephemeris
            .state(
                NaifBodyId::EARTH,
                NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                TdbEpoch::j2000(),
            )
            .unwrap();

        assert_eq!(
            ephemeris.provenance().manifest_id,
            "naif-de440-egm2008-primary-v1"
        );
        assert!(earth.position_m.is_finite());
        assert!(earth.velocity_mps.is_finite());
        assert!(LeapSecondTable::parse_lsk(ephemeris.leap_seconds_lsk()).is_ok());
    }

    #[test]
    fn manifest_retains_typed_dataset_provenance() {
        let manifest = ron::from_str::<KernelManifest>(&fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ))
        .unwrap();

        assert_eq!(manifest.kernels.len(), 1);
        let dataset = &manifest.kernels[0];
        assert_eq!(dataset.role, ScientificDatasetRole::Translation);
        assert_eq!(dataset.kind, KernelKind::Spk);
        assert_eq!(
            dataset.coverage,
            Some(ScientificDatasetCoverage {
                start_julian_date: J2000_JULIAN_DATE_TDB,
                end_julian_date: J2000_JULIAN_DATE_TDB + 1.0,
            })
        );
        assert_eq!(dataset.frame, ScientificDatasetFrame::SsbIcrfJ2000);
        assert_eq!(dataset.time_scale, ScientificDatasetTimeScale::Tdb);
    }

    #[test]
    fn manifest_reports_explicitly_unavailable_dataset_role() {
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );

        let manifest = ron::from_str::<KernelManifest>(&contents).unwrap();
        assert_eq!(
            manifest.unavailable_roles,
            vec![ScientificDatasetRole::EarthOrientation]
        );
    }

    #[test]
    fn manifest_rejects_checksum_mismatch() {
        let root = fixture_root();
        let kernel_root = root.join("kernels");
        fs::create_dir_all(&kernel_root).unwrap();
        fs::write(kernel_root.join("fixture.bsp"), b"abc").unwrap();
        let manifest_path = root.join("manifest.ron");
        fs::write(
            &manifest_path,
            fixture_manifest(
                "kernels",
                "0000000000000000000000000000000000000000000000000000000000000000",
            ),
        )
        .unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(matches!(
            validate_manifest(&manifest_path, &manifest),
            Err(EphemerisError::KernelChecksum {
                role: ScientificDatasetRole::Translation,
                ..
            })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_reports_missing_kernel() {
        let root = fixture_root();
        fs::create_dir_all(root.join("kernels")).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );
        fs::write(&manifest_path, contents).unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(matches!(
            validate_manifest(&manifest_path, &manifest),
            Err(EphemerisError::KernelRead {
                role: ScientificDatasetRole::Translation,
                ..
            })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_missing_translation_role() {
        let root = fixture_root();
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace("role: Translation", "role: Orientation")
        .replace("kind: Spk", "kind: TextPck")
        .replace("frame: SsbIcrfJ2000", "frame: IauBodyFixed");
        fs::write(&manifest_path, contents).unwrap();

        assert!(matches!(
            load_manifest(&manifest_path),
            Err(EphemerisError::InvalidManifest { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_translation_coverage_mismatch() {
        let root = fixture_root();
        fs::create_dir_all(&root).unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "end_julian_date_tdb: 2451546.0",
            "end_julian_date_tdb: 2451547.0",
        );
        fs::write(&manifest_path, contents).unwrap();

        assert!(matches!(
            load_manifest(&manifest_path),
            Err(EphemerisError::InvalidManifest { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn provenance_reports_out_of_coverage_tdb_datasets() {
        let root = fixture_root();
        let kernel_root = root.join("kernels");
        fs::create_dir_all(&kernel_root).unwrap();
        fs::write(kernel_root.join("fixture.bsp"), b"abc").unwrap();
        let manifest_path = root.join("manifest.ron");
        let contents = fixture_manifest(
            "kernels",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        )
        .replace(
            "kernels: [(",
            "unavailable_roles: [EarthOrientation],\n                kernels: [(",
        );
        fs::write(&manifest_path, contents).unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        let provenance = validate_manifest(&manifest_path, &manifest).unwrap();
        assert_eq!(
            provenance.dataset_statuses_at_tdb(
                TdbEpoch::from_julian_date(J2000_JULIAN_DATE_TDB + 2.0).unwrap()
            ),
            vec![
                ScientificDatasetStatus {
                    role: ScientificDatasetRole::Translation,
                    file_name: Some("fixture.bsp".to_string()),
                    availability: ScientificDatasetAvailability::OutOfCoverage,
                },
                ScientificDatasetStatus {
                    role: ScientificDatasetRole::EarthOrientation,
                    file_name: None,
                    availability: ScientificDatasetAvailability::Unavailable,
                }
            ]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_rejects_epoch_outside_declared_coverage() {
        let coverage = KernelCoverage {
            start_julian_date_tdb: J2000_JULIAN_DATE_TDB,
            end_julian_date_tdb: J2000_JULIAN_DATE_TDB + 1.0,
        };

        assert!(coverage.contains(TdbEpoch::j2000()));
        assert!(!coverage.contains(TdbEpoch::from_seconds_since_j2000(172_800.0).unwrap()));
    }

    #[test]
    fn provisioned_pck_evaluates_earth_orientation_from_the_kernel_contract() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let orientation = ephemeris
            .orientation(NaifBodyId::EARTH, TdbEpoch::j2000())
            .unwrap();

        assert_eq!(orientation.provenance.source, OrientationDataSource::Kernel);
        assert_eq!(
            orientation.provenance.inertial_frame,
            OrientationInertialFrame::IcrfJ2000
        );
        assert_eq!(
            orientation.provenance.body_fixed_frame,
            OrientationBodyFixedFrame::IauBodyFixed
        );
        assert!(orientation.inertial_to_body_fixed.is_finite());
        assert!(orientation.angular_velocity_inertial_rad_s.is_finite());
        assert!(orientation.angular_velocity_inertial_rad_s.z > 0.0);
    }

    #[test]
    fn provisioned_gm_evaluates_kernel_backed_body_parameters_in_si() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let earth_mu_m3_s2 = ephemeris
            .gravitational_parameter_m3_s2(NaifBodyId::EARTH)
            .unwrap();
        let sun_mu_m3_s2 = ephemeris
            .gravitational_parameter_m3_s2(NaifBodyId::SUN)
            .unwrap();

        assert!((earth_mu_m3_s2 - 3.986_004_355_070_226e14).abs() < 1.0);
        assert!(sun_mu_m3_s2 > earth_mu_m3_s2);
    }

    #[test]
    fn provisioned_egm2008_j2_model_is_validated_with_the_manifest() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let model = ephemeris.earth_j2_model();

        assert_eq!(model.model_id, "EGM2008");
        assert!(model.is_valid());
    }

    #[derive(Clone, Copy)]
    struct HorizonsStateReference {
        name: &'static str,
        target: NaifBodyId,
        center: NaifBodyId,
        julian_date_tdb: f64,
        position_km: DVec3,
        velocity_km_s: DVec3,
    }

    // These are geometric ICRF vectors from JPL Horizons DE441, retrieved on
    // 2026-08-30 with `EPHEM_TYPE=VECTORS`, `TIME_TYPE=TDB`, `OUT_UNITS=KM-S`,
    // `REF_PLANE=FRAME`, and `VEC_CORR=NONE`. DE440s is the provisioned runtime
    // authority, so the budgets cover its documented small DE440/DE441 delta,
    // not an analytic approximation or presentation-space rounding.
    const DE440S_DE441_POSITION_BUDGET_M: f64 = 100.0;
    const DE440S_DE441_VELOCITY_BUDGET_MPS: f64 = 1.0e-3;

    fn assert_matches_horizons_reference(
        ephemeris: &SpiceEphemeris,
        reference: HorizonsStateReference,
    ) {
        let epoch = TdbEpoch::from_julian_date(reference.julian_date_tdb).unwrap();
        let state = ephemeris
            .state(reference.target, reference.center, epoch)
            .unwrap();
        let expected_position_m = reference.position_km * KILOMETERS_TO_METERS;
        let expected_velocity_mps = reference.velocity_km_s * KILOMETERS_TO_METERS;
        let position_residual_m = state.position_m.distance(expected_position_m);
        let velocity_residual_mps = state.velocity_mps.distance(expected_velocity_mps);

        assert!(
            position_residual_m <= DE440S_DE441_POSITION_BUDGET_M,
            "{} at JD TDB {}: target {} relative to {} has position residual {} m; budget {} m (Horizons DE441 geometric ICRF)",
            reference.name,
            reference.julian_date_tdb,
            reference.target.value(),
            reference.center.value(),
            position_residual_m,
            DE440S_DE441_POSITION_BUDGET_M,
        );
        assert!(
            velocity_residual_mps <= DE440S_DE441_VELOCITY_BUDGET_MPS,
            "{} at JD TDB {}: target {} relative to {} has velocity residual {} m/s; budget {} m/s (Horizons DE441 geometric ICRF)",
            reference.name,
            reference.julian_date_tdb,
            reference.target.value(),
            reference.center.value(),
            velocity_residual_mps,
            DE440S_DE441_VELOCITY_BUDGET_MPS,
        );
    }

    #[test]
    #[ignore = "requires scripts/provision_de440_kernels.sh"]
    fn de440_states_match_recorded_horizons_references_across_epochs() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let references = [
            HorizonsStateReference {
                name: "Earth/SSB at J2000",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    -2.756_674_048_281_145e7,
                    1.323_613_811_535_491e8,
                    5.741_865_328_625_385e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.978_494_749_851_088e1,
                    -5.029_753_814_928_081,
                    -2.180_645_069_035_755,
                ),
            },
            HorizonsStateReference {
                name: "Earth/SSB at 2020-01-01 TDB",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    -2.545_334_341_413_143e7,
                    1.340_372_255_727_666e8,
                    5.810_929_286_273_248e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.986_338_200_299_215e1,
                    -4.740_000_899_098_53,
                    -2.053_804_264_578_785,
                ),
            },
            HorizonsStateReference {
                name: "Earth/SSB at 2030-01-01 TDB",
                target: NaifBodyId::EARTH,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -2.592_636_728_814_095e7,
                    1.328_867_520_755_589e8,
                    5.760_933_037_884_695e7,
                ),
                velocity_km_s: DVec3::new(
                    -2.982_040_565_319_705e1,
                    -4.921_880_284_757_354,
                    -2.134_418_313_712_851,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at J2000",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    5.974_998_767_925_48e8,
                    4.089_903_139_317_586e8,
                    1.607_562_819_387_201e8,
                ),
                velocity_km_s: DVec3::new(
                    -7.900_525_116_640_771,
                    1.017_179_630_923_791e1,
                    4.552_467_787_262_923,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at 2020-01-01 TDB",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    7.814_211_696_278_183e7,
                    -7.134_231_711_509_035e8,
                    -3.077_001_434_693_068e8,
                ),
                velocity_km_s: DVec3::new(
                    1.284_045_161_930_421e1,
                    1.888_435_760_560_088,
                    4.969_311_071_956_998e-1,
                ),
            },
            HorizonsStateReference {
                name: "Jupiter barycenter/SSB at 2030-01-01 TDB",
                target: NaifBodyId::JUPITER_BARYCENTER,
                center: NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -6.009_939_337_897_289e8,
                    -5.056_518_667_674_727e8,
                    -2.020_966_454_330_997e8,
                ),
                velocity_km_s: DVec3::new(
                    8.616_519_447_313_426,
                    -8.267_349_456_967_262,
                    -3.753_346_028_507_114,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at J2000",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_451_545.0,
                position_km: DVec3::new(
                    -2.916_083_841_877_129e5,
                    -2.667_168_338_540_655e5,
                    -7.610_248_730_658_794e4,
                ),
                velocity_km_s: DVec3::new(
                    6.435_313_889_889_519e-1,
                    -6.660_876_829_565_195e-1,
                    -3.013_257_046_610_932e-1,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at 2020-01-01 TDB",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_458_849.5,
                position_km: DVec3::new(
                    3.901_856_393_400_028e5,
                    -7.652_259_535_377_791e4,
                    -7.072_465_410_445_34e4,
                ),
                velocity_km_s: DVec3::new(
                    2.487_277_177_505_814e-1,
                    8.724_607_189_917_472e-1,
                    3.400_651_264_502e-1,
                ),
            },
            HorizonsStateReference {
                name: "Moon/Earth at 2030-01-01 TDB",
                target: NaifBodyId::MOON,
                center: NaifBodyId::EARTH,
                julian_date_tdb: 2_462_502.5,
                position_km: DVec3::new(
                    -1.930_715_952_026_768e5,
                    -2.772_423_478_014_667e5,
                    -1.368_828_942_977_237e5,
                ),
                velocity_km_s: DVec3::new(
                    9.140_320_609_081_021e-1,
                    -5.532_798_398_327_774e-1,
                    -1.432_625_780_408_247e-1,
                ),
            },
        ];

        for reference in references {
            assert_matches_horizons_reference(&ephemeris, reference);
        }
    }
}
