//! Versioned offline measured surface fields for presentation-only hydrology,
//! canopy cover, and canopy height constraints.
//!
//! A package stores measured water occurrence, tree-cover percentage, and mean
//! canopy height on a regular terrain-radial latitude/longitude grid. It is
//! intentionally separate from the categorical land-cover packages and from the
//! authoritative terrain source: the values refine vegetation placement, tree
//! height, and close-range materials, and they never feed height, collision,
//! altitude, or any physics value.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"CSSFLD\0\0";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 88;
/// Stored channels per sample: water occurrence percent, tree-cover percent, and
/// mean canopy height in whole metres. Zero height means no measurement.
const COMPONENTS: usize = 3;
/// Canonical channel order and semantics recorded in every package.
pub const SURFACE_FIELD_NAMES: [&str; COMPONENTS] =
    ["water_occurrence_pct", "tree_cover_pct", "canopy_height_m"];
/// Default native directory holding `.cssfld` packages.
pub const DEFAULT_SURFACE_FIELDS_DIR: &str = "assets/large_files/terrain/surface_fields_v1";

/// Failures when validating or decoding a surface-field package.
#[derive(Debug)]
pub enum SurfaceFieldsError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for SurfaceFieldsError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Bilinearly sampled measured fields. Percentages are in `[0, 100]`; canopy
/// height is in whole metres and zero means no measured height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceFieldsSample {
    water_occurrence_pct: f32,
    tree_cover_pct: f32,
    canopy_height_m: f32,
}

impl SurfaceFieldsSample {
    pub fn new(water_occurrence_pct: f32, tree_cover_pct: f32, canopy_height_m: f32) -> Self {
        Self {
            water_occurrence_pct: water_occurrence_pct.clamp(0.0, 100.0),
            tree_cover_pct: tree_cover_pct.clamp(0.0, 100.0),
            canopy_height_m: canopy_height_m.clamp(0.0, 255.0),
        }
    }

    pub fn water_occurrence_pct(&self) -> f32 {
        self.water_occurrence_pct
    }

    pub fn tree_cover_pct(&self) -> f32 {
        self.tree_cover_pct
    }

    pub fn canopy_height_m(&self) -> f32 {
        self.canopy_height_m
    }

    /// Fraction of the year a site was observed as water.
    pub fn water_occurrence(&self) -> f32 {
        self.water_occurrence_pct / 100.0
    }

    /// Measured tree-cover fraction.
    pub fn tree_cover(&self) -> f32 {
        self.tree_cover_pct / 100.0
    }
}

/// Read-only measured-field query shared by a package and a composed set.
pub trait SurfaceFieldsLookup {
    fn sample_fields(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SurfaceFieldsSample>;
}

/// Provenance required for an accepted measured-field package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SurfaceFieldsMetadata {
    pub body: String,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub vertical_datum: String,
    pub source: String,
    pub source_urls: Vec<String>,
    pub field_names: Vec<String>,
    pub source_resolution_m: f64,
    pub nodata_policy: String,
    pub source_sha256: String,
    pub license: String,
    pub attribution: String,
    pub conversion_version: u32,
}

/// A regular terrain-radial grid of measured water occurrence, tree cover, and
/// mean canopy height.
#[derive(Debug, Clone)]
pub struct SurfaceFieldsPackage {
    width: u32,
    height: u32,
    west_deg: f64,
    south_deg: f64,
    east_deg: f64,
    north_deg: f64,
    metadata: SurfaceFieldsMetadata,
    values: Vec<u8>,
}

impl SurfaceFieldsLookup for SurfaceFieldsPackage {
    fn sample_fields(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SurfaceFieldsSample> {
        SurfaceFieldsPackage::sample_fields(self, latitude_deg, longitude_deg)
    }
}

impl SurfaceFieldsPackage {
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
        metadata: SurfaceFieldsMetadata,
        values: Vec<u8>,
    ) -> Result<Self, SurfaceFieldsError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|count| count.checked_mul(COMPONENTS))
            .ok_or_else(|| {
                SurfaceFieldsError::InvalidFormat("surface-field dimensions overflow".into())
            })?;
        if width < 2
            || height < 2
            || !(west_deg < east_deg && south_deg < north_deg)
            || values.len() != expected
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
            || metadata.field_names.len() != COMPONENTS
            || metadata
                .field_names
                .iter()
                .zip(SURFACE_FIELD_NAMES.iter())
                .any(|(found, expected)| found.as_str() != *expected)
            || metadata.nodata_policy.is_empty()
            || metadata.license.is_empty()
            || metadata.attribution.is_empty()
            || metadata.conversion_version == 0
            || metadata.source_sha256.len() != 64
            || !metadata
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || values
                .as_chunks::<COMPONENTS>()
                .0
                .iter()
                .any(|sample| sample[0] > 100 || sample[1] > 100)
        {
            return Err(SurfaceFieldsError::InvalidFormat(
                "invalid surface-field package".into(),
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
            values,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SurfaceFieldsError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(SurfaceFieldsError::InvalidFormat(
                "invalid surface-field header".into(),
            ));
        }
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at = |offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(SurfaceFieldsError::InvalidFormat(
                "unsupported surface-field package version".into(),
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
            .ok_or_else(|| {
                SurfaceFieldsError::InvalidFormat("surface-field dimensions overflow".into())
            })?;
        let metadata_end = HEADER_BYTES.checked_add(metadata_bytes).ok_or_else(|| {
            SurfaceFieldsError::InvalidFormat("surface-field metadata length overflows".into())
        })?;
        let expected_bytes = metadata_end.checked_add(count).ok_or_else(|| {
            SurfaceFieldsError::InvalidFormat("surface-field package length overflows".into())
        })?;
        if bytes.len() != expected_bytes {
            return Err(SurfaceFieldsError::InvalidFormat(
                "invalid surface-field payload length".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end]).map_err(|_| {
                SurfaceFieldsError::InvalidFormat("surface-field metadata is not UTF-8".into())
            })?,
        )
        .map_err(|error| {
            SurfaceFieldsError::InvalidFormat(format!("invalid surface-field metadata: {error}"))
        })?;
        let payload = &bytes[metadata_end..];
        if Sha256::digest(payload).as_slice() != &bytes[56..88] {
            return Err(SurfaceFieldsError::InvalidFormat(
                "surface-field checksum mismatch".into(),
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

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, SurfaceFieldsError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), SurfaceFieldsError> {
        fs::write(path, self.write_bytes())?;
        Ok(())
    }

    /// Encode without touching the filesystem (used by tests and the converter).
    pub fn write_bytes(&self) -> Vec<u8> {
        let metadata = ron::to_string(&self.metadata).expect("metadata must encode");
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + self.values.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        for value in [self.west_deg, self.south_deg, self.east_deg, self.north_deg] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&Sha256::digest(&self.values));
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&self.values);
        bytes
    }

    pub fn metadata(&self) -> &SurfaceFieldsMetadata {
        &self.metadata
    }

    pub fn coverage_bounds_deg(&self) -> (f64, f64, f64, f64) {
        (self.west_deg, self.south_deg, self.east_deg, self.north_deg)
    }

    /// Bilinear fields at a coordinate, or `None` outside coverage.
    pub fn sample_fields(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
    ) -> Option<SurfaceFieldsSample> {
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
            f32::from(self.values[(y * width + x) * COMPONENTS + channel])
        };
        let sample = |channel: usize| {
            let top = component(channel, x0, y0) * (1.0 - tx) + component(channel, x1, y0) * tx;
            let bottom = component(channel, x0, y1) * (1.0 - tx) + component(channel, x1, y1) * tx;
            top * (1.0 - ty) + bottom * ty
        };
        Some(SurfaceFieldsSample::new(sample(0), sample(1), sample(2)))
    }
}

/// Ordered set of measured-field packages. Later packages win where coverage
/// overlaps, so regional 1 arc-second grids refine any coarser fallback.
#[derive(Debug, Clone, Default)]
pub struct SurfaceFieldsSet {
    packages: Vec<SurfaceFieldsPackage>,
}

impl SurfaceFieldsSet {
    pub fn new(packages: Vec<SurfaceFieldsPackage>) -> Self {
        Self { packages }
    }

    pub fn from_paths(
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, SurfaceFieldsError> {
        let packages = paths
            .into_iter()
            .map(|path| SurfaceFieldsPackage::from_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        if packages.is_empty() {
            return Err(SurfaceFieldsError::InvalidFormat(
                "surface-field set requires at least one package".into(),
            ));
        }
        Ok(Self::new(packages))
    }

    pub fn packages(&self) -> &[SurfaceFieldsPackage] {
        &self.packages
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }
}

impl SurfaceFieldsLookup for SurfaceFieldsSet {
    fn sample_fields(&self, latitude_deg: f64, longitude_deg: f64) -> Option<SurfaceFieldsSample> {
        self.packages
            .iter()
            .rev()
            .find_map(|package| package.sample_fields(latitude_deg, longitude_deg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> SurfaceFieldsMetadata {
        SurfaceFieldsMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum:
                "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid".into(),
            vertical_datum: "not applicable (surface water, canopy cover, canopy height)".into(),
            source: "JRC Global Surface Water + Hansen Global Forest Change + Meta/WRI Canopy Height Maps".into(),
            source_urls: vec!["https://example.invalid/source.tif".into()],
            field_names: SURFACE_FIELD_NAMES
                .iter()
                .map(|name| name.to_string())
                .collect(),
            source_resolution_m: 30.0,
            nodata_policy:
                "nodata falls back to zero water occurrence, zero tree cover, and zero measured canopy height"
                    .into(),
            source_sha256: "0".repeat(64),
            license: "CC BY 4.0".into(),
            attribution: "European Commission JRC; Hansen/UMD/Google/USGS/NASA; Meta/WRI Canopy Height Maps"
                .into(),
            conversion_version: 2,
        }
    }

    fn package() -> SurfaceFieldsPackage {
        SurfaceFieldsPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            metadata(),
            vec![10, 80, 25, 20, 60, 15, 30, 40, 5, 0, 0, 0],
        )
        .expect("valid package")
    }

    #[test]
    fn roundtrip_is_deterministic_and_rejects_corruption() {
        let package = package();
        let bytes = package.write_bytes();
        assert_eq!(bytes, package.write_bytes());
        let decoded = SurfaceFieldsPackage::from_bytes(&bytes).expect("roundtrip");
        let fields = decoded.sample_fields(1.0, 0.0).expect("covered");
        assert!((fields.water_occurrence_pct() - 10.0).abs() < 1e-6);
        assert!((fields.tree_cover_pct() - 80.0).abs() < 1e-6);
        assert!((fields.canopy_height_m() - 25.0).abs() < 1e-6);

        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(SurfaceFieldsPackage::from_bytes(&corrupted).is_err());

        let mut wrong_version = bytes;
        wrong_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(SurfaceFieldsPackage::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn sampling_is_bilinear_and_outside_coverage_is_none() {
        let package = package();
        let northwest = package.sample_fields(1.0, 0.0).expect("covered");
        assert!((northwest.water_occurrence_pct() - 10.0).abs() < 1e-6);
        let center = package.sample_fields(0.5, 0.5).expect("covered");
        assert!((center.water_occurrence_pct() - 15.0).abs() < 1e-6);
        assert!((center.tree_cover_pct() - 45.0).abs() < 1e-6);
        assert!((center.canopy_height_m() - 11.25).abs() < 1e-6);
        assert_eq!(package.sample_fields(2.0, 0.5), None);
    }

    #[test]
    fn invalid_values_or_field_names_are_rejected() {
        let mut bad_value = vec![0u8; 12];
        bad_value[0] = 120;
        assert!(SurfaceFieldsPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            metadata(),
            bad_value
        )
        .is_err());
        let mut bad_metadata = metadata();
        bad_metadata.field_names[1] = "canopy_height_m".into();
        assert!(SurfaceFieldsPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            bad_metadata,
            vec![0u8; 12]
        )
        .is_err());
    }

    #[test]
    fn later_packages_override_overlapping_coverage() {
        let coarse = SurfaceFieldsPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            4.0,
            4.0,
            metadata(),
            vec![50, 20, 10, 50, 20, 10, 50, 20, 10, 50, 20, 10],
        )
        .expect("valid package");
        let regional = SurfaceFieldsPackage::from_samples(
            2,
            2,
            1.0,
            1.0,
            3.0,
            3.0,
            metadata(),
            vec![0, 90, 30, 0, 90, 30, 0, 90, 30, 0, 90, 30],
        )
        .expect("valid package");
        let set = SurfaceFieldsSet::new(vec![coarse, regional]);
        assert_eq!(set.package_count(), 2);
        let refined = set.sample_fields(2.0, 2.0).expect("covered");
        assert!((refined.water_occurrence_pct() - 0.0).abs() < 1e-6);
        assert!((refined.tree_cover_pct() - 90.0).abs() < 1e-6);
        let fallback = set.sample_fields(4.0, 0.0).expect("covered");
        assert!((fallback.water_occurrence_pct() - 50.0).abs() < 1e-6);
        assert_eq!(set.sample_fields(8.0, 8.0), None);
    }
}
