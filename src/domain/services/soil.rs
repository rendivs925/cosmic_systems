//! Versioned offline soil-texture packages for presentation-only materials.
//!
//! A soil package stores measured 0-5 cm clay, sand, and silt mass fractions on
//! a regular terrain-radial latitude/longitude grid. It is deliberately
//! separate from the authoritative terrain source and from the categorical
//! land-cover packages: soil is continuous, and it never feeds height,
//! collision, altitude, or any physics value. When a package is absent,
//! malformed, or out of coverage the caller keeps the source-derived material
//! untouched.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"CSSOIL\0\0";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 88;
/// Stored components per sample: clay, sand, silt, in integer percent.
const COMPONENTS: usize = 3;

/// Default native directory holding `.cssoil` packages. Every package is loaded
/// in file-name order so a regional grid refines the global fallback.
pub const DEFAULT_SOIL_DIR: &str = "assets/large_files/terrain/soil_v1";

/// Failures when validating or decoding a soil package.
#[derive(Debug)]
pub enum SoilError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for SoilError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Normalized measured soil texture. Fractions always sum to one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SoilTexture {
    clay: f32,
    sand: f32,
    silt: f32,
}

impl SoilTexture {
    /// Normalize non-negative percentages. A degenerate zero total falls back
    /// to a neutral loam so a nodata placeholder never turns ground black.
    pub fn from_percentages(clay: f32, sand: f32, silt: f32) -> Self {
        let total = clay.max(0.0) + sand.max(0.0) + silt.max(0.0);
        if !total.is_finite() || total <= f32::EPSILON {
            return Self::loam();
        }
        Self {
            clay: clay.max(0.0) / total,
            sand: sand.max(0.0) / total,
            silt: silt.max(0.0) / total,
        }
    }

    /// Neutral loam used when no measured texture is available.
    pub const fn loam() -> Self {
        Self {
            clay: 1.0 / 3.0,
            sand: 1.0 / 3.0,
            silt: 1.0 / 3.0,
        }
    }

    pub fn clay(&self) -> f32 {
        self.clay
    }

    pub fn sand(&self) -> f32 {
        self.sand
    }

    pub fn silt(&self) -> f32 {
        self.silt
    }

    /// Dominant measured component in `[0, 2]`: clay, sand, or silt.
    pub fn dominant(&self) -> usize {
        if self.clay >= self.sand && self.clay >= self.silt {
            0
        } else if self.sand >= self.silt {
            1
        } else {
            2
        }
    }
}

/// Read-only soil-texture query shared by a single package and a composed set.
/// Values are continuous, so sampling is bilinear rather than nearest.
pub trait SoilLookup {
    fn sample_texture(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SoilTexture>;
}

/// Provenance required for an accepted soil package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoilMetadata {
    pub body: String,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub vertical_datum: String,
    pub source: String,
    /// One reviewed GetCoverage URL per measured variable.
    pub source_urls: Vec<String>,
    pub source_resolution_m: f64,
    pub nodata_policy: String,
    pub source_sha256: String,
    pub license: String,
    pub attribution: String,
    pub conversion_version: u32,
}

/// A regular terrain-radial latitude/longitude grid of clay, sand, and silt
/// integer percentages.
#[derive(Debug, Clone)]
pub struct SoilPackage {
    width: u32,
    height: u32,
    west_deg: f64,
    south_deg: f64,
    east_deg: f64,
    north_deg: f64,
    metadata: SoilMetadata,
    components: Vec<u8>,
}

impl SoilLookup for SoilPackage {
    fn sample_texture(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SoilTexture> {
        SoilPackage::sample_texture(self, latitude_deg, longitude_deg)
    }
}

impl SoilPackage {
    #[expect(
        clippy::too_many_arguments,
        reason = "Package metadata, coverage, and immutable samples are independently validated."
    )]
    pub fn from_samples(
        width: u32,
        height: u32,
        west_deg: f64,
        south_deg: f64,
        east_deg: f64,
        north_deg: f64,
        metadata: SoilMetadata,
        components: Vec<u8>,
    ) -> Result<Self, SoilError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(COMPONENTS))
            .ok_or_else(|| SoilError::InvalidFormat("soil dimensions overflow".into()))?;
        if width < 2
            || height < 2
            || !(west_deg < east_deg && south_deg < north_deg)
            || components.len() != expected
            || !metadata.source_resolution_m.is_finite()
            || metadata.source_resolution_m <= 0.0
            || metadata.body.is_empty()
            || metadata.coordinate_frame != "terrain-radial-degrees"
            || metadata.horizontal_datum.is_empty()
            || metadata.vertical_datum.is_empty()
            || metadata.source.is_empty()
            || metadata.source_urls.is_empty()
            || metadata
                .source_urls
                .iter()
                .any(|url| !url.starts_with("https://"))
            || metadata.nodata_policy.is_empty()
            || metadata.license.is_empty()
            || metadata.attribution.is_empty()
            || metadata.conversion_version == 0
            || metadata.source_sha256.len() != 64
            || !metadata
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || components
                .chunks_exact(COMPONENTS)
                .any(|sample| sample.iter().any(|value| *value > 100) || sample_sum(sample) != 100)
        {
            return Err(SoilError::InvalidFormat("invalid soil package".into()));
        }
        Ok(Self {
            width,
            height,
            west_deg,
            south_deg,
            east_deg,
            north_deg,
            metadata,
            components,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SoilError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(SoilError::InvalidFormat("invalid soil header".into()));
        }
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at = |offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(SoilError::InvalidFormat(
                "unsupported soil package version".into(),
            ));
        }
        let metadata_bytes = u32_at(12) as usize;
        let width = u32_at(16);
        let height = u32_at(20);
        let (west_deg, south_deg, east_deg, north_deg) =
            (f64_at(24), f64_at(32), f64_at(40), f64_at(48));
        let count = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(COMPONENTS))
            .ok_or_else(|| SoilError::InvalidFormat("soil dimensions overflow".into()))?;
        let metadata_end = HEADER_BYTES
            .checked_add(metadata_bytes)
            .ok_or_else(|| SoilError::InvalidFormat("soil metadata length overflows".into()))?;
        let expected_bytes = metadata_end
            .checked_add(count)
            .ok_or_else(|| SoilError::InvalidFormat("soil package length overflows".into()))?;
        if bytes.len() != expected_bytes {
            return Err(SoilError::InvalidFormat(
                "invalid soil payload length".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end])
                .map_err(|_| SoilError::InvalidFormat("soil metadata is not UTF-8".into()))?,
        )
        .map_err(|error| SoilError::InvalidFormat(format!("invalid soil metadata: {error}")))?;
        let payload = &bytes[metadata_end..];
        if Sha256::digest(payload).as_slice() != &bytes[56..88] {
            return Err(SoilError::InvalidFormat("soil checksum mismatch".into()));
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

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, SoilError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), SoilError> {
        fs::write(path, self.write_bytes())?;
        Ok(())
    }

    /// Encode without touching the filesystem (used by tests and the converter).
    pub fn write_bytes(&self) -> Vec<u8> {
        let metadata = ron::to_string(&self.metadata).expect("metadata must encode");
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + self.components.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        for value in [self.west_deg, self.south_deg, self.east_deg, self.north_deg] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&Sha256::digest(&self.components));
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&self.components);
        bytes
    }

    pub fn metadata(&self) -> &SoilMetadata {
        &self.metadata
    }

    pub fn coverage_bounds_deg(&self) -> (f64, f64, f64, f64) {
        (self.west_deg, self.south_deg, self.east_deg, self.north_deg)
    }

    /// Bilinear texture at a coordinate, or `None` outside coverage.
    pub fn sample_texture(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SoilTexture> {
        if !(self.west_deg..=self.east_deg).contains(&longitude_deg)
            || !(self.south_deg..=self.north_deg).contains(&latitude_deg)
        {
            return None;
        }
        let x = ((longitude_deg - self.west_deg) / (self.east_deg - self.west_deg)
            * (self.width - 1) as f64)
            .clamp(0.0, (self.width - 1) as f64);
        let y = ((self.north_deg - latitude_deg) / (self.north_deg - self.south_deg)
            * (self.height - 1) as f64)
            .clamp(0.0, (self.height - 1) as f64);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1).min(self.width as usize - 1);
        let y1 = (y0 + 1).min(self.height as usize - 1);
        let tx = (x - x0 as f64) as f32;
        let ty = (y - y0 as f64) as f32;
        let width = self.width as usize;
        let component = |channel: usize, x: usize, y: usize| {
            f32::from(self.components[(y * width + x) * COMPONENTS + channel])
        };
        let sample = |channel: usize| {
            let top = component(channel, x0, y0) * (1.0 - tx) + component(channel, x1, y0) * tx;
            let bottom = component(channel, x0, y1) * (1.0 - tx) + component(channel, x1, y1) * tx;
            top * (1.0 - ty) + bottom * ty
        };
        Some(SoilTexture::from_percentages(
            sample(0),
            sample(1),
            sample(2),
        ))
    }
}

fn sample_sum(sample: &[u8]) -> u32 {
    sample.iter().map(|value| u32::from(*value)).sum()
}

/// Ordered set of soil packages. Later packages win where coverage overlaps,
/// which lets a regional 250 m grid refine a coarse global fallback without
/// changing the binary package format.
#[derive(Debug, Clone, Default)]
pub struct SoilSet {
    packages: Vec<SoilPackage>,
}

impl SoilSet {
    pub fn new(packages: Vec<SoilPackage>) -> Self {
        Self { packages }
    }

    pub fn from_paths(
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, SoilError> {
        let packages = paths
            .into_iter()
            .map(|path| SoilPackage::from_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        if packages.is_empty() {
            return Err(SoilError::InvalidFormat(
                "soil set requires at least one package".into(),
            ));
        }
        Ok(Self::new(packages))
    }

    pub fn packages(&self) -> &[SoilPackage] {
        &self.packages
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }
}

impl SoilLookup for SoilSet {
    fn sample_texture(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SoilTexture> {
        self.packages
            .iter()
            .rev()
            .find_map(|package| package.sample_texture(latitude_deg, longitude_deg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> SoilMetadata {
        SoilMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum:
                "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid".into(),
            vertical_datum: "not applicable (surface soil texture)".into(),
            source: "SoilGrids 2.0, 0-5 cm mean".into(),
            source_urls: vec!["https://maps.isric.org/mapserv?map=/map/clay.map".into()],
            source_resolution_m: 250.0,
            nodata_policy: "nodata falls back to neutral loam".into(),
            source_sha256: "0".repeat(64),
            license: "CC BY 4.0".into(),
            attribution: "ISRIC - World Soil Information".into(),
            conversion_version: 1,
        }
    }

    fn sample(clay: u8, sand: u8, silt: u8) -> [u8; 3] {
        [clay, sand, silt]
    }

    fn package() -> SoilPackage {
        SoilPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            metadata(),
            vec![
                sample(50, 30, 20),
                sample(10, 70, 20),
                sample(20, 20, 60),
                sample(33, 33, 34),
            ]
            .into_iter()
            .flatten()
            .collect(),
        )
        .expect("valid package")
    }

    #[test]
    fn roundtrip_is_deterministic_and_rejects_corruption() {
        let package = package();
        let bytes = package.write_bytes();
        assert_eq!(bytes, package.write_bytes());
        let decoded = SoilPackage::from_bytes(&bytes).expect("roundtrip");
        let texture = decoded.sample_texture(1.0, 0.0).expect("covered");
        assert!((texture.clay() - 0.5).abs() < 1e-6);
        assert!((texture.sand() - 0.3).abs() < 1e-6);

        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(SoilPackage::from_bytes(&corrupted).is_err());

        let mut wrong_version = bytes;
        wrong_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(SoilPackage::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn sampling_is_bilinear_and_outside_coverage_is_none() {
        // Rows are north-first, matching the terrain-radial grid convention.
        let package = package();
        let northwest = package.sample_texture(1.0, 0.0).expect("covered");
        assert!((northwest.clay() - 0.5).abs() < 1e-6);
        let northeast = package.sample_texture(1.0, 1.0).expect("covered");
        assert!((northeast.sand() - 0.7).abs() < 1e-6);
        let center = package.sample_texture(0.5, 0.5).expect("covered");
        let mean_clay = (0.50 + 0.10 + 0.20 + 0.33) / 4.0;
        assert!((center.clay() - mean_clay).abs() < 2e-3);
        assert_eq!(package.sample_texture(2.0, 0.5), None);
    }

    #[test]
    fn invalid_samples_are_rejected() {
        let build = |components: Vec<u8>| {
            SoilPackage::from_samples(2, 2, 0.0, 0.0, 1.0, 1.0, metadata(), components)
        };
        assert!(build(vec![0; 11]).is_err());
        let mut wrong_sum = vec![0u8; 12];
        wrong_sum[..3].copy_from_slice(&[50, 30, 30]);
        assert!(build(wrong_sum).is_err());
        let mut above_hundred = vec![0u8; 12];
        above_hundred[..3].copy_from_slice(&[120, 0, 0]);
        assert!(build(above_hundred).is_err());
    }

    #[test]
    fn texture_normalizes_and_reports_the_dominant_component() {
        let texture = SoilTexture::from_percentages(60.0, 30.0, 10.0);
        assert!((texture.clay() - 0.6).abs() < 1e-6);
        assert_eq!(texture.dominant(), 0);
        assert_eq!(
            SoilTexture::from_percentages(0.0, 0.0, 0.0),
            SoilTexture::loam()
        );
        assert_eq!(SoilTexture::loam().dominant(), 0);
    }
    #[test]
    fn later_packages_override_overlapping_coverage() {
        let coarse = SoilPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            4.0,
            4.0,
            metadata(),
            vec![sample(50, 30, 20); 4].into_iter().flatten().collect(),
        )
        .expect("valid package");
        let regional = SoilPackage::from_samples(
            2,
            2,
            1.0,
            1.0,
            3.0,
            3.0,
            metadata(),
            vec![sample(10, 70, 20); 4].into_iter().flatten().collect(),
        )
        .expect("valid package");
        let set = SoilSet::new(vec![coarse, regional]);
        assert_eq!(set.package_count(), 2);
        let refined = set.sample_texture(2.0, 2.0).expect("covered");
        assert!((refined.sand() - 0.7).abs() < 1e-6);
        let fallback = set.sample_texture(4.0, 0.0).expect("covered");
        assert!((fallback.clay() - 0.5).abs() < 1e-6);
        assert_eq!(set.sample_texture(8.0, 8.0), None);
    }
}
