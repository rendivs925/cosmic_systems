//! The ephemeris error type shared by manifest validation and state evaluation.

use super::manifest::{KernelCoverage, ScientificDatasetRole};
use super::{NaifBodyId, TdbEpoch};
use std::fmt;
use std::path::PathBuf;

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
