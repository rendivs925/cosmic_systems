//! Coarse geoid undulation model for ellipsoidal terrain heights.
//!
//! ETOPO 2022 terrain heights are stored on the WGS 84 ellipsoid. Sea level is
//! not the ellipsoid, so a source that classifies land, water, or surface
//! appearance must subtract the local geoid undulation:
//!
//! ```text
//! H_orthometric = h_ellipsoid - N_geoid
//! ```
//!
//! This module owns that single sampling law. The grid is an offline-derived
//! EGM2008 product; it performs no network access and has no Bevy dependency.

use super::global_raster::GlobalRasterSpec;
use std::fs;
use std::path::Path;

/// Failure while loading a geoid grid.
#[derive(Debug)]
pub enum GeoidError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl std::fmt::Display for GeoidError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "cannot read geoid grid: {error}"),
            Self::InvalidFormat(message) => write!(formatter, "invalid geoid grid: {message}"),
        }
    }
}

impl std::error::Error for GeoidError {}

impl From<std::io::Error> for GeoidError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// A validated global geoid undulation grid.
#[derive(Debug)]
pub struct GeoidModel {
    spec: GlobalRasterSpec,
    raw: Vec<u8>,
}

impl GeoidModel {
    /// Load and validate a geoid raster. The caller pins the expected layout;
    /// an unexpected size is rejected rather than sampled out of bounds.
    pub fn from_path(path: impl AsRef<Path>, spec: GlobalRasterSpec) -> Result<Self, GeoidError> {
        let raw = fs::read(path)?;
        Self::from_raw(raw, spec)
    }

    pub fn from_raw(raw: Vec<u8>, spec: GlobalRasterSpec) -> Result<Self, GeoidError> {
        spec.validate().map_err(GeoidError::InvalidFormat)?;
        let expected = spec
            .expected_bytes()
            .ok_or_else(|| GeoidError::InvalidFormat("geoid grid size overflows".into()))?;
        if raw.len() != expected {
            return Err(GeoidError::InvalidFormat(format!(
                "geoid grid is {} bytes; expected {expected}",
                raw.len()
            )));
        }
        Ok(Self { spec, raw })
    }

    /// EGM2008 undulation at a geographic position, in metres.
    pub fn undulation_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.spec.sample_m(&self.raw, latitude_deg, longitude_deg)
    }

    pub fn spec(&self) -> GlobalRasterSpec {
        self.spec
    }

    pub fn byte_len(&self) -> usize {
        self.raw.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GlobalRasterSpec {
        GlobalRasterSpec {
            columns: 4,
            rows: 2,
            longitude_origin_deg: -135.0,
            longitude_step_deg: 90.0,
            longitude_period_columns: 4,
            latitude_origin_deg: 45.0,
            latitude_step_deg: 90.0,
            height_scale_m: 0.01,
        }
    }

    fn raw() -> Vec<u8> {
        [[-100i16, 0, 100, 200], [300, 400, 500, 600]]
            .iter()
            .flat_map(|row| row.iter())
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    #[test]
    fn samples_scaled_undulation() {
        let model = GeoidModel::from_raw(raw(), spec()).unwrap();
        assert_eq!(model.undulation_m(45.0, -135.0), -1.0);
        assert_eq!(model.undulation_m(-45.0, 45.0), 5.0);
    }

    #[test]
    fn rejects_wrong_byte_length() {
        assert!(GeoidModel::from_raw(raw()[..8].to_vec(), spec()).is_err());
    }

    #[test]
    fn rejects_invalid_spec() {
        let mut spec = spec();
        spec.height_scale_m = 0.0;
        assert!(GeoidModel::from_raw(raw(), spec).is_err());
    }
}
