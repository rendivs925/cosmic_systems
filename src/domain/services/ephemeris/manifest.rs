//! Kernel manifest schema, provenance, and startup validation.

use super::{EphemerisError, TdbEpoch};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

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
    MarsOrientationOverride,
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
            Self::MarsOrientationOverride => "Mars orientation override",
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
            ScientificDatasetRole::MarsOrientationOverride => {
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
    /// Stable identifier for the validated kernel set used by a run. Kernel
    /// records are sorted so manifest ordering cannot change the result.
    pub fn run_identity(&self) -> String {
        let mut kernels: Vec<_> = self
            .validated_kernels
            .iter()
            .map(|kernel| format!("{}:{}:{}", kernel.role, kernel.file_name, kernel.sha256))
            .collect();
        kernels.sort();
        let kernel_set_sha256 = format!("{:x}", Sha256::digest(kernels.join("\n")));
        format!("{}:{kernel_set_sha256}", self.manifest_id)
    }

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

pub(crate) fn manifest_is_valid(manifest: &KernelManifest) -> bool {
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
