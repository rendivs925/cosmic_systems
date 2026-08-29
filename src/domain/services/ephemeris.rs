//! Offline, kernel-backed solar-system body states.
//!
//! This module is the only domain boundary allowed to load and evaluate the
//! curated NAIF SPICE/JPL DE kernels. It exposes geometric f64 SI states in the
//! J2000 frame; rendering and local-flight conversions remain elsewhere.

use anise::constants::frames::SSB_J2000;
use anise::frames::Frame;
use anise::prelude::Almanac;
use anise::time::Epoch;
use bevy::math::DVec3;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

pub const J2000_JULIAN_DATE_TDB: f64 = 2_451_545.0;
const KILOMETERS_TO_METERS: f64 = 1_000.0;

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

    /// Map the current celestial catalog's kernel-backed bodies to their NAIF
    /// identifiers. Unmapped catalog moons remain presentation-only until
    /// their required kernel coverage is added to the curated manifest.
    pub fn for_catalog_name(name: &str) -> Option<Self> {
        match name {
            "Sun" => Some(Self::SUN),
            "Mercury" => Some(Self::MERCURY_BARYCENTER),
            "Venus" => Some(Self::VENUS_BARYCENTER),
            "Earth" => Some(Self::EARTH),
            "Mars" => Some(Self::MARS_BARYCENTER),
            "Jupiter" => Some(Self::JUPITER_BARYCENTER),
            "Saturn" => Some(Self::SATURN_BARYCENTER),
            "Uranus" => Some(Self::URANUS_BARYCENTER),
            "Neptune" => Some(Self::NEPTUNE_BARYCENTER),
            "Moon" => Some(Self::MOON),
            _ => None,
        }
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
    pub kind: KernelKind,
    pub sha256: String,
    pub expected_size_bytes: u64,
    pub source_url: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
pub enum KernelKind {
    Spk,
    TextPck,
    LeapSeconds,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KernelProvenance {
    pub manifest_id: String,
    pub manifest_path: PathBuf,
    pub coverage: KernelCoverage,
    pub validated_kernels: Vec<ValidatedKernel>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatedKernel {
    pub file_name: String,
    pub kind: KernelKind,
    pub sha256: String,
    pub path: PathBuf,
}

/// Immutable evaluator loaded from one validated local manifest.
pub struct SpiceEphemeris {
    almanac: Almanac,
    provenance: KernelProvenance,
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
                        path: kernel.path.clone(),
                        message: error.to_string(),
                    })?;
            }
        }

        Ok(Self {
            almanac,
            provenance,
        })
    }

    pub fn provenance(&self) -> &KernelProvenance {
        &self.provenance
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
    if manifest.id.trim().is_empty() || !manifest.coverage.is_valid() || manifest.kernels.is_empty()
    {
        return Err(EphemerisError::InvalidManifest {
            path: manifest_path.to_path_buf(),
        });
    }
    Ok(manifest)
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
        {
            return Err(EphemerisError::InvalidManifest {
                path: manifest_path.to_path_buf(),
            });
        }
        let path = kernel_root.join(&kernel.file_name);
        let bytes = fs::read(&path).map_err(|source| EphemerisError::KernelRead {
            path: path.clone(),
            source,
        })?;
        if bytes.len() as u64 != kernel.expected_size_bytes {
            return Err(EphemerisError::KernelSize {
                path,
                expected: kernel.expected_size_bytes,
                actual: bytes.len() as u64,
            });
        }
        let actual_sha256 = format!("{:x}", Sha256::digest(&bytes));
        if actual_sha256 != kernel.sha256 {
            return Err(EphemerisError::KernelChecksum {
                path,
                expected: kernel.sha256.clone(),
                actual: actual_sha256,
            });
        }
        validated_kernels.push(ValidatedKernel {
            file_name: kernel.file_name.clone(),
            kind: kernel.kind,
            sha256: kernel.sha256.clone(),
            path,
        });
    }

    Ok(KernelProvenance {
        manifest_id: manifest.id.clone(),
        manifest_path: manifest_path.to_path_buf(),
        coverage: manifest.coverage,
        validated_kernels,
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
        path: PathBuf,
        source: std::io::Error,
    },
    KernelSize {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    KernelChecksum {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    KernelLoad {
        path: PathBuf,
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
            Self::KernelRead { path, source } => {
                write!(formatter, "cannot read kernel {}: {source}", path.display())
            }
            Self::KernelSize {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "kernel {} has {actual} bytes; expected {expected}",
                path.display()
            ),
            Self::KernelChecksum {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "kernel {} checksum mismatch: expected {expected}, got {actual}",
                path.display()
            ),
            Self::KernelLoad { path, message } => {
                write!(
                    formatter,
                    "cannot load kernel {}: {message}",
                    path.display()
                )
            }
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
                    kind: Spk,
                    sha256: "{sha256}",
                    expected_size_bytes: 3,
                    source_url: "https://example.invalid/fixture.bsp",
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
            Err(EphemerisError::KernelChecksum { .. })
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_reports_missing_kernel() {
        let root = fixture_root();
        fs::create_dir_all(root.join("kernels")).unwrap();
        let manifest_path = root.join("manifest.ron");
        fs::write(
            &manifest_path,
            fixture_manifest(
                "kernels",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
        )
        .unwrap();

        let manifest = load_manifest(&manifest_path).unwrap();
        assert!(matches!(
            validate_manifest(&manifest_path, &manifest),
            Err(EphemerisError::KernelRead { .. })
        ));
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
    #[ignore = "requires scripts/provision_de440_kernels.sh"]
    fn de440_earth_state_matches_recorded_horizons_reference_at_j2000() {
        let ephemeris = SpiceEphemeris::load("assets/configs/ephemeris/de440.ron").unwrap();
        let state = ephemeris
            .state(
                NaifBodyId::EARTH,
                NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                TdbEpoch::j2000(),
            )
            .unwrap();

        // JPL Horizons DE441, Earth (399) relative to SSB (0), JD TDB
        // 2451545.0, ICRF, geometric, KM-S. Retrieved 2026-08-30 from the
        // Horizons API; DE440s is expected to differ from DE441 slightly.
        let horizons_position_m = DVec3::new(
            -2.756_674_048_281_145e10,
            1.323_613_811_535_491e11,
            5.741_865_328_625_385e10,
        );
        let horizons_velocity_mps = DVec3::new(
            -2.978_494_749_851_088e4,
            -5.029_753_814_928_081e3,
            -2.180_645_069_035_755e3,
        );

        assert!(state.position_m.distance(horizons_position_m) < 100.0);
        assert!(state.velocity_mps.distance(horizons_velocity_mps) < 1.0e-3);

        let repeated = ephemeris
            .state(
                NaifBodyId::EARTH,
                NaifBodyId::SOLAR_SYSTEM_BARYCENTER,
                TdbEpoch::j2000(),
            )
            .unwrap();
        assert_eq!(state, repeated);
    }
}
