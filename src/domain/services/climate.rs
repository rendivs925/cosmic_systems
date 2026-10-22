//! Versioned offline monthly climate packages for presentation-only atmosphere
//! and cloud parameters.
//!
//! A `.csclim` package stores twelve monthly climatology fields on a regular
//! terrain-radial latitude/longitude grid:
//!
//! 1. 2 m air temperature, degrees Celsius
//! 2. surface pressure, hPa
//! 3. 2 m relative humidity, percent
//! 4. 2 m eastward wind, m/s
//! 5. 2 m northward wind, m/s
//! 6. cloud amount, percent
//!
//! The package is a measured, global fallback for the sky and volumetric cloud
//! presentation. It is deliberately not an authoritative physics atmosphere:
//! the existing [`crate::domain::services::atmosphere::AtmosphereSource`] remains
//! the single authority for density, pressure, and speed of sound consumed by
//! flight dynamics. Adding measured profiles to that authority would require a
//! separate physics-baseline review.
//!
//! Values are quantized per channel (see [`quantize`]) and validated by a
//! payload SHA-256. Sampling is bilinear in space and cyclic-linear in month.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

const MAGIC: [u8; 8] = *b"CSCLIM\0\0";
const VERSION: u32 = 1;
const HEADER_BYTES: usize = 96;
/// Stored channels per cell: temperature, pressure, humidity, wind east, wind
/// north, cloud amount.
const CHANNELS: usize = 6;
/// Bytes per quantized cell.
const CELL_BYTES: usize = CHANNELS * 2;

/// Default native directory holding `.csclim` packages.
pub const DEFAULT_CLIMATE_DIR: &str = "assets/large_files/terrain/climate_v1";

/// Failures when validating or decoding a climate package.
#[derive(Debug)]
pub enum ClimateError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for ClimateError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

/// One measured monthly climatology sample. Units are SI except where the
/// source convention is clearer (hPa is converted to Pa, percent to fractions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClimateSample {
    pub temperature_k: f64,
    pub pressure_pa: f64,
    /// Relative humidity in `[0, 1]`.
    pub relative_humidity: f64,
    pub wind_east_mps: f64,
    pub wind_north_mps: f64,
    /// Total cloud amount in `[0, 1]`.
    pub cloud_fraction: f64,
}

impl ClimateSample {
    pub fn wind_speed_mps(&self) -> f64 {
        self.wind_east_mps.hypot(self.wind_north_mps)
    }

    /// Unit direction of the measured horizontal wind in the body-fixed x/z
    /// plane used by the cloud advection uniform. Terrain axes are x/lon0,
    /// y/north, z/east, so local east and north are projected into that plane.
    /// Calm wind returns an eastward fallback direction.
    pub fn wind_direction_xz(&self, latitude_deg: f64, longitude_deg: f64) -> (f32, f32) {
        let latitude = latitude_deg.to_radians();
        let longitude = longitude_deg.to_radians();
        let (sin_lat, _cos_lat) = latitude.sin_cos();
        let (sin_lon, cos_lon) = longitude.sin_cos();
        let east_x = -sin_lon;
        let east_z = cos_lon;
        let north_x = -sin_lat * cos_lon;
        let north_z = -sin_lat * sin_lon;
        let x = self.wind_east_mps * east_x + self.wind_north_mps * north_x;
        let z = self.wind_east_mps * east_z + self.wind_north_mps * north_z;
        let length = x.hypot(z);
        if length < 1.0e-6 {
            (1.0, 0.0)
        } else {
            ((x / length) as f32, (z / length) as f32)
        }
    }
}

/// Read-only climate query shared by a package and a composed set.
pub trait ClimateLookup {
    /// Sample at a terrain-radial coordinate. `month_fraction` is in months
    /// since January, wraps cyclically, and may be fractional.
    fn sample_climate(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        month_fraction: f64,
    ) -> Option<ClimateSample>;
}

/// Provenance required for an accepted climate package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClimateMetadata {
    pub body: String,
    pub coordinate_frame: String,
    pub horizontal_datum: String,
    pub vertical_datum: String,
    pub source: String,
    pub source_urls: Vec<String>,
    pub source_resolution_deg: f64,
    pub reference_period: String,
    pub nodata_policy: String,
    pub source_sha256: String,
    pub license: String,
    pub attribution: String,
    pub conversion_version: u32,
}

/// A regular terrain-radial grid carrying `months` quantized climate fields.
#[derive(Debug, Clone)]
pub struct ClimatePackage {
    width: u32,
    height: u32,
    months: u32,
    west_deg: f64,
    south_deg: f64,
    east_deg: f64,
    north_deg: f64,
    metadata: ClimateMetadata,
    cells: Vec<u8>,
}

impl ClimateLookup for ClimatePackage {
    fn sample_climate(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        month_fraction: f64,
    ) -> Option<ClimateSample> {
        ClimatePackage::sample_climate(self, latitude_deg, longitude_deg, month_fraction)
    }
}

impl ClimatePackage {
    #[expect(
        clippy::too_many_arguments,
        reason = "Package metadata, coverage, month count, and payload are independently validated."
    )]
    pub fn from_cells(
        width: u32,
        height: u32,
        months: u32,
        west_deg: f64,
        south_deg: f64,
        east_deg: f64,
        north_deg: f64,
        metadata: ClimateMetadata,
        cells: Vec<u8>,
    ) -> Result<Self, ClimateError> {
        let cells_per_month = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ClimateError::InvalidFormat("climate dimensions overflow".into()))?;
        let expected = (months as usize)
            .checked_mul(cells_per_month)
            .and_then(|count| count.checked_mul(CELL_BYTES))
            .ok_or_else(|| {
                ClimateError::InvalidFormat("climate payload length overflows".into())
            })?;
        if width < 2
            || height < 2
            || months == 0
            || months > 12
            || !(west_deg < east_deg && south_deg < north_deg)
            || !(west_deg >= -180.0 && east_deg <= 180.0 && south_deg >= -90.0 && north_deg <= 90.0)
            || !metadata.source_resolution_deg.is_finite()
            || metadata.source_resolution_deg <= 0.0
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
            || metadata.reference_period.is_empty()
            || metadata.nodata_policy.is_empty()
            || metadata.license.is_empty()
            || metadata.attribution.is_empty()
            || metadata.conversion_version == 0
            || metadata.source_sha256.len() != 64
            || !metadata
                .source_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || cells.len() != expected
        {
            return Err(ClimateError::InvalidFormat(
                "invalid climate package".into(),
            ));
        }
        Ok(Self {
            width,
            height,
            months,
            west_deg,
            south_deg,
            east_deg,
            north_deg,
            metadata,
            cells,
        })
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ClimateError> {
        if bytes.len() < HEADER_BYTES || bytes[..8] != MAGIC {
            return Err(ClimateError::InvalidFormat("invalid climate header".into()));
        }
        let u32_at = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at = |offset| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != VERSION {
            return Err(ClimateError::InvalidFormat(
                "unsupported climate package version".into(),
            ));
        }
        let metadata_bytes = u32_at(12) as usize;
        let width = u32_at(16);
        let height = u32_at(20);
        let months = u32_at(24);
        let west_deg = f64_at(28);
        let south_deg = f64_at(36);
        let east_deg = f64_at(44);
        let north_deg = f64_at(52);
        let metadata_end = HEADER_BYTES.checked_add(metadata_bytes).ok_or_else(|| {
            ClimateError::InvalidFormat("climate metadata length overflows".into())
        })?;
        let cells_per_month = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| ClimateError::InvalidFormat("climate dimensions overflow".into()))?;
        let payload_bytes = (months as usize)
            .checked_mul(cells_per_month)
            .and_then(|count| count.checked_mul(CELL_BYTES))
            .ok_or_else(|| {
                ClimateError::InvalidFormat("climate payload length overflows".into())
            })?;
        let expected_bytes = metadata_end.checked_add(payload_bytes).ok_or_else(|| {
            ClimateError::InvalidFormat("climate package length overflows".into())
        })?;
        if bytes.len() != expected_bytes {
            return Err(ClimateError::InvalidFormat(
                "invalid climate payload length".into(),
            ));
        }
        let metadata = ron::from_str(
            std::str::from_utf8(&bytes[HEADER_BYTES..metadata_end])
                .map_err(|_| ClimateError::InvalidFormat("climate metadata is not UTF-8".into()))?,
        )
        .map_err(|error| {
            ClimateError::InvalidFormat(format!("invalid climate metadata: {error}"))
        })?;
        let payload = &bytes[metadata_end..];
        if Sha256::digest(payload).as_slice() != &bytes[60..92] {
            return Err(ClimateError::InvalidFormat(
                "climate checksum mismatch".into(),
            ));
        }
        Self::from_cells(
            width,
            height,
            months,
            west_deg,
            south_deg,
            east_deg,
            north_deg,
            metadata,
            payload.to_vec(),
        )
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ClimateError> {
        Self::from_bytes(&fs::read(path)?)
    }

    pub fn write_path(&self, path: impl AsRef<Path>) -> Result<(), ClimateError> {
        fs::write(path, self.write_bytes())?;
        Ok(())
    }

    /// Encode without touching the filesystem (used by tests and the converter).
    pub fn write_bytes(&self) -> Vec<u8> {
        let metadata = ron::to_string(&self.metadata).expect("metadata must encode");
        let mut bytes = Vec::with_capacity(HEADER_BYTES + metadata.len() + self.cells.len());
        bytes.extend_from_slice(&MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.months.to_le_bytes());
        for value in [self.west_deg, self.south_deg, self.east_deg, self.north_deg] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&Sha256::digest(&self.cells));
        bytes.extend_from_slice(&[0u8; 4]);
        bytes.extend_from_slice(metadata.as_bytes());
        bytes.extend_from_slice(&self.cells);
        bytes
    }

    pub fn metadata(&self) -> &ClimateMetadata {
        &self.metadata
    }

    pub fn coverage_bounds_deg(&self) -> (f64, f64, f64, f64) {
        (self.west_deg, self.south_deg, self.east_deg, self.north_deg)
    }

    pub fn months(&self) -> u32 {
        self.months
    }

    /// Bilinear/cyclic sample at a terrain-radial coordinate.
    pub fn sample_climate(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        month_fraction: f64,
    ) -> Option<ClimateSample> {
        if !latitude_deg.is_finite()
            || !longitude_deg.is_finite()
            || !(self.south_deg..=self.north_deg).contains(&latitude_deg)
        {
            return None;
        }
        let wrapped = self.wrap_longitude(longitude_deg)?;
        let month = if month_fraction.is_finite() {
            month_fraction.rem_euclid(self.months as f64)
        } else {
            0.0
        };
        let month0 = month.floor() as usize;
        let month1 = (month0 + 1) % self.months as usize;
        let month_mix = (month - month0 as f64) as f32;
        let first = self.sample_month(month0, latitude_deg, wrapped)?;
        let second = self.sample_month(month1, latitude_deg, wrapped)?;
        Some(ClimateSample {
            temperature_k: lerp(first.temperature_k, second.temperature_k, month_mix),
            pressure_pa: lerp(first.pressure_pa, second.pressure_pa, month_mix),
            relative_humidity: lerp(first.relative_humidity, second.relative_humidity, month_mix),
            wind_east_mps: lerp(first.wind_east_mps, second.wind_east_mps, month_mix),
            wind_north_mps: lerp(first.wind_north_mps, second.wind_north_mps, month_mix),
            cloud_fraction: lerp(first.cloud_fraction, second.cloud_fraction, month_mix),
        })
    }

    fn wrap_longitude(&self, longitude_deg: f64) -> Option<f64> {
        let span = self.east_deg - self.west_deg;
        if span >= 360.0 {
            Some(self.west_deg + (longitude_deg - self.west_deg).rem_euclid(span))
        } else if (self.west_deg..=self.east_deg).contains(&longitude_deg) {
            Some(longitude_deg)
        } else {
            None
        }
    }

    fn sample_month(
        &self,
        month: usize,
        latitude_deg: f64,
        longitude_deg: f64,
    ) -> Option<ClimateSample> {
        let span = self.east_deg - self.west_deg;
        let global = span >= 360.0;
        let x = if global {
            let step = span / self.width as f64;
            ((longitude_deg - self.west_deg) / step).rem_euclid(self.width as f64)
        } else {
            ((longitude_deg - self.west_deg) / span * (self.width - 1) as f64)
                .clamp(0.0, (self.width - 1) as f64)
        };
        let y = ((self.north_deg - latitude_deg) / (self.north_deg - self.south_deg)
            * (self.height - 1) as f64)
            .clamp(0.0, (self.height - 1) as f64);
        let x0 = x.floor() as usize;
        let y0 = y.floor() as usize;
        let x1 = if global {
            (x0 + 1) % self.width as usize
        } else {
            (x0 + 1).min(self.width as usize - 1)
        };
        let y1 = (y0 + 1).min(self.height as usize - 1);
        let tx = (x - x0 as f64) as f32;
        let ty = (y - y0 as f64) as f32;
        let width = self.width as usize;
        let cell = |x: usize, y: usize| {
            let index = (month * self.height as usize * width + y * width + x) * CELL_BYTES;
            decode_cell(&self.cells[index..index + CELL_BYTES])
        };
        let northwest = cell(x0, y0);
        let northeast = cell(x1, y0);
        let southwest = cell(x0, y1);
        let southeast = cell(x1, y1);
        Some(interpolate_bilinear(
            northwest, northeast, southwest, southeast, tx, ty,
        ))
    }
}

fn lerp(a: f64, b: f64, t: f32) -> f64 {
    a + (b - a) * f64::from(t)
}

fn interpolate_bilinear(
    northwest: ClimateSample,
    northeast: ClimateSample,
    southwest: ClimateSample,
    southeast: ClimateSample,
    tx: f32,
    ty: f32,
) -> ClimateSample {
    let blend = |a: f64, b: f64, c: f64, d: f64| {
        let top = a + (b - a) * f64::from(tx);
        let bottom = c + (d - c) * f64::from(tx);
        top + (bottom - top) * f64::from(ty)
    };
    ClimateSample {
        temperature_k: blend(
            northwest.temperature_k,
            northeast.temperature_k,
            southwest.temperature_k,
            southeast.temperature_k,
        ),
        pressure_pa: blend(
            northwest.pressure_pa,
            northeast.pressure_pa,
            southwest.pressure_pa,
            southeast.pressure_pa,
        ),
        relative_humidity: blend(
            northwest.relative_humidity,
            northeast.relative_humidity,
            southwest.relative_humidity,
            southeast.relative_humidity,
        ),
        wind_east_mps: blend(
            northwest.wind_east_mps,
            northeast.wind_east_mps,
            southwest.wind_east_mps,
            southeast.wind_east_mps,
        ),
        wind_north_mps: blend(
            northwest.wind_north_mps,
            northeast.wind_north_mps,
            southwest.wind_north_mps,
            southeast.wind_north_mps,
        ),
        cloud_fraction: blend(
            northwest.cloud_fraction,
            northeast.cloud_fraction,
            southwest.cloud_fraction,
            southeast.cloud_fraction,
        ),
    }
}

/// Quantize a climate sample into the package's fixed-point representation.
/// The `i16`/`u16` layout keeps a global twelve-month package near 30 MB while
/// preserving source precision (0.01 degC, 0.05 hPa, 0.01 %, 0.01 m/s).
pub fn quantize(cells: &mut Vec<u8>, sample: ClimateSample) {
    let temperature = (sample.temperature_k - 273.15) * 100.0;
    let pressure = (sample.pressure_pa / 100.0 - 300.0) * 20.0;
    let humidity = sample.relative_humidity * 10_000.0;
    let wind_east = sample.wind_east_mps * 100.0;
    let wind_north = sample.wind_north_mps * 100.0;
    let cloud = sample.cloud_fraction * 10_000.0;
    cells.extend_from_slice(&clamp_i16(temperature).to_le_bytes());
    cells.extend_from_slice(&clamp_u16(pressure).to_le_bytes());
    cells.extend_from_slice(&clamp_u16(humidity).to_le_bytes());
    cells.extend_from_slice(&clamp_i16(wind_east).to_le_bytes());
    cells.extend_from_slice(&clamp_i16(wind_north).to_le_bytes());
    cells.extend_from_slice(&clamp_u16(cloud).to_le_bytes());
}

fn clamp_i16(value: f64) -> i16 {
    if !value.is_finite() {
        return 0;
    }
    value
        .round()
        .clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
}

fn clamp_u16(value: f64) -> u16 {
    if !value.is_finite() {
        return 0;
    }
    value.round().clamp(0.0, f64::from(u16::MAX)) as u16
}

fn decode_cell(bytes: &[u8]) -> ClimateSample {
    let i16_at = |offset| i16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let u16_at = |offset| u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
    let temperature_c = f64::from(i16_at(0)) / 100.0;
    let pressure_hpa = f64::from(u16_at(2)) / 20.0 + 300.0;
    ClimateSample {
        temperature_k: temperature_c + 273.15,
        pressure_pa: pressure_hpa * 100.0,
        relative_humidity: f64::from(u16_at(4)) / 10_000.0,
        wind_east_mps: f64::from(i16_at(6)) / 100.0,
        wind_north_mps: f64::from(i16_at(8)) / 100.0,
        cloud_fraction: f64::from(u16_at(10)) / 10_000.0,
    }
}

/// Ordered set of climate packages. Later packages win where coverage
/// overlaps, so a future regional refinement can override the global fallback
/// without changing the runtime query.
#[derive(Debug, Clone, Default)]
pub struct ClimateSet {
    packages: Vec<ClimatePackage>,
}

impl ClimateSet {
    pub fn new(packages: Vec<ClimatePackage>) -> Self {
        Self { packages }
    }

    pub fn from_paths(
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<Self, ClimateError> {
        let packages = paths
            .into_iter()
            .map(|path| ClimatePackage::from_path(path))
            .collect::<Result<Vec<_>, _>>()?;
        if packages.is_empty() {
            return Err(ClimateError::InvalidFormat(
                "climate set requires at least one package".into(),
            ));
        }
        Ok(Self::new(packages))
    }

    pub fn packages(&self) -> &[ClimatePackage] {
        &self.packages
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }
}

impl ClimateLookup for ClimateSet {
    fn sample_climate(
        &self,
        latitude_deg: f64,
        longitude_deg: f64,
        month_fraction: f64,
    ) -> Option<ClimateSample> {
        self.packages
            .iter()
            .rev()
            .find_map(|package| package.sample_climate(latitude_deg, longitude_deg, month_fraction))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> ClimateMetadata {
        ClimateMetadata {
            body: "Earth".into(),
            coordinate_frame: "terrain-radial-degrees".into(),
            horizontal_datum:
                "WGS 84 / EPSG:4326 source; planetocentric terrain-radial output grid".into(),
            vertical_datum: "not applicable (surface climatology)".into(),
            source: "NASA POWER MERRA-2 and CERES SYN1deg monthly climatology".into(),
            source_urls: vec!["https://nasa-power.s3.us-west-2.amazonaws.com/merra2/".into()],
            source_resolution_deg: 0.5,
            reference_period: "2001-01-01..2020-12-31".into(),
            nodata_policy: "nodata samples are replaced with the annual mean".into(),
            source_sha256: "0".repeat(64),
            license: "CC BY 4.0".into(),
            attribution: "NASA POWER / MERRA-2 / CERES SYN1deg".into(),
            conversion_version: 1,
        }
    }

    fn sample(temperature_c: f64, pressure_hpa: f64, cloud: f64) -> ClimateSample {
        ClimateSample {
            temperature_k: temperature_c + 273.15,
            pressure_pa: pressure_hpa * 100.0,
            relative_humidity: 0.5,
            wind_east_mps: 2.0,
            wind_north_mps: -1.0,
            cloud_fraction: cloud,
        }
    }

    fn package() -> ClimatePackage {
        let mut cells = Vec::new();
        for value in [
            sample(0.0, 1000.0, 0.1),
            sample(10.0, 1010.0, 0.2),
            sample(20.0, 1020.0, 0.3),
            sample(30.0, 1030.0, 0.4),
        ] {
            quantize(&mut cells, value);
        }
        ClimatePackage::from_cells(2, 2, 1, 0.0, 0.0, 1.0, 1.0, metadata(), cells)
            .expect("valid package")
    }

    #[test]
    fn roundtrip_is_deterministic_and_rejects_corruption() {
        let package = package();
        let bytes = package.write_bytes();
        assert_eq!(bytes, package.write_bytes());
        let decoded = ClimatePackage::from_bytes(&bytes).expect("roundtrip");
        let value = decoded.sample_climate(1.0, 0.0, 0.0).expect("covered");
        assert!((value.temperature_k - 273.15).abs() < 5e-3);
        assert!((value.pressure_pa - 100_000.0).abs() < 10.0);

        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(ClimatePackage::from_bytes(&corrupted).is_err());

        let mut wrong_version = bytes;
        wrong_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(ClimatePackage::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn sampling_is_bilinear_and_months_interpolate_cyclically() {
        let package = package();
        let northwest = package.sample_climate(1.0, 0.0, 0.0).expect("covered");
        assert!((northwest.temperature_k - 273.15).abs() < 5e-3);
        let center = package.sample_climate(0.5, 0.5, 0.0).expect("covered");
        assert!((center.temperature_k - 288.15).abs() < 0.01);
        assert!((center.cloud_fraction - 0.25).abs() < 1e-3);
        assert_eq!(package.sample_climate(2.0, 0.5, 0.0), None);
    }

    #[test]
    fn global_longitude_wraps_without_a_seam() {
        let mut cells = Vec::new();
        for longitude in [0.0f64, 90.0, 180.0, 270.0] {
            for _ in 0..2 {
                quantize(
                    &mut cells,
                    ClimateSample {
                        temperature_k: 273.15 + longitude / 100.0,
                        pressure_pa: 101_325.0,
                        relative_humidity: 0.5,
                        wind_east_mps: 0.0,
                        wind_north_mps: 0.0,
                        cloud_fraction: 0.5,
                    },
                );
            }
        }
        let package =
            ClimatePackage::from_cells(4, 2, 1, -180.0, -90.0, 180.0, 90.0, metadata(), cells)
                .expect("valid global package");
        let at_west = package.sample_climate(0.0, -180.0, 0.0).expect("covered");
        let at_east = package.sample_climate(0.0, 180.0, 0.0).expect("covered");
        assert!((at_west.temperature_k - at_east.temperature_k).abs() < 5e-3);
        let wrapped = package.sample_climate(0.0, 181.0, 0.0).expect("covered");
        assert!((wrapped.temperature_k - at_west.temperature_k).abs() < 5e-3);
    }

    #[test]
    fn later_packages_override_overlapping_coverage() {
        let mut coarse_cells = Vec::new();
        let mut regional_cells = Vec::new();
        for _ in 0..4 {
            quantize(&mut coarse_cells, sample(0.0, 1000.0, 0.1));
            quantize(&mut regional_cells, sample(25.0, 900.0, 0.8));
        }
        let coarse =
            ClimatePackage::from_cells(2, 2, 1, 0.0, 0.0, 4.0, 4.0, metadata(), coarse_cells)
                .expect("valid package");
        let regional =
            ClimatePackage::from_cells(2, 2, 1, 1.0, 1.0, 3.0, 3.0, metadata(), regional_cells)
                .expect("valid package");
        let set = ClimateSet::new(vec![coarse, regional]);
        assert_eq!(set.package_count(), 2);
        let refined = set.sample_climate(2.0, 2.0, 0.0).expect("covered");
        assert!((refined.temperature_k - 298.15).abs() < 0.01);
        let fallback = set.sample_climate(4.0, 0.0, 0.0).expect("covered");
        assert!((fallback.temperature_k - 273.15).abs() < 0.01);
        assert_eq!(set.sample_climate(8.0, 8.0, 0.0), None);
    }

    #[test]
    fn wind_direction_projects_into_the_body_fixed_xz_plane() {
        let eastward = ClimateSample {
            temperature_k: 288.15,
            pressure_pa: 101_325.0,
            relative_humidity: 0.5,
            wind_east_mps: 10.0,
            wind_north_mps: 0.0,
            cloud_fraction: 0.5,
        };
        let (x, z) = eastward.wind_direction_xz(0.0, 0.0);
        assert!((x - 0.0).abs() < 1e-6);
        assert!((z - 1.0).abs() < 1e-6);
        let calm = ClimateSample {
            wind_east_mps: 0.0,
            wind_north_mps: 0.0,
            ..eastward
        };
        assert_eq!(calm.wind_direction_xz(45.0, 12.0), (1.0, 0.0));
    }
}
