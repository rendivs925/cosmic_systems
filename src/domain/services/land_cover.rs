//! Versioned offline land-cover packages for presentation-only vegetation.
//!
//! A land-cover package is an offline-prepared, provenance-recorded raster of
//! broad surface classes (ESA WorldCover-style) that drives vegetation species
//! and density. It is deliberately separate from the authoritative terrain
//! source: it never feeds height, collision, altitude, or physics. When a
//! package is absent, malformed, or out of coverage the caller deterministically
//! falls back to the source's climate-derived `vegetation_density`.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"CSLCVR\0\0";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 88;

/// Failures when validating or decoding a land-cover package.
#[derive(Debug)]
pub enum LandCoverError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for LandCoverError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Broad surface classes, ordered to match the ESA WorldCover class numbering
/// used by the offline converter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandCoverClass {
    TreeCover,
    Shrubland,
    Grassland,
    Cropland,
    BuiltUp,
    BareSparse,
    SnowIce,
    PermanentWater,
    HerbaceousWetland,
    Mangroves,
    MossLichen,
}

impl LandCoverClass {
    /// Decode a stored class byte. Unknown values fall back to bare/sparse so a
    /// newer source class never turns into vegetation.
    pub fn from_code(code: u8) -> Self {
        match code {
            0 => Self::TreeCover,
            1 => Self::Shrubland,
            2 => Self::Grassland,
            3 => Self::Cropland,
            4 => Self::BuiltUp,
            5 => Self::BareSparse,
            6 => Self::SnowIce,
            7 => Self::PermanentWater,
            8 => Self::HerbaceousWetland,
            9 => Self::Mangroves,
            10 => Self::MossLichen,
            _ => Self::BareSparse,
        }
    }

    pub const fn code(self) -> u8 {
        match self {
            Self::TreeCover => 0,
            Self::Shrubland => 1,
            Self::Grassland => 2,
            Self::Cropland => 3,
            Self::BuiltUp => 4,
            Self::BareSparse => 5,
            Self::SnowIce => 6,
            Self::PermanentWater => 7,
            Self::HerbaceousWetland => 8,
            Self::Mangroves => 9,
            Self::MossLichen => 10,
        }
    }

    /// Normalized vegetation density in `[0, 1]` for this class. Dense canopy
    /// classes approach one; water, snow, and built-up ground approach zero.
    pub const fn vegetation_density(self) -> f64 {
        match self {
            Self::TreeCover => 1.0,
            Self::Mangroves => 0.9,
            Self::HerbaceousWetland => 0.5,
            Self::Shrubland => 0.4,
            Self::Grassland => 0.35,
            Self::Cropland => 0.25,
            Self::MossLichen => 0.2,
            Self::BareSparse => 0.05,
            Self::BuiltUp => 0.02,
            Self::SnowIce | Self::PermanentWater => 0.0,
        }
    }
}

/// Provenance required for an accepted land-cover package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LandCoverMetadata {
    pub body: String,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub source: String,
    pub source_resolution_m: f64,
    pub nodata_policy: String,
    pub source_sha256: String,
    pub license: String,
    pub conversion_version: u32,
}

/// A regular terrain-radial latitude/longitude grid of broad surface classes.
#[derive(Debug, Clone)]
pub struct LandCoverPackage {
    width: u32,
    height: u32,
    west_deg: f64,
    south_deg: f64,
    east_deg: f64,
    north_deg: f64,
    metadata: LandCoverMetadata,
    classes: Vec<u8>,
}

impl LandCoverPackage {
    #[expect(
        clippy::too_many_arguments,
        reason = "Package metadata, coverage, and immutable classes are independently validated."
    )]
    pub fn from_samples(
        width: u32,
        height: u32,
        west_deg: f64,
        south_deg: f64,
        east_deg: f64,
        north_deg: f64,
        metadata: LandCoverMetadata,
        classes: Vec<u8>,
    ) -> Result<Self, LandCoverError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                LandCoverError::InvalidFormat("land-cover dimensions overflow".into())
            })?;
        if width < 2
            || height < 2
            || !(west_deg < east_deg && south_deg < north_deg)
            || classes.len() != expected
            || !metadata.source_resolution_m.is_finite()
            || metadata.source_resolution_m <= 0.0
            || metadata.body.is_empty()
            || metadata.coordinate_frame != "terrain-radial-degrees"
            || metadata.horizontal_datum.is_empty()
            || metadata.source.is_empty()
            || metadata.nodata_policy.is_empty()
            || metadata.license.is_empty()
            || metadata.conversion_version == 0
            || metadata.source_sha256.len() != 64
            || !metadata
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || classes.iter().any(|code| *code > 10)
        {
            return Err(LandCoverError::InvalidFormat(
                "invalid land-cover package".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            west_deg,
            south_deg,
            east_deg,
            north_deg,
            metadata,
            classes,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LandCoverError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(LandCoverError::InvalidFormat(
                "invalid land-cover header".into(),
            ));
        }
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at = |offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(LandCoverError::InvalidFormat(
                "unsupported land-cover version".into(),
            ));
        }
        let metadata_bytes = u32_at(12) as usize;
        let width = u32_at(16);
        let height = u32_at(20);
        let (west_deg, south_deg, east_deg, north_deg) =
            (f64_at(24), f64_at(32), f64_at(40), f64_at(48));
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                LandCoverError::InvalidFormat("land-cover dimensions overflow".into())
            })?;
        let metadata_end = HEADER_BYTES.checked_add(metadata_bytes).ok_or_else(|| {
            LandCoverError::InvalidFormat("land-cover metadata length overflows".into())
        })?;
        let expected_bytes = metadata_end.checked_add(count).ok_or_else(|| {
            LandCoverError::InvalidFormat("land-cover package length overflows".into())
        })?;
        if bytes.len() != expected_bytes {
            return Err(LandCoverError::InvalidFormat(
                "invalid land-cover payload length".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end]).map_err(|_| {
                LandCoverError::InvalidFormat("land-cover metadata is not UTF-8".into())
            })?,
        )
        .map_err(|error| {
            LandCoverError::InvalidFormat(format!("invalid land-cover metadata: {error}"))
        })?;
        let payload = &bytes[metadata_end..];
        if Sha256::digest(payload).as_slice() != &bytes[56..88] {
            return Err(LandCoverError::InvalidFormat(
                "land-cover checksum mismatch".into(),
            ));
        }
        Self::from_samples(
            width,
            height,
            west_deg,
            south_deg,
            east_deg,
            north_deg,
            metadata,
            payload.to_vec(),
        )
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, LandCoverError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), LandCoverError> {
        fs::write(path, self.write_bytes())?;
        Ok(())
    }

    /// Encode without touching the filesystem (used by tests and the converter).
    pub fn write_bytes(&self) -> Vec<u8> {
        let metadata = ron::to_string(&self.metadata).expect("metadata must encode");
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + self.classes.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        for value in [self.west_deg, self.south_deg, self.east_deg, self.north_deg] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&Sha256::digest(&self.classes));
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&self.classes);
        bytes
    }

    pub fn metadata(&self) -> &LandCoverMetadata {
        &self.metadata
    }

    pub fn coverage_bounds_deg(&self) -> (f64, f64, f64, f64) {
        (self.west_deg, self.south_deg, self.east_deg, self.north_deg)
    }

    /// Nearest-neighbor class at a coordinate, or `None` outside coverage.
    /// Nearest is intentional: the classes are categorical and bilinear
    /// interpolation of class ids is meaningless.
    pub fn sample_class(&self, latitude_deg: f64, longitude_deg: f64) -> Option<LandCoverClass> {
        if !(self.west_deg..=self.east_deg).contains(&longitude_deg)
            || !(self.south_deg..=self.north_deg).contains(&latitude_deg)
        {
            return None;
        }
        let x = ((longitude_deg - self.west_deg) / (self.east_deg - self.west_deg)
            * (self.width - 1) as f64)
            .round() as usize;
        let y = ((self.north_deg - latitude_deg) / (self.north_deg - self.south_deg)
            * (self.height - 1) as f64)
            .round() as usize;
        let x = x.min(self.width as usize - 1);
        let y = y.min(self.height as usize - 1);
        Some(LandCoverClass::from_code(
            self.classes[y * self.width as usize + x],
        ))
    }

    /// Presentation vegetation density at a coordinate, or `None` outside
    /// coverage. Callers combine this with the source's climate density.
    pub fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> Option<f64> {
        self.sample_class(latitude_deg, longitude_deg)
            .map(LandCoverClass::vegetation_density)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> LandCoverMetadata {
        LandCoverMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum: "WGS84".into(),
            source: "ESA WorldCover 2021 v200".into(),
            source_resolution_m: 10.0,
            nodata_policy: "fallback to climate density".into(),
            source_sha256: "0".repeat(64),
            license: "CC BY 4.0".into(),
            conversion_version: 1,
        }
    }

    fn package() -> LandCoverPackage {
        LandCoverPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            metadata(),
            vec![
                LandCoverClass::TreeCover.code(),
                LandCoverClass::Grassland.code(),
                LandCoverClass::PermanentWater.code(),
                LandCoverClass::SnowIce.code(),
            ],
        )
        .expect("valid package")
    }

    #[test]
    fn roundtrip_is_deterministic_and_rejects_corruption() {
        let package = package();
        let bytes = package.write_bytes();
        assert_eq!(bytes, package.write_bytes());
        let decoded = LandCoverPackage::from_bytes(&bytes).expect("roundtrip");
        assert_eq!(
            decoded.sample_class(0.9, 0.1),
            Some(LandCoverClass::TreeCover)
        );

        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(LandCoverPackage::from_bytes(&corrupted).is_err());

        let mut wrong_version = bytes;
        wrong_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(LandCoverPackage::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn sampling_is_nearest_and_outside_coverage_is_none() {
        // Rows are north-first, matching the terrain-radial grid convention.
        let package = package();
        assert_eq!(
            package.sample_class(0.9, 0.1),
            Some(LandCoverClass::TreeCover)
        );
        assert_eq!(
            package.sample_class(0.9, 0.9),
            Some(LandCoverClass::Grassland)
        );
        assert_eq!(
            package.sample_class(0.1, 0.1),
            Some(LandCoverClass::PermanentWater)
        );
        assert_eq!(
            package.sample_class(0.1, 0.9),
            Some(LandCoverClass::SnowIce)
        );
        assert_eq!(package.sample_class(2.0, 0.5), None);
    }

    #[test]
    fn class_density_is_bounded_and_ordered() {
        assert_eq!(LandCoverClass::TreeCover.vegetation_density(), 1.0);
        assert_eq!(LandCoverClass::PermanentWater.vegetation_density(), 0.0);
        assert_eq!(LandCoverClass::SnowIce.vegetation_density(), 0.0);
        for class in [
            LandCoverClass::TreeCover,
            LandCoverClass::Shrubland,
            LandCoverClass::Grassland,
            LandCoverClass::Cropland,
            LandCoverClass::BareSparse,
            LandCoverClass::BuiltUp,
        ] {
            let density = class.vegetation_density();
            assert!((0.0..=1.0).contains(&density));
        }
        assert!(
            LandCoverClass::TreeCover.vegetation_density()
                > LandCoverClass::Shrubland.vegetation_density()
        );
        // Unknown codes degrade to bare/sparse, never to vegetation.
        assert_eq!(LandCoverClass::from_code(250), LandCoverClass::BareSparse);
    }

    #[test]
    fn vegetation_density_outside_coverage_is_none() {
        let package = package();
        assert_eq!(package.vegetation_density(0.9, 0.1), Some(1.0));
        assert_eq!(package.vegetation_density(9.0, 9.0), None);
    }
}
