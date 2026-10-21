//! Deterministic terrain realism metrics for scientific parity checks.
//!
//! This is a measurement-only domain module: it samples the authoritative
//! [`TerrainSource`] on a geographic grid, derives global invariants
//! (hypsometry, slope distribution, drainage, coastline, roughness), and
//! serializes the result as a baseline. It never feeds the simulation.
//!
//! Grid convention: samples sit at cell centers, rows run north to south, and
//! area weights use the exact spherical band area for each row. Metrics that
//! depend on grid resolution are only comparable against a baseline measured at
//! the same resolution (see `docs/plans/terrain-realism-definition.md`).

mod hydrology;

use crate::domain::services::terrain_source::TerrainSource;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

pub use hydrology::HydrologyStats;

/// Baseline location used by the CLI and regression checks.
pub const DEFAULT_BASELINE_PATH: &str = "tests/baselines/terrain-metrics-earth-etopo2022.ron";

/// Measurement grid and channel-network settings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainMetricsConfig {
    /// Number of longitude samples.
    pub width: u32,
    /// Number of latitude samples.
    pub height: u32,
    /// D8 flow accumulation (in cells) at which a cell joins the stream
    /// network.
    pub channel_threshold_cells: f64,
    /// Largest variogram lag in grid cells.
    pub variogram_max_lag: u32,
}

impl TerrainMetricsConfig {
    /// Rough, interactive pass (0.5 degree grid).
    pub fn quick() -> Self {
        Self {
            width: 720,
            height: 360,
            channel_threshold_cells: 32.0,
            variogram_max_lag: 32,
        }
    }

    /// Full pass (0.25 degree grid, the global relief comparison grid).
    pub fn full() -> Self {
        Self {
            width: 1_440,
            height: 720,
            channel_threshold_cells: 32.0,
            variogram_max_lag: 64,
        }
    }

    fn validate(&self) -> Result<(), TerrainMetricsError> {
        if self.width < 8 || self.height < 8 {
            return Err(TerrainMetricsError::InvalidConfig(
                "measurement grid must be at least 8x8 samples",
            ));
        }
        if !(self.channel_threshold_cells.is_finite() && self.channel_threshold_cells >= 1.0) {
            return Err(TerrainMetricsError::InvalidConfig(
                "channel_threshold_cells must be finite and at least 1",
            ));
        }
        if self.variogram_max_lag < 2 {
            return Err(TerrainMetricsError::InvalidConfig(
                "variogram_max_lag must be at least 2 cells",
            ));
        }
        Ok(())
    }
}

/// Elevation distribution over the whole body and its land surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypsometry {
    pub min_m: f64,
    pub max_m: f64,
    pub land_fraction: f64,
    pub ocean_fraction: f64,
    pub land_mean_m: f64,
    pub land_p50_m: f64,
    pub land_p95_m: f64,
}

/// Area-weighted land slope statistics in degrees.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlopeStats {
    pub land_p50_deg: f64,
    pub land_p95_deg: f64,
    pub max_deg: f64,
    pub fraction_above_35_deg: f64,
}

/// Coastline geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoastlineStats {
    pub length_km: f64,
    /// Box-counting dimension; resolution dependent.
    pub fractal_dimension: Option<f64>,
}

/// Along-profile roughness estimate from the elevation variogram.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoughnessStats {
    /// Slope of `log(gamma)` against `log(lag)`.
    pub variogram_slope: Option<f64>,
    /// Estimated 2D power-spectrum exponent, `2 + variogram_slope`.
    pub psd_beta: Option<f64>,
}

/// Complete deterministic parity report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainMetricsReport {
    pub body: String,
    pub grid_width: u32,
    pub grid_height: u32,
    pub channel_threshold_cells: f64,
    pub sample_count: u64,
    pub hypsometry: Hypsometry,
    pub slope: SlopeStats,
    pub hydrology: HydrologyStats,
    pub coastline: CoastlineStats,
    pub roughness: RoughnessStats,
}

/// Measurement failure.
#[derive(Debug)]
pub enum TerrainMetricsError {
    InvalidConfig(&'static str),
    NonFiniteSample {
        latitude_deg: f64,
        longitude_deg: f64,
    },
    Io(std::io::Error),
    Ron(ron::Error),
    BaselineMismatch(String),
}

impl fmt::Display for TerrainMetricsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid metrics configuration: {message}"),
            Self::NonFiniteSample {
                latitude_deg,
                longitude_deg,
            } => write!(
                f,
                "terrain source returned a non-finite height at {latitude_deg}, {longitude_deg}"
            ),
            Self::Io(error) => write!(f, "metrics I/O error: {error}"),
            Self::Ron(error) => write!(f, "metrics serialization error: {error}"),
            Self::BaselineMismatch(message) => write!(f, "baseline mismatch: {message}"),
        }
    }
}

impl std::error::Error for TerrainMetricsError {}

impl From<std::io::Error> for TerrainMetricsError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ron::Error> for TerrainMetricsError {
    fn from(error: ron::Error) -> Self {
        Self::Ron(error)
    }
}

/// Sample the source and compute every metric. Pure and deterministic for a
/// fixed source, grid, and configuration.
pub fn measure_terrain_metrics(
    source: &dyn TerrainSource,
    config: &TerrainMetricsConfig,
    body: &str,
    body_radius_m: f64,
) -> Result<TerrainMetricsReport, TerrainMetricsError> {
    config.validate()?;
    if !(body_radius_m.is_finite() && body_radius_m > 0.0) {
        return Err(TerrainMetricsError::InvalidConfig(
            "body radius must be finite and positive",
        ));
    }

    let grid = SampleGrid::sample(source, config, body_radius_m)?;
    let hypsometry = grid.hypsometry();
    let slope = grid.slope_stats(body_radius_m);
    let coastline = grid.coastline_stats(body_radius_m);
    let roughness = grid.roughness(config.variogram_max_lag);
    let hydrology = hydrology::measure(hydrology::HydrologyInputs {
        heights: &grid.heights,
        width: grid.width,
        height: grid.height,
        land: &grid.land,
        latitude_deg: &grid.latitude_deg,
        cell_area_km2: &grid.cell_area_km2,
        channel_threshold_cells: config.channel_threshold_cells,
        body_radius_m,
    });

    Ok(TerrainMetricsReport {
        body: body.to_owned(),
        grid_width: config.width,
        grid_height: config.height,
        channel_threshold_cells: config.channel_threshold_cells,
        sample_count: grid.heights.len() as u64,
        hypsometry,
        slope,
        hydrology,
        coastline,
        roughness,
    })
}

/// Serialize a report as stable, human-reviewable RON.
pub fn save_report(
    path: impl AsRef<Path>,
    report: &TerrainMetricsReport,
) -> Result<(), TerrainMetricsError> {
    if let Some(parent) = path.as_ref().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text = ron::ser::to_string_pretty(report, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, text)?;
    Ok(())
}

/// Load a previously written baseline.
pub fn load_report(path: impl AsRef<Path>) -> Result<TerrainMetricsReport, TerrainMetricsError> {
    let text = std::fs::read_to_string(path)?;
    ron::from_str(&text).map_err(|error| TerrainMetricsError::Ron(error.code))
}

/// Compare a freshly measured report against a baseline. Counts and grid
/// identity must match exactly; floating metrics use a tight tolerance because
/// a deterministic run is expected to reproduce the baseline bit-for-bit.
pub fn compare_reports(
    baseline: &TerrainMetricsReport,
    actual: &TerrainMetricsReport,
) -> Result<(), TerrainMetricsError> {
    const REL_TOL: f64 = 1e-6;
    const ABS_TOL: f64 = 1e-9;

    if baseline.body != actual.body
        || baseline.grid_width != actual.grid_width
        || baseline.grid_height != actual.grid_height
        || baseline.sample_count != actual.sample_count
        || baseline.channel_threshold_cells.to_bits() != actual.channel_threshold_cells.to_bits()
    {
        return Err(TerrainMetricsError::BaselineMismatch(
            "report identity/grid differs".into(),
        ));
    }

    let mut first_mismatch = None;
    let mut check = |name: &str, expected: f64, value: f64| {
        let tolerance = ABS_TOL.max(REL_TOL * expected.abs());
        if (expected - value).abs() > tolerance && first_mismatch.is_none() {
            first_mismatch = Some(format!("{name}: expected {expected}, measured {value}"));
        }
    };
    let h = (&actual.hypsometry, &baseline.hypsometry);
    check("hypsometry.min_m", h.1.min_m, h.0.min_m);
    check("hypsometry.max_m", h.1.max_m, h.0.max_m);
    check(
        "hypsometry.land_fraction",
        h.1.land_fraction,
        h.0.land_fraction,
    );
    check(
        "hypsometry.ocean_fraction",
        h.1.ocean_fraction,
        h.0.ocean_fraction,
    );
    check("hypsometry.land_mean_m", h.1.land_mean_m, h.0.land_mean_m);
    check("hypsometry.land_p50_m", h.1.land_p50_m, h.0.land_p50_m);
    check("hypsometry.land_p95_m", h.1.land_p95_m, h.0.land_p95_m);
    let s = (&actual.slope, &baseline.slope);
    check("slope.land_p50_deg", s.1.land_p50_deg, s.0.land_p50_deg);
    check("slope.land_p95_deg", s.1.land_p95_deg, s.0.land_p95_deg);
    check("slope.max_deg", s.1.max_deg, s.0.max_deg);
    check(
        "slope.fraction_above_35_deg",
        s.1.fraction_above_35_deg,
        s.0.fraction_above_35_deg,
    );
    let c = (&actual.coastline, &baseline.coastline);
    check("coastline.length_km", c.1.length_km, c.0.length_km);
    check_optional(
        &mut check,
        "coastline.fractal_dimension",
        c.1.fractal_dimension,
        c.0.fractal_dimension,
    );
    let r = (&actual.roughness, &baseline.roughness);
    check_optional(
        &mut check,
        "roughness.variogram_slope",
        r.1.variogram_slope,
        r.0.variogram_slope,
    );
    check_optional(&mut check, "roughness.psd_beta", r.1.psd_beta, r.0.psd_beta);
    hydrology::compare(&actual.hydrology, &baseline.hydrology, &mut check);

    match first_mismatch {
        Some(message) => Err(TerrainMetricsError::BaselineMismatch(message)),
        None => Ok(()),
    }
}

fn check_optional(
    check: &mut impl FnMut(&str, f64, f64),
    name: &str,
    expected: Option<f64>,
    value: Option<f64>,
) {
    match (expected, value) {
        (Some(expected), Some(value)) => check(name, expected, value),
        (None, None) => {}
        (Some(expected), None) => check(name, expected, f64::NAN),
        (None, Some(value)) => check(name, f64::NAN, value),
    }
}

/// Equirectangular sample grid over a whole body.
struct SampleGrid {
    width: usize,
    height: usize,
    heights: Vec<f32>,
    land: Vec<bool>,
    latitude_deg: Vec<f64>,
    cell_area_km2: Vec<f64>,
}

impl SampleGrid {
    fn sample(
        source: &dyn TerrainSource,
        config: &TerrainMetricsConfig,
        body_radius_m: f64,
    ) -> Result<Self, TerrainMetricsError> {
        let width = config.width as usize;
        let height = config.height as usize;
        let latitude_step_deg = 180.0 / height as f64;
        let longitude_step_deg = 360.0 / width as f64;
        let longitude_step_rad = longitude_step_deg.to_radians();

        let mut heights = Vec::with_capacity(width * height);
        let mut land = Vec::with_capacity(width * height);
        let mut latitude_deg = Vec::with_capacity(height);
        let mut cell_area_km2 = Vec::with_capacity(height);

        for y in 0..height {
            let latitude = 90.0 - (y as f64 + 0.5) * latitude_step_deg;
            latitude_deg.push(latitude);
            // Exact spherical band area for this row.
            let north = (latitude + 0.5 * latitude_step_deg).to_radians();
            let south = (latitude - 0.5 * latitude_step_deg).to_radians();
            let area_m2 = body_radius_m
                * body_radius_m
                * longitude_step_rad
                * (north.sin() - south.sin()).abs();
            cell_area_km2.push(area_m2 / 1e6);

            for x in 0..width {
                let longitude = -180.0 + (x as f64 + 0.5) * longitude_step_deg;
                let height_m = source.height_m(latitude, longitude);
                if !height_m.is_finite() {
                    return Err(TerrainMetricsError::NonFiniteSample {
                        latitude_deg: latitude,
                        longitude_deg: longitude,
                    });
                }
                heights.push(height_m as f32);
                land.push(height_m > source.sea_level_height_m(latitude, longitude));
            }
        }

        Ok(Self {
            width,
            height,
            heights,
            land,
            latitude_deg,
            cell_area_km2,
        })
    }

    fn hypsometry(&self) -> Hypsometry {
        let mut min_m = f64::INFINITY;
        let mut max_m = f64::NEG_INFINITY;
        let mut total_area = 0.0;
        let mut land_area = 0.0;
        let mut land_weighted_elevation = 0.0;
        let mut land_samples = Vec::with_capacity(self.heights.len());

        for y in 0..self.height {
            let area = self.cell_area_km2[y];
            for x in 0..self.width {
                let index = y * self.width + x;
                let elevation = f64::from(self.heights[index]);
                min_m = min_m.min(elevation);
                max_m = max_m.max(elevation);
                total_area += area;
                if self.land[index] {
                    land_area += area;
                    land_weighted_elevation += elevation * area;
                    land_samples.push((elevation, area));
                }
            }
        }

        let land_p50 = weighted_percentile(&mut land_samples, 0.50);
        let land_p95 = weighted_percentile(&mut land_samples, 0.95);
        Hypsometry {
            min_m,
            max_m,
            land_fraction: land_area / total_area,
            ocean_fraction: 1.0 - land_area / total_area,
            land_mean_m: land_weighted_elevation / land_area,
            land_p50_m: land_p50,
            land_p95_m: land_p95,
        }
    }

    fn slope_stats(&self, body_radius_m: f64) -> SlopeStats {
        let latitude_step_rad = (180.0 / self.height as f64).to_radians();
        let longitude_step_rad = (360.0 / self.width as f64).to_radians();
        let north_south_m = (body_radius_m * latitude_step_rad).max(1.0);
        let mut samples = Vec::new();
        let mut total_weight = 0.0;
        let mut steep_weight = 0.0;
        let mut max_deg: f64 = 0.0;

        for y in 0..self.height {
            let latitude_rad = self.latitude_deg[y].to_radians();
            let east_west_m =
                (body_radius_m * latitude_rad.cos().abs() * longitude_step_rad).max(1.0);
            let y_north = y.saturating_sub(1);
            let y_south = (y + 1).min(self.height - 1);
            for x in 0..self.width {
                let index = y * self.width + x;
                if !self.land[index] {
                    continue;
                }
                let west = self.heights[y * self.width + ((x + self.width - 1) % self.width)];
                let east = self.heights[y * self.width + ((x + 1) % self.width)];
                let north = self.heights[y_north * self.width + x];
                let south = self.heights[y_south * self.width + x];
                let dz_dx = f64::from(east - west) / (2.0 * east_west_m);
                let dz_dy = f64::from(south - north) / (2.0 * north_south_m);
                let slope_deg = dz_dx.hypot(dz_dy).atan().to_degrees();
                let area = self.cell_area_km2[y];
                max_deg = max_deg.max(slope_deg);
                total_weight += area;
                if slope_deg > 35.0 {
                    steep_weight += area;
                }
                samples.push((slope_deg, area));
            }
        }

        SlopeStats {
            land_p50_deg: weighted_percentile(&mut samples, 0.50),
            land_p95_deg: weighted_percentile(&mut samples, 0.95),
            max_deg,
            fraction_above_35_deg: steep_weight / total_weight.max(f64::MIN_POSITIVE),
        }
    }

    fn coastline_stats(&self, body_radius_m: f64) -> CoastlineStats {
        let latitude_step_rad = (180.0 / self.height as f64).to_radians() * body_radius_m;
        let longitude_step_rad = (360.0 / self.width as f64).to_radians() * body_radius_m;
        let mut length_m = 0.0;
        let mut mask = vec![false; self.width * self.height];

        for y in 0..self.height {
            let latitude_rad = self.latitude_deg[y].to_radians();
            let east_west_m = (longitude_step_rad * latitude_rad.cos().abs()).max(1.0);
            for x in 0..self.width {
                let index = y * self.width + x;
                if !self.land[index] {
                    continue;
                }
                let west = self.land[y * self.width + ((x + self.width - 1) % self.width)];
                let east = self.land[y * self.width + ((x + 1) % self.width)];
                // Beyond the poles the surface is still land, not ocean.
                let north = y == 0 || self.land[(y - 1) * self.width + x];
                let south = y + 1 == self.height || self.land[(y + 1) * self.width + x];
                if !(west && east && north && south) {
                    mask[index] = true;
                }
                // Each land/ocean edge is counted once, from the land side.
                if !east {
                    length_m += latitude_step_rad;
                }
                if !west {
                    length_m += latitude_step_rad;
                }
                // The pole rows have no ocean beyond them.
                if y > 0 && !north {
                    length_m += east_west_m;
                }
                if y + 1 < self.height && !south {
                    length_m += east_west_m;
                }
            }
        }

        CoastlineStats {
            length_km: length_m / 1_000.0,
            fractal_dimension: box_counting_dimension(&mask, self.width, self.height),
        }
    }

    fn roughness(&self, max_lag: u32) -> RoughnessStats {
        let max_lag = (max_lag as usize).min(self.width / 3).min(self.height / 3);
        if max_lag < 2 {
            return RoughnessStats {
                variogram_slope: None,
                psd_beta: None,
            };
        }
        // Deterministic subsampling keeps the pass bounded on large grids.
        let target_pairs = 4_000usize;
        let stride = (((self.width * self.height) as f64 / target_pairs as f64)
            .sqrt()
            .ceil() as usize)
            .max(1);
        let mut points = Vec::new();
        let mut lag = 1usize;
        while lag <= max_lag {
            let mut sum = 0.0;
            let mut count = 0usize;
            let mut y = 0;
            while y < self.height {
                let mut x = 0;
                while x + lag < self.width {
                    let first = y * self.width + x;
                    let east = first + lag;
                    if self.land[first] && self.land[east] {
                        let difference =
                            f64::from(self.heights[first]) - f64::from(self.heights[east]);
                        sum += difference * difference;
                        count += 1;
                    }
                    x += stride;
                }
                y += stride;
            }
            let mut y = 0;
            while y + lag < self.height {
                let mut x = 0;
                while x < self.width {
                    let first = y * self.width + x;
                    let south = (y + lag) * self.width + x;
                    if self.land[first] && self.land[south] {
                        let difference =
                            f64::from(self.heights[first]) - f64::from(self.heights[south]);
                        sum += difference * difference;
                        count += 1;
                    }
                    x += stride;
                }
                y += stride;
            }
            if count >= 32 {
                let gamma = 0.5 * sum / count as f64;
                if gamma > 0.0 {
                    points.push(((lag as f64).ln(), gamma.ln()));
                }
            }
            lag = (lag as f64 * 1.5).ceil() as usize;
        }

        let variogram_slope = log_log_slope(&points);
        RoughnessStats {
            variogram_slope,
            psd_beta: variogram_slope.map(|slope| 2.0 + slope),
        }
    }
}

fn log_log_slope(points: &[(f64, f64)]) -> Option<f64> {
    if points.len() < 2 {
        return None;
    }
    let n = points.len() as f64;
    let mean_x = points.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = points.iter().map(|(_, y)| y).sum::<f64>() / n;
    let covariance: f64 = points
        .iter()
        .map(|(x, y)| (x - mean_x) * (y - mean_y))
        .sum();
    let variance: f64 = points.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    if variance <= f64::EPSILON {
        return None;
    }
    Some(covariance / variance)
}

/// Area-weighted percentile over `(value, weight)` pairs. The input is sorted
/// in place; weights and values must be finite and non-negative.
fn weighted_percentile(samples: &mut [(f64, f64)], percentile: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.sort_by(|a, b| a.0.total_cmp(&b.0));
    let total: f64 = samples.iter().map(|(_, weight)| weight).sum();
    if total <= 0.0 {
        return samples[0].0;
    }
    let target = percentile.clamp(0.0, 1.0) * total;
    let mut cumulative = 0.0;
    for (value, weight) in samples.iter() {
        cumulative += weight;
        if cumulative >= target {
            return *value;
        }
    }
    samples[samples.len() - 1].0
}

/// Box-counting dimension of a binary mask. Box sizes are powers of two; the
/// slope of `log(count)` against `log(1 / size)` is the dimension.
fn box_counting_dimension(mask: &[bool], width: usize, height: usize) -> Option<f64> {
    let mut points = Vec::new();
    let mut size = 1usize;
    while size <= width.min(height) / 4 {
        let boxes_w = width.div_ceil(size);
        let boxes_h = height.div_ceil(size);
        let mut occupied = vec![false; boxes_w * boxes_h];
        for y in 0..height {
            for x in 0..width {
                if mask[y * width + x] {
                    occupied[(y / size) * boxes_w + x / size] = true;
                }
            }
        }
        let count = occupied.iter().filter(|value| **value).count();
        if count > 0 {
            points.push(((1.0 / size as f64).ln(), (count as f64).ln()));
        }
        size *= 2;
    }
    log_log_slope(&points)
}

/// Great-circle distance in metres between two latitude/longitude samples.
pub(super) fn haversine_m(
    latitude_a_deg: f64,
    longitude_a_deg: f64,
    latitude_b_deg: f64,
    longitude_b_deg: f64,
    body_radius_m: f64,
) -> f64 {
    let lat_a = latitude_a_deg.to_radians();
    let lat_b = latitude_b_deg.to_radians();
    let delta_lat = lat_b - lat_a;
    let delta_lon = (longitude_b_deg - longitude_a_deg).to_radians();
    let a = (delta_lat * 0.5).sin().powi(2)
        + lat_a.cos() * lat_b.cos() * (delta_lon * 0.5).sin().powi(2);
    2.0 * body_radius_m * a.sqrt().clamp(0.0, 1.0).asin()
}

/// Longitude of the `x`-th sample center on a full-turn grid.
pub(super) fn sample_longitude_deg(x: usize, width: usize) -> f64 {
    -180.0 + (x as f64 + 0.5) * 360.0 / width as f64
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
