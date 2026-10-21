//! Bilinear sampling for global equirectangular signed-`i16` rasters.
//!
//! Offline converters normalize source DEMs into a north-to-south row-major
//! little-endian metre raster before cube-sphere packaging. This module is the
//! one sampling law shared by those converters; it performs no I/O and does not
//! depend on Bevy.

/// Signed 16-bit little-endian samples are the only encoding accepted by the
/// cube-sphere DEM package.
pub const GLOBAL_RASTER_HEIGHT_BYTES: usize = 2;

/// Layout of a global equirectangular raster. Origins are sample centers, not
/// cell edges, so grid-registered and pixel-registered sources use the same
/// interpolation path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlobalRasterSpec {
    pub columns: usize,
    pub rows: usize,
    /// Longitude of column zero's sample center, in degrees east.
    pub longitude_origin_deg: f64,
    pub longitude_step_deg: f64,
    /// Number of columns in one 360-degree wrap. A duplicated seam column is
    /// stored in `columns` but is not part of the periodic span.
    pub longitude_period_columns: usize,
    /// Latitude of row zero's sample center, in degrees north.
    pub latitude_origin_deg: f64,
    pub latitude_step_deg: f64,
    /// Metres represented by one signed 16-bit sample. Height products use
    /// `1.0`; compact geoid grids use centimetres (`0.01`).
    pub height_scale_m: f64,
}

impl GlobalRasterSpec {
    /// ETOPO1 grid-registered raster: 1 arc-minute samples with duplicated
    /// first/last longitude columns covering the seam.
    pub const fn etopo1() -> Self {
        Self {
            columns: 21_601,
            rows: 10_801,
            longitude_origin_deg: -180.0,
            longitude_step_deg: 360.0 / 21_600.0,
            longitude_period_columns: 21_600,
            latitude_origin_deg: 90.0,
            latitude_step_deg: 180.0 / 10_800.0,
            height_scale_m: 1.0,
        }
    }

    /// ETOPO 2022 30 arc-second pixel-registered raster normalized by
    /// `scripts/ingest_etopo2022.py`: 43200x21600 samples, no seam duplicate.
    pub const fn etopo2022_30s() -> Self {
        Self {
            columns: 43_200,
            rows: 21_600,
            longitude_origin_deg: -180.0 + 0.5 / 120.0,
            longitude_step_deg: 1.0 / 120.0,
            longitude_period_columns: 43_200,
            latitude_origin_deg: 90.0 - 0.5 / 120.0,
            latitude_step_deg: 1.0 / 120.0,
            height_scale_m: 1.0,
        }
    }

    /// EGM2008 geoid undulation sampled every 0.5 degrees by
    /// `scripts/ingest_egm2008_geoid.py`. Samples are centimetres so a global
    /// grid stays compact while retaining decimetre precision.
    pub const fn egm2008_geoid_0p5deg() -> Self {
        Self {
            columns: 720,
            rows: 360,
            longitude_origin_deg: -179.995_833_333_3,
            longitude_step_deg: 0.5,
            longitude_period_columns: 720,
            latitude_origin_deg: 89.504_166_666_7,
            latitude_step_deg: 0.5,
            height_scale_m: 0.01,
        }
    }

    pub fn expected_bytes(self) -> Option<usize> {
        self.columns
            .checked_mul(self.rows)?
            .checked_mul(GLOBAL_RASTER_HEIGHT_BYTES)
    }

    /// Reject layouts that would make indexing or interpolation undefined.
    pub fn validate(self) -> Result<(), String> {
        if self.columns < 2 || self.rows < 2 {
            return Err("global raster must have at least two rows and columns".into());
        }
        if self.longitude_period_columns == 0 || self.longitude_period_columns > self.columns {
            return Err("global raster longitude period must be within its column count".into());
        }
        if !self.longitude_step_deg.is_finite() || self.longitude_step_deg <= 0.0 {
            return Err("global raster longitude step must be finite and positive".into());
        }
        if !self.latitude_step_deg.is_finite() || self.latitude_step_deg <= 0.0 {
            return Err("global raster latitude step must be finite and positive".into());
        }
        if !self.longitude_origin_deg.is_finite() || !self.latitude_origin_deg.is_finite() {
            return Err("global raster origins must be finite".into());
        }
        if !self.height_scale_m.is_finite() || self.height_scale_m <= 0.0 {
            return Err("global raster height scale must be finite and positive".into());
        }
        Ok(())
    }

    /// Bilinearly sample signed metres, wrapping longitude periodically and
    /// clamping latitude at the poles. The caller must have validated the
    /// layout and the raster byte length.
    pub fn sample_m(self, raw: &[u8], latitude_deg: f64, longitude_deg: f64) -> f64 {
        let latitude = latitude_deg.clamp(-90.0, 90.0);
        let x = ((longitude_deg - self.longitude_origin_deg) / self.longitude_step_deg)
            .rem_euclid(self.longitude_period_columns as f64);
        let y = ((self.latitude_origin_deg - latitude) / self.latitude_step_deg)
            .clamp(0.0, (self.rows - 1) as f64);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = (x0 + 1) % self.longitude_period_columns;
        let y1 = (y0 + 1).min(self.rows - 1);
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        let height = |column: usize, row: usize| self.height_m(raw, column, row);
        let north = height(x0, y0) + (height(x1, y0) - height(x0, y0)) * tx;
        let south = height(x0, y1) + (height(x1, y1) - height(x0, y1)) * tx;
        north + (south - north) * ty
    }

    fn height_m(self, raw: &[u8], column: usize, row: usize) -> f64 {
        let offset = (row * self.columns + column) * GLOBAL_RASTER_HEIGHT_BYTES;
        f64::from(i16::from_le_bytes([raw[offset], raw[offset + 1]])) * self.height_scale_m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raster(rows: &[[i16; 4]]) -> Vec<u8> {
        rows.iter()
            .flat_map(|row| row.iter())
            .flat_map(|height| height.to_le_bytes())
            .collect()
    }

    fn pixel_spec() -> GlobalRasterSpec {
        GlobalRasterSpec {
            columns: 4,
            rows: 2,
            longitude_origin_deg: -135.0,
            longitude_step_deg: 90.0,
            longitude_period_columns: 4,
            latitude_origin_deg: 45.0,
            latitude_step_deg: 90.0,
            height_scale_m: 1.0,
        }
    }

    #[test]
    fn samples_sample_centers_exactly() {
        let raw = raster(&[[1, 2, 3, 4], [10, 20, 30, 40]]);
        let spec = pixel_spec();
        assert_eq!(spec.sample_m(&raw, 45.0, -135.0), 1.0);
        assert_eq!(spec.sample_m(&raw, 45.0, 135.0), 4.0);
        assert_eq!(spec.sample_m(&raw, -45.0, 45.0), 30.0);
    }

    #[test]
    fn interpolates_and_wraps_longitude() {
        let raw = raster(&[[1, 2, 3, 4], [1, 2, 3, 4]]);
        let spec = pixel_spec();
        assert_eq!(spec.sample_m(&raw, 45.0, 0.0), 2.5);
        assert_eq!(spec.sample_m(&raw, 45.0, 180.0), 2.5);
        assert_eq!(spec.sample_m(&raw, 45.0, -180.0), 2.5);
    }

    #[test]
    fn clamps_latitude_at_poles() {
        let raw = raster(&[[1, 1, 1, 1], [9, 9, 9, 9]]);
        let spec = pixel_spec();
        assert_eq!(spec.sample_m(&raw, 90.0, 0.0), 1.0);
        assert_eq!(spec.sample_m(&raw, -90.0, 0.0), 9.0);
    }

    #[test]
    fn applies_a_signed_sample_scale() {
        let raw = raster(&[[1, 2, 3, 4], [10, 20, 30, 40]]);
        let mut spec = pixel_spec();
        spec.height_scale_m = 0.01;
        assert_eq!(spec.sample_m(&raw, 45.0, -135.0), 0.01);
        assert_eq!(spec.sample_m(&raw, -45.0, 45.0), 0.3);
    }

    #[test]
    fn rejects_invalid_layouts() {
        let mut spec = pixel_spec();
        spec.rows = 1;
        assert!(spec.validate().is_err());
        let mut spec = pixel_spec();
        spec.longitude_period_columns = 5;
        assert!(spec.validate().is_err());
        let mut spec = pixel_spec();
        spec.latitude_step_deg = 0.0;
        assert!(spec.validate().is_err());
    }
}
