//! Immutable offline local-elevation packages for measured terrain coverage.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"CSLDEM\0\0";
const VERSION: u32 = 2;
const HEADER_BYTES: usize = 88;

#[derive(Debug)]
pub enum LocalElevationError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for LocalElevationError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// Provenance required for an accepted local elevation package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalElevationMetadata {
    pub body: String,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub vertical_datum: String,
    pub source_resolution_m: f64,
    pub nodata_policy: String,
    pub source_sha256: String,
    pub license: String,
    pub conversion_version: u32,
    pub blend_border_m: f64,
}

/// A regular terrain-radial latitude/longitude grid whose valid samples are terrain-datum meters.
#[derive(Debug, Clone)]
pub struct LocalElevationPackage {
    width: u32,
    height: u32,
    west_deg: f64,
    south_deg: f64,
    east_deg: f64,
    north_deg: f64,
    metadata: LocalElevationMetadata,
    samples_m: Vec<f32>,
}

impl LocalElevationPackage {
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
        metadata: LocalElevationMetadata,
        samples_m: Vec<f32>,
    ) -> Result<Self, LocalElevationError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                LocalElevationError::InvalidFormat("local DEM dimensions overflow".into())
            })?;
        if width < 2
            || height < 2
            || !(west_deg < east_deg && south_deg < north_deg)
            || samples_m.len() != expected
            || !metadata.source_resolution_m.is_finite()
            || metadata.source_resolution_m <= 0.0
            || !metadata.blend_border_m.is_finite()
            || metadata.blend_border_m < 0.0
            || metadata.body.is_empty()
            || metadata.coordinate_frame != "terrain-radial-degrees"
            || metadata.horizontal_datum.is_empty()
            || metadata.vertical_datum.is_empty()
            || metadata.nodata_policy.is_empty()
            || metadata.license.is_empty()
            || metadata.conversion_version == 0
            || metadata.source_sha256.len() != 64
            || !metadata
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(LocalElevationError::InvalidFormat(
                "invalid local DEM coverage".into(),
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
            samples_m,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, LocalElevationError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(LocalElevationError::InvalidFormat(
                "invalid local DEM header".into(),
            ));
        }
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at = |offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(LocalElevationError::InvalidFormat(
                "unsupported local DEM version".into(),
            ));
        }
        let metadata_bytes = u32_at(12) as usize;
        let width = u32_at(16);
        let height = u32_at(20);
        let (west_deg, south_deg, east_deg, north_deg) =
            (f64_at(24), f64_at(32), f64_at(40), f64_at(48));
        if width < 2 || height < 2 || !(west_deg < east_deg && south_deg < north_deg) {
            return Err(LocalElevationError::InvalidFormat(
                "invalid local DEM coverage".into(),
            ));
        }
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                LocalElevationError::InvalidFormat("local DEM dimensions overflow".into())
            })?;
        let metadata_end = HEADER_BYTES.checked_add(metadata_bytes).ok_or_else(|| {
            LocalElevationError::InvalidFormat("local DEM metadata length overflows".into())
        })?;
        let payload_bytes = count.checked_mul(4).ok_or_else(|| {
            LocalElevationError::InvalidFormat("local DEM payload length overflows".into())
        })?;
        let expected_bytes = metadata_end.checked_add(payload_bytes).ok_or_else(|| {
            LocalElevationError::InvalidFormat("local DEM package length overflows".into())
        })?;
        if bytes.len() != expected_bytes {
            return Err(LocalElevationError::InvalidFormat(
                "invalid local DEM payload length".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end]).map_err(|_| {
                LocalElevationError::InvalidFormat("local DEM metadata is not UTF-8".into())
            })?,
        )
        .map_err(|error| {
            LocalElevationError::InvalidFormat(format!("invalid local DEM metadata: {error}"))
        })?;
        let sample_payload = &bytes[metadata_end..];
        if Sha256::digest(sample_payload).as_slice() != &bytes[56..88] {
            return Err(LocalElevationError::InvalidFormat(
                "local DEM runtime checksum mismatch".into(),
            ));
        }
        let (sample_bytes, remainder) = sample_payload.as_chunks::<4>();
        debug_assert!(remainder.is_empty());
        let samples_m = sample_bytes
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        Self::from_samples(
            width, height, west_deg, south_deg, east_deg, north_deg, metadata, samples_m,
        )
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, LocalElevationError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), LocalElevationError> {
        let metadata = ron::to_string(&self.metadata).map_err(|error| {
            LocalElevationError::InvalidFormat(format!(
                "unable to encode local DEM metadata: {error}"
            ))
        })?;
        let mut payload = Vec::with_capacity(self.samples_m.len() * 4);
        for value in &self.samples_m {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + payload.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        for value in [self.west_deg, self.south_deg, self.east_deg, self.north_deg] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&Sha256::digest(&payload));
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&payload);
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn metadata(&self) -> &LocalElevationMetadata {
        &self.metadata
    }

    pub fn coverage_bounds_deg(&self) -> (f64, f64, f64, f64) {
        (self.west_deg, self.south_deg, self.east_deg, self.north_deg)
    }

    /// Distance to the rectangular package boundary in degrees. Callers use it
    /// only to blend an otherwise valid local sample into its fallback source.
    pub fn edge_distance_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        (latitude_deg - self.south_deg)
            .min(self.north_deg - latitude_deg)
            .min(longitude_deg - self.west_deg)
            .min(self.east_deg - longitude_deg)
            .max(0.0)
    }

    /// Conservative bounds across valid terrain-datum samples only.
    pub fn elevation_bounds_m(&self) -> Option<(f64, f64)> {
        self.samples_m
            .iter()
            .copied()
            .filter(|value| value.is_finite())
            .map(f64::from)
            .fold(None, |bounds, value| match bounds {
                Some((min_m, max_m)) => Some((min_m.min(value), max_m.max(value))),
                None => Some((value, value)),
            })
    }

    /// Returns no value outside coverage or when any interpolation corner is nodata.
    pub fn sample_m(&self, latitude_deg: f64, longitude_deg: f64) -> Option<f64> {
        if !(self.west_deg..=self.east_deg).contains(&longitude_deg)
            || !(self.south_deg..=self.north_deg).contains(&latitude_deg)
        {
            return None;
        }
        let x = (longitude_deg - self.west_deg) / (self.east_deg - self.west_deg)
            * (self.width - 1) as f64;
        let y = (self.north_deg - latitude_deg) / (self.north_deg - self.south_deg)
            * (self.height - 1) as f64;
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = (
            (x0 + 1).min(self.width as usize - 1),
            (y0 + 1).min(self.height as usize - 1),
        );
        let at = |x, y| self.samples_m[y * self.width as usize + x];
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        if [a, b, c, d].iter().any(|value| !value.is_finite()) {
            return None;
        }
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        Some((a as f64 + (b - a) as f64 * tx) * (1.0 - ty) + (c as f64 + (d - c) as f64 * tx) * ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bilinear_sampling_is_deterministic_and_rejects_outside_coverage() {
        let package = LocalElevationPackage::from_samples(
            2,
            2,
            0.0,
            0.0,
            1.0,
            1.0,
            test_metadata(),
            vec![1.0, 3.0, 5.0, 7.0],
        )
        .unwrap();
        assert_eq!(package.sample_m(0.5, 0.5), Some(4.0));
        assert_eq!(package.sample_m(2.0, 0.5), None);
    }

    fn test_metadata() -> LocalElevationMetadata {
        LocalElevationMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum: "WGS84".into(),
            vertical_datum: "test".into(),
            source_resolution_m: 1.0,
            nodata_policy: "non-finite samples fall back".into(),
            source_sha256: "0".repeat(64),
            license: "test".into(),
            conversion_version: 1,
            blend_border_m: 100.0,
        }
    }
}
