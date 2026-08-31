//! Authoritative terrain height source (AGENTS.md sections 20-21).
//!
//! `TerrainSource` is the single terrain-data boundary. Render meshes and
//! collision queries consume the same trait, so a procedural, heightmap, or
//! DEM source can be swapped without rewriting either consumer.
//!
//! Heights are in meters above the planet's mean radius (geocentric). The
//! source is planet-scoped: the planet is bound when the source is attached to
//! a celestial body, so the interface takes latitude/longitude only.
//!
//! Procedural generation is deterministic: it uses seeded value noise with no
//! runtime state, so identical inputs always produce identical output
//! independent of frame rate, spawn order, or camera movement.

use bevy::math::DVec3;
use std::fmt::Debug;
#[cfg(feature = "dem")]
use std::path::Path;
use std::sync::{Arc, OnceLock};

use crate::domain::services::erosion::{ErodedTerrainSource, ErosionConfig};

/// Feature-scale of the 3D value-noise field on the unit sphere: higher values
/// produce more, smaller features across the planet.
const NOISE_SCALE: f64 = 10.0;

/// Domain-warp strength: how far the low-frequency warp field displaces the
/// sample point before the fractals are evaluated. This is what makes ridgelines
/// meander and removes the ubiquitous "noise-grid" look (inexorable best
/// practice — cheap and always worth it).
const WARP_STRENGTH: f64 = 3.0;

/// Power redistribution exponent applied to the base rolling terrain: values
/// above 1 flatten plains while keeping peaks sharp (the classic "plains + peak"
/// shaping, per the procedural-terrain references).
const SHAPE_POWER: f64 = 1.35;

/// Separate warp/moisture seeds so the fields are statistically independent.
const SEED_WARP_X: u64 = 0xD1B5_A9B1_7E1F_2A3C;
const SEED_WARP_Y: u64 = 0x5B1E_3C4D_92AF_4B11;
const SEED_WARP_Z: u64 = 0x9E77_6E5D_C0A8_3B22;
const SEED_MOISTURE: u64 = 0x4C3A_2B19_08F7_E6D5;

/// A source of terrain surface heights in meters above the mean radius.
pub trait TerrainSource: Send + Sync + Debug {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64;

    /// Prepare expensive, deterministic data for a sample. This is invoked only
    /// by terrain worker tasks; fixed-step collision queries must use `height_m`
    /// without causing I/O or a terrain bake.
    fn prepare_sample(&self, _latitude_deg: f64, _longitude_deg: f64) {}

    /// Coarse, non-authoritative height for whole-body presentation such as the
    /// rocket overview map. Sources with expensive local detail should expose a
    /// cheap base value here; physics, collision, and terrain meshes must keep
    /// using [`Self::height_m`].
    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.height_m(latitude_deg, longitude_deg)
    }

    /// Normalized soil moisture in `[0, 1]` (drives vegetation/biome). Sources
    /// without a moisture model default to a neutral `0.5` so biomes still
    /// vary by elevation and latitude.
    fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.5
    }

    /// Normalized river-channel strength in `[0, 1]`. This is presentation
    /// metadata derived from the same authoritative terrain source; a default
    /// of zero keeps sources without hydrology dry.
    fn river_strength(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    /// Coarse, non-authoritative moisture for whole-body presentation.
    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.moisture(latitude_deg, longitude_deg)
    }

    /// Coarse, non-authoritative slope for whole-body presentation. It derives
    /// from overview heights so it cannot initialize local terrain caches.
    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let d = 0.02; // ~2 km probe for a stable, feature-scale gradient
        let hx = self.overview_height_m(latitude_deg, longitude_deg + d)
            - self.overview_height_m(latitude_deg, longitude_deg - d);
        let hy = self.overview_height_m(latitude_deg + d, longitude_deg)
            - self.overview_height_m(latitude_deg - d, longitude_deg);
        let lat_m = 111_320.0;
        let lon_m = (111_320.0 * latitude_deg.to_radians().cos()).abs().max(1.0);
        let gradient = (hx / (2.0 * d * lon_m)).hypot(hy / (2.0 * d * lat_m));
        gradient.atan().to_degrees()
    }

    /// Normalized latitude zone in `[0, 1]` (`0` = south pole, `1` = north
    /// pole), used to fade cold-biome coloring toward the poles. Default maps
    /// latitude linearly; planets cold/temperate callers may override.
    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        ((latitude_deg + 90.0) / 180.0).clamp(0.0, 1.0)
    }
}

/// Conservative elevation interval in meters above mean radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ElevationBounds {
    pub min_m: f64,
    pub max_m: f64,
}

impl ElevationBounds {
    pub fn new(min_m: f64, max_m: f64) -> Self {
        assert!(
            min_m.is_finite() && max_m.is_finite() && min_m <= max_m,
            "terrain elevation bounds must be finite and ordered"
        );
        Self { min_m, max_m }
    }

    /// Conservative bounds for the sum of two independent elevation layers.
    pub fn combine(self, other: Self) -> Self {
        Self::new(self.min_m + other.min_m, self.max_m + other.max_m)
    }

    pub fn range_m(self) -> f64 {
        self.max_m - self.min_m
    }
}

/// A terrain elevation layer with an explicitly declared conservative envelope.
/// The source must return a contribution, not a radius, in meters.
#[derive(Debug)]
pub struct TerrainElevationLayer {
    source: Arc<dyn TerrainSource>,
    bounds: ElevationBounds,
}

impl TerrainElevationLayer {
    pub fn new(source: Arc<dyn TerrainSource>, bounds: ElevationBounds) -> Self {
        Self { source, bounds }
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }
}

/// LOD-only fade metadata for bounded procedural detail. It is intentionally
/// separate from terrain sampling: camera movement must never change height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetailLodFade {
    /// Detail is absent at this LOD and every coarser LOD.
    pub first_faded_level: u32,
    /// Detail is fully represented at this LOD and every finer LOD.
    pub first_full_detail_level: u32,
}

impl DetailLodFade {
    pub fn new(first_faded_level: u32, first_full_detail_level: u32) -> Self {
        assert!(
            first_faded_level <= first_full_detail_level,
            "detail fade must progress from coarse to fine LOD"
        );
        Self {
            first_faded_level,
            first_full_detail_level,
        }
    }

    /// Smoothly increases detail representation from coarse to fine LOD.
    pub fn weight_for_level(self, level: u32) -> f64 {
        if level <= self.first_faded_level {
            return 0.0;
        }
        if level >= self.first_full_detail_level {
            return 1.0;
        }
        let span = (self.first_full_detail_level - self.first_faded_level) as f64;
        let t = (level - self.first_faded_level) as f64 / span;
        t * t * (3.0 - 2.0 * t)
    }
}

/// A bounded procedural contribution plus the LOD metadata used by mesh
/// generation. The fade is not applied by [`LayeredTerrainSource::height_m`].
#[derive(Debug)]
pub struct TerrainDetailLayer {
    source: Arc<dyn TerrainSource>,
    bounds: ElevationBounds,
    pub lod_fade: DetailLodFade,
}

impl TerrainDetailLayer {
    pub fn new(
        source: Arc<dyn TerrainSource>,
        bounds: ElevationBounds,
        lod_fade: DetailLodFade,
    ) -> Self {
        Self {
            source,
            bounds,
            lod_fade,
        }
    }

    pub fn bounds(&self) -> ElevationBounds {
        self.bounds
    }
}

/// The one authoritative composition of a planet's terrain elevation layers.
///
/// The base and macro layers provide global shape, an optional DEM layer adds
/// configured local data or its own deterministic fallback, and detail remains
/// in the physical source at every LOD. Rendering may use `lod_fade` to blend a
/// representation, but collision and height queries always sample this sum.
#[derive(Debug)]
pub struct LayeredTerrainSource {
    pub base: TerrainElevationLayer,
    pub macro_elevation: Option<TerrainElevationLayer>,
    pub dem_elevation: Option<TerrainElevationLayer>,
    pub procedural_detail: Option<TerrainDetailLayer>,
}

impl LayeredTerrainSource {
    pub fn new(
        base: TerrainElevationLayer,
        macro_elevation: Option<TerrainElevationLayer>,
        dem_elevation: Option<TerrainElevationLayer>,
        procedural_detail: Option<TerrainDetailLayer>,
    ) -> Self {
        Self {
            base,
            macro_elevation,
            dem_elevation,
            procedural_detail,
        }
    }

    /// Conservative deterministic bounds for every active elevation layer.
    pub fn elevation_bounds_m(&self) -> ElevationBounds {
        [
            Some(self.base.bounds()),
            self.macro_elevation
                .as_ref()
                .map(TerrainElevationLayer::bounds),
            self.dem_elevation
                .as_ref()
                .map(TerrainElevationLayer::bounds),
            self.procedural_detail
                .as_ref()
                .map(TerrainDetailLayer::bounds),
        ]
        .into_iter()
        .flatten()
        .fold(ElevationBounds::new(0.0, 0.0), ElevationBounds::combine)
    }

    fn primary_surface(&self) -> &dyn TerrainSource {
        self.dem_elevation
            .as_ref()
            .map(|layer| layer.source.as_ref())
            .or_else(|| {
                self.macro_elevation
                    .as_ref()
                    .map(|layer| layer.source.as_ref())
            })
            .unwrap_or(self.base.source.as_ref())
    }
}

impl TerrainSource for LayeredTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let mut height_m = self.base.source.height_m(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.dem_elevation {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.procedural_detail {
            height_m += layer.source.height_m(latitude_deg, longitude_deg);
        }
        height_m
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.base.source.prepare_sample(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            layer.source.prepare_sample(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.dem_elevation {
            layer.source.prepare_sample(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.procedural_detail {
            layer.source.prepare_sample(latitude_deg, longitude_deg);
        }
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let mut height_m = self
            .base
            .source
            .overview_height_m(latitude_deg, longitude_deg);
        if let Some(layer) = &self.macro_elevation {
            height_m += layer.source.overview_height_m(latitude_deg, longitude_deg);
        }
        if let Some(layer) = &self.dem_elevation {
            height_m += layer.source.overview_height_m(latitude_deg, longitude_deg);
        }
        height_m
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface().moisture(latitude_deg, longitude_deg)
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .river_strength(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.primary_surface()
            .overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.primary_surface().zone_lat(latitude_deg)
    }
}

/// Height from a deterministic 3D value-noise field. Evaluating the noise on
/// the unit sphere (via a direction vector) is naturally seamless — there is no
/// longitude seam to worry about.
#[derive(Debug, Clone, Copy)]
pub struct ValueNoise;

impl ValueNoise {
    fn cell3(&self, seed: u64, x: i64, y: i64, z: i64) -> f64 {
        let mut h = seed
            ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9)
            ^ (z as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 30;
        h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        h ^= h >> 27;
        h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
        h ^= h >> 31;
        (h & 0xFFFF_FFFF_FFFF) as f64 / 0x1_0000_0000_0000u64 as f64
    }

    fn smooth(t: f64) -> f64 {
        t * t * (3.0 - 2.0 * t)
    }

    fn value_noise3(&self, seed: u64, x: f64, y: f64, z: f64) -> f64 {
        let (x0, y0, z0) = (x.floor() as i64, y.floor() as i64, z.floor() as i64);
        let (fx, fy, fz) = (
            Self::smooth(x - x.floor()),
            Self::smooth(y - y.floor()),
            Self::smooth(z - z.floor()),
        );
        let (x1, y1, z1) = (x0 + 1, y0 + 1, z0 + 1);
        let lerp = |a: f64, b: f64, t: f64| a + (b - a) * t;
        // Trilinear interpolation of the 8 lattice corners.
        let c000 = self.cell3(seed, x0, y0, z0);
        let c100 = self.cell3(seed, x1, y0, z0);
        let c010 = self.cell3(seed, x0, y1, z0);
        let c110 = self.cell3(seed, x1, y1, z0);
        let c001 = self.cell3(seed, x0, y0, z1);
        let c101 = self.cell3(seed, x1, y0, z1);
        let c011 = self.cell3(seed, x0, y1, z1);
        let c111 = self.cell3(seed, x1, y1, z1);
        let x00 = lerp(c000, c100, fx);
        let x10 = lerp(c010, c110, fx);
        let x01 = lerp(c001, c101, fx);
        let x11 = lerp(c011, c111, fx);
        let y0v = lerp(x00, x10, fy);
        let y1v = lerp(x01, x11, fy);
        lerp(y0v, y1v, fz)
    }

    fn fbm(&self, seed: u64, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 1.0;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for octave in 0..octaves {
            sum += amp
                * self.value_noise3(
                    seed.wrapping_add((octave as u64).wrapping_mul(0x517C_C1B7_2722_0A95)),
                    x * freq,
                    y * freq,
                    z * freq,
                );
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }

    fn ridged_noise(&self, seed: u64, x: f64, y: f64, z: f64, octaves: u32) -> f64 {
        let mut sum = 0.0;
        let mut amp = 0.5;
        let mut freq = 1.0;
        let mut norm = 0.0;
        for octave in 0..octaves {
            let n = self.value_noise3(
                seed.wrapping_add((octave as u64).wrapping_mul(0x6EED_0E9D_9D95_A5C5)),
                x * freq,
                y * freq,
                z * freq,
            );
            let ridge = 1.0 - (2.0 * n - 1.0).abs();
            sum += amp * ridge * ridge;
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        sum / norm
    }
}

/// Central angle in degrees between two lat/lon points.
pub fn central_angle_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (a1, b1, a2, b2) = (
        lat1.to_radians(),
        lon1.to_radians(),
        lat2.to_radians(),
        lon2.to_radians(),
    );
    let d = (a1.sin() * a2.sin() + a1.cos() * a2.cos() * (b2 - b1).cos())
        .clamp(-1.0, 1.0)
        .acos();
    d.to_degrees()
}

/// Procedural planet terrain: multi-octave rolling terrain plus ridged
/// mountains and optional craters, all seeded and deterministic.
#[derive(Debug, Clone)]
pub struct ProceduralTerrainSource {
    pub seed: u64,
    /// Amplitude of the rolling fBm terrain, m.
    pub amplitude_m: f64,
    /// Amplitude of the ridged mountain layer, m.
    pub mountain_amplitude_m: f64,
    /// Number of craters (0 for Earth-like bodies).
    pub crater_count: u32,
    noise: ValueNoise,
}

impl Default for ProceduralTerrainSource {
    fn default() -> Self {
        Self {
            seed: 1,
            amplitude_m: 2_500.0,
            mountain_amplitude_m: 1_200.0,
            crater_count: 0,
            noise: ValueNoise,
        }
    }
}

impl ProceduralTerrainSource {
    pub fn new(seed: u64, amplitude_m: f64, mountain_amplitude_m: f64, crater_count: u32) -> Self {
        Self {
            seed,
            amplitude_m,
            mountain_amplitude_m,
            crater_count,
            noise: ValueNoise,
        }
    }

    /// Conservative analytic envelope of the rolling, ridge, and crater terms.
    pub fn elevation_bounds_m(&self) -> ElevationBounds {
        let crater_depth_m = self.crater_count as f64 * 1_600.0;
        let crater_rim_m = self.crater_count as f64 * 480.0;
        ElevationBounds::new(
            -self.amplitude_m - crater_depth_m,
            self.amplitude_m + self.mountain_amplitude_m + crater_rim_m,
        )
    }

    fn crater_field(&self, lat: f64, lon: f64) -> f64 {
        if self.crater_count == 0 {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 0..self.crater_count {
            let s = self
                .seed
                .wrapping_add(0xC3A5_C85C_97CB_3127)
                .wrapping_add(i as u64);
            let lat_c = self.noise.cell3(s, i as i64, 1, 0) * 180.0 - 90.0;
            let lon_c = self.noise.cell3(s, i as i64, 2, 0) * 360.0 - 180.0;
            let radius_deg = 0.5 + self.noise.cell3(s, i as i64, 3, 0) * 4.0;
            let depth_m = 100.0 + self.noise.cell3(s, i as i64, 4, 0) * 1_500.0;
            total += crater_height(lat, lon, lat_c, lon_c, radius_deg, depth_m);
        }
        total
    }
}

/// Parabolic crater bowl with a raised rim, in meters (negative inside).
pub fn crater_height(
    lat: f64,
    lon: f64,
    lat_c: f64,
    lon_c: f64,
    radius_deg: f64,
    depth_m: f64,
) -> f64 {
    let d = central_angle_deg(lat, lon, lat_c, lon_c);
    if d >= radius_deg {
        return 0.0;
    }
    let t = d / radius_deg;
    let bowl = -(1.0 - t * t) * depth_m;
    let rim = if t > 0.7 {
        (t - 0.7) / 0.3 * depth_m * 0.3
    } else {
        0.0
    };
    bowl + rim
}

impl TerrainSource for ProceduralTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        // Evaluate the 3D noise on the unit sphere (seamless in longitude).
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let dir = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        let base = dir * NOISE_SCALE;

        // Domain warp (best practice): displace the sample point by a
        // low-frequency vector noise field before the fractal evaluation.
        // Ridgelines meander and the axis-aligned "noise grid" disappears.
        let wx = self
            .noise
            .fbm(self.seed ^ SEED_WARP_X, base.x, base.y, base.z, 2)
            - 0.5;
        let wy = self
            .noise
            .fbm(self.seed ^ SEED_WARP_Y, base.x, base.y, base.z, 2)
            - 0.5;
        let wz = self
            .noise
            .fbm(self.seed ^ SEED_WARP_Z, base.x, base.y, base.z, 2)
            - 0.5;
        let p = DVec3::new(
            base.x + wx * WARP_STRENGTH,
            base.y + wy * WARP_STRENGTH,
            base.z + wz * WARP_STRENGTH,
        );

        // Base rolling terrain, then power redistribution: flat plains with
        // sharp peaks instead of uniformly lumpy fBm.
        let rolling01 = self.noise.fbm(self.seed, p.x, p.y, p.z, 4).clamp(0.0, 1.0);
        let rolling = rolling01.powf(SHAPE_POWER) - 0.5;
        let mountains = self
            .noise
            .ridged_noise(self.seed.wrapping_add(7), p.x, p.y, p.z, 4);
        let mut h = rolling * 2.0 * self.amplitude_m + mountains * self.mountain_amplitude_m;
        if self.crater_count > 0 {
            h += self.crater_field(latitude_deg, longitude_deg);
        }
        h
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let p = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin()) * NOISE_SCALE;
        self.noise
            .fbm(self.seed ^ SEED_MOISTURE, p.x, p.y, p.z, 3)
            .clamp(0.0, 1.0)
    }
}

/// Adds bounded near-surface relief after Earth-scale erosion. Sampling seeded
/// 3D noise on the unit sphere keeps this layer continuous across longitude and
/// cube-sphere face boundaries.
#[derive(Debug, Clone)]
pub struct LocalDetailTerrainSource {
    base: std::sync::Arc<dyn TerrainSource>,
    seed: u64,
    noise: ValueNoise,
}

impl LocalDetailTerrainSource {
    pub fn new(base: std::sync::Arc<dyn TerrainSource>, seed: u64) -> Self {
        Self {
            base,
            seed,
            noise: ValueNoise,
        }
    }

    pub const fn elevation_bounds_m() -> ElevationBounds {
        ElevationBounds {
            min_m: -36.0,
            max_m: 24.0,
        }
    }

    fn detail_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());

        // ~250 m ridges plus ~100 m drainage-like troughs at Earth's radius.
        let ridges = self.noise.ridged_noise(
            self.seed,
            direction.x * 25_000.0,
            direction.y * 25_000.0,
            direction.z * 25_000.0,
            3,
        ) - 0.5;
        let drainage_noise = self.noise.value_noise3(
            self.seed ^ 0xD2A1_6A6E,
            direction.x * 60_000.0,
            direction.y * 60_000.0,
            direction.z * 60_000.0,
        );
        let drainage = (1.0 - (drainage_noise * 2.0 - 1.0).abs()).powi(3);
        (ridges * 48.0 - drainage * 12.0).clamp(
            Self::elevation_bounds_m().min_m,
            Self::elevation_bounds_m().max_m,
        )
    }
}

impl TerrainSource for LocalDetailTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.height_m(latitude_deg, longitude_deg) + self.detail_m(latitude_deg, longitude_deg)
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.base.prepare_sample(latitude_deg, longitude_deg);
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.moisture(latitude_deg, longitude_deg)
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.river_strength(latitude_deg, longitude_deg)
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.overview_height_m(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.base.zone_lat(latitude_deg)
    }
}

/// A flat detailed launch/landing site inside the global terrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainSite {
    pub name: &'static str,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// Radius of the exactly flat pad surface.
    pub radius_deg: f64,
    /// Outer radius of the C1 grade back to the unmodified terrain. Must be
    /// greater than `radius_deg`.
    pub blend_radius_deg: f64,
    pub elevation_m: f64,
}

impl TerrainSite {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        central_angle_deg(lat, lon, self.latitude_deg, self.longitude_deg) <= self.radius_deg
    }

    fn flat_weight(&self, lat: f64, lon: f64) -> f64 {
        let distance_deg = central_angle_deg(lat, lon, self.latitude_deg, self.longitude_deg);
        if distance_deg <= self.radius_deg {
            return 1.0;
        }
        if self.blend_radius_deg <= self.radius_deg {
            return 0.0;
        }
        1.0 - ss(self.radius_deg, self.blend_radius_deg, distance_deg)
    }
}

/// A site whose base-terrain center height is calibrated once on first use in
/// its grade band. Exact pad samples need no calibration and must not force a
/// synchronous terrain bake while the scene is being constructed.
#[derive(Debug, Clone)]
struct CalibratedTerrainSite {
    site: TerrainSite,
    base_center_height_m: Arc<OnceLock<f64>>,
}

/// Detailed launch-site patches overlaid on a base terrain source. Sites
/// stay flat while a broad, smooth grade rejoins the unmodified base terrain.
#[derive(Debug, Clone)]
pub struct SiteAwareTerrainSource {
    base: std::sync::Arc<dyn TerrainSource>,
    sites: Vec<CalibratedTerrainSite>,
}

impl SiteAwareTerrainSource {
    pub fn new(base: std::sync::Arc<dyn TerrainSource>, sites: Vec<TerrainSite>) -> Self {
        let sites = sites
            .into_iter()
            .map(|site| {
                assert!(
                    site.radius_deg >= 0.0 && site.blend_radius_deg > site.radius_deg,
                    "terrain site '{}' must have a non-negative flat radius and a larger blend radius",
                    site.name
                );
                CalibratedTerrainSite {
                    site,
                    base_center_height_m: Arc::new(OnceLock::new()),
                }
            })
            .collect();
        Self { base, sites }
    }
}

impl TerrainSource for SiteAwareTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        for calibrated in &self.sites {
            let site = calibrated.site;
            let flat_weight = site.flat_weight(latitude_deg, longitude_deg);
            if flat_weight > 0.0 {
                if flat_weight == 1.0 {
                    return site.elevation_m;
                }
                let base = self.base.height_m(latitude_deg, longitude_deg);
                // Preserve local relief while carrying the center-height bias
                // across the whole grade. The smoothstep-derived flat weight
                // has zero slope at both ends, avoiding a pad-edge shelf.
                let base_center_height_m = *calibrated
                    .base_center_height_m
                    .get_or_init(|| self.base.height_m(site.latitude_deg, site.longitude_deg));
                let center_bias_m = site.elevation_m - base_center_height_m;
                let corrected_base = base + center_bias_m * flat_weight;
                return corrected_base + (site.elevation_m - corrected_base) * flat_weight;
            }
        }
        self.base.height_m(latitude_deg, longitude_deg)
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.base.prepare_sample(latitude_deg, longitude_deg);
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.moisture(latitude_deg, longitude_deg)
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        // Launch-pad flattening is too local to be meaningful in a global map.
        self.base.overview_height_m(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.base.zone_lat(latitude_deg)
    }
}

/// A heightmap-backed source sampling a raw normalized grid. Kept domain-pure
/// (no Bevy images); the renderer feeds the sampled grid in.
#[derive(Debug, Clone)]
pub struct HeightmapTerrainSource {
    pub size_px: u32,
    /// Normalized heights in [0, 1], row-major (y = row = latitude).
    pub data: Vec<f32>,
    pub lat_min_deg: f64,
    pub lat_max_deg: f64,
    pub lon_min_deg: f64,
    pub lon_max_deg: f64,
    pub height_min_m: f64,
    pub height_max_m: f64,
}

impl TerrainSource for HeightmapTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.clamp(self.lat_min_deg, self.lat_max_deg);
        let lon = longitude_deg.clamp(self.lon_min_deg, self.lon_max_deg);
        let size = self.size_px as f64;
        let fy = (lat - self.lat_min_deg) / (self.lat_max_deg - self.lat_min_deg) * (size - 1.0);
        let fx = (lon - self.lon_min_deg) / (self.lon_max_deg - self.lon_min_deg) * (size - 1.0);
        let (y0, y1) = (fy.floor() as u32, fy.ceil() as u32);
        let (x0, x1) = (fx.floor() as u32, fx.ceil() as u32);
        let at = |x: u32, y: u32| {
            let idx = (y.min(self.size_px - 1) * self.size_px + x.min(self.size_px - 1)) as usize;
            self.data.get(idx).copied().unwrap_or(0.0) as f64
        };
        let sy = fy - fy.floor();
        let sx = fx - fx.floor();
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * sx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * sx;
        let n = top + (bottom - top) * sy;
        self.height_min_m + n * (self.height_max_m - self.height_min_m)
    }
}

/// Real planetary DEM data source placeholder. Real DEM integration plugs in
/// behind the same trait without touching render or collision consumers.
#[derive(Debug, Clone, Default)]
pub struct PlanetaryDemSource;

impl TerrainSource for PlanetaryDemSource {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }
}

/// Continuous surface appearance (albedo/roughness/metallic) blended from
/// elevation, soil moisture, latitude zone and local slope — the "one
/// continuous law" terrain best-practice (glassy wash → soft hills → textured
/// slopes → carved rock), replacing hard biome bands with soft ecotones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceAppearance {
    pub albedo: [f32; 3],
    /// Perceptual roughness `[0, 1]` (higher = rougher/lambertian).
    pub roughness: f32,
    pub metallic: f32,
}

/// Helper: `smoothstep(edge0, edge1, x)`.
fn ss(a: f64, b: f64, x: f64) -> f64 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f64) -> [f32; 3] {
    let t = t as f32;
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn lerp_f(a: f32, b: f32, t: f64) -> f32 {
    a + (b - a) * t as f32
}

/// Compute a continuous, deterministic surface appearance at a terrain point.
/// `elevation_m` is height above mean radius, `moisture`/`zone_lat` in `[0,1]`,
/// `slope_deg` the local gradient (≥ 0). Pure function — unit-testable and
/// shared by the render and any future biome-gated systems.
pub fn surface_appearance(
    elevation_m: f64,
    moisture: f64,
    zone_lat: f64,
    slope_deg: f64,
) -> SurfaceAppearance {
    const SAND: [f32; 3] = [0.76, 0.70, 0.50];
    const GRASS: [f32; 3] = [0.32, 0.50, 0.22];
    const FOREST: [f32; 3] = [0.26, 0.42, 0.16];
    const SAVANNA: [f32; 3] = [0.62, 0.56, 0.30];
    const DESERT: [f32; 3] = [0.80, 0.74, 0.55];
    const TUNDRA: [f32; 3] = [0.62, 0.60, 0.58];
    const POLAR: [f32; 3] = [0.92, 0.94, 0.97];
    const ROCK: [f32; 3] = [0.45, 0.42, 0.40];
    const SNOW: [f32; 3] = [0.94, 0.95, 0.98];
    const SEAFLOOR: [f32; 3] = [0.08, 0.16, 0.22];

    // Seafloor below sea level.
    if elevation_m < 0.0 {
        let depth = (-elevation_m).min(4000.0) / 4000.0;
        let albedo = lerp3([0.12, 0.28, 0.42], SEAFLOOR, depth);
        return SurfaceAppearance {
            albedo,
            roughness: lerp_f(0.25, 0.7, depth),
            metallic: 0.0,
        };
    }

    // Shoreline → sand.
    let sand_t = 1.0 - ss(0.0, 4.0, elevation_m.min(4.0));
    let mut albedo = lerp3(SAND, GRASS, 1.0 - sand_t);

    // Moisture drives grass → forest (wet) / savanna → desert (dry).
    if moisture < 0.4 {
        let dry_t = ss(0.4, 0.15, moisture); // dries below 0.4
        albedo = lerp3(albedo, SAVANNA, dry_t * 0.6);
        albedo = lerp3(albedo, DESERT, (dry_t * 0.5) * ss(120.0, 0.0, elevation_m));
    } else {
        let wet_t = ss(0.4, 0.75, moisture);
        albedo = lerp3(albedo, FOREST, wet_t * 0.7);
    }

    // Latitude zone: cold toward the poles.
    let polar_dist = (zone_lat - 0.5).abs() * 2.0; // 0 equator → 1 pole
    let cold_t = ss(0.55, 0.9, polar_dist);
    albedo = lerp3(albedo, TUNDRA, cold_t * 0.6);
    albedo = lerp3(albedo, POLAR, cold_t * ss(0.8, 1.0, polar_dist));

    // Steep → bare rock, cliff edges keep their sharp character.
    let rock_t = ss(35.0, 55.0, slope_deg);
    albedo = lerp3(albedo, ROCK, rock_t);

    // Snow line: high altitude above the snow band turns white (low roughness).
    let snow_t = ss(4500.0, 5200.0, elevation_m);
    albedo = lerp3(albedo, SNOW, snow_t);
    let roughness = 0.85 - 0.35 * snow_t;

    SurfaceAppearance {
        albedo,
        roughness: roughness as f32,
        metallic: 0.0,
    }
}

/// Blend a ground appearance with a source-derived river channel. The strength
/// comes from the erosion source, rather than an independent render-time water
/// field, so terrain color agrees with its carved drainage network.
pub fn with_river_appearance(
    mut appearance: SurfaceAppearance,
    river_strength: f64,
) -> SurfaceAppearance {
    let strength = river_strength.clamp(0.0, 1.0);
    let wet_bank = strength.sqrt() * 0.32;
    appearance.albedo = lerp3(appearance.albedo, [0.10, 0.18, 0.12], wet_bank);

    let channel = ss(0.12, 0.65, strength);
    appearance.albedo = lerp3(appearance.albedo, [0.012, 0.075, 0.14], channel);
    appearance.roughness = lerp_f(appearance.roughness, 0.18, channel);
    appearance
}

/// Local terrain slope (degrees ≥ 0) at a lat/lon by central differences over
/// a small arc. Uses the authoritative source; deterministic for a fixed source.
pub fn slope_deg_at(source: &dyn TerrainSource, latitude_deg: f64, longitude_deg: f64) -> f64 {
    let d = 0.02; // ~2 km probe for a stable, feature-scale gradient
    let hx = source.height_m(latitude_deg, longitude_deg + d)
        - source.height_m(latitude_deg, longitude_deg - d);
    let hy = source.height_m(latitude_deg + d, longitude_deg)
        - source.height_m(latitude_deg - d, longitude_deg);
    let lat_m = 111_320.0; // meters per degree of latitude
    let lon_m = (111_320.0 * latitude_deg.to_radians().cos()).abs().max(1.0);
    let grad = (hx / (2.0 * d * lon_m)).hypot(hy / (2.0 * d * lat_m));
    grad.atan().to_degrees()
}

/// The shared terrain source for a planet by name. Earth is the only body with
/// an active terrain/collision authority. Other catalog bodies are currently
/// texture-backed presentation only, so callers receive a flat source rather
/// than silently inventing procedural physical terrain for them.
pub fn terrain_source_for(name: &str) -> std::sync::Arc<dyn TerrainSource> {
    let base: std::sync::Arc<dyn TerrainSource> = match name {
        "Earth" => layered_earth_source(),
        _ => std::sync::Arc::new(FlatTerrainSource),
    };
    with_sites(base, terrain_sites(name))
}

#[derive(Debug)]
struct FlatTerrainSource;

impl TerrainSource for FlatTerrainSource {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }
}

fn zero_elevation_layer() -> TerrainElevationLayer {
    TerrainElevationLayer::new(Arc::new(FlatTerrainSource), ElevationBounds::new(0.0, 0.0))
}

fn layered_earth_source() -> Arc<dyn TerrainSource> {
    let procedural = Arc::new(ProceduralTerrainSource::new(0xE4A7, 2_500.0, 1_200.0, 0));
    let eroded = erode_earth(procedural);
    let detail = Arc::new(LocalDetailTerrainSource::new(
        Arc::new(FlatTerrainSource),
        0xE4A7_D371,
    ));

    // The erosion solver has a finite, configuration-bounded material budget.
    // This broad interval remains conservative without probing terrain samples.
    let eroded_bounds = ElevationBounds::new(-10_000.0, 20_000_000.0);
    Arc::new(LayeredTerrainSource::new(
        zero_elevation_layer(),
        Some(TerrainElevationLayer::new(eroded, eroded_bounds)),
        None,
        Some(TerrainDetailLayer::new(
            detail,
            LocalDetailTerrainSource::elevation_bounds_m(),
            DetailLodFade::new(3, 6),
        )),
    ))
}

fn terrain_sites(name: &str) -> Vec<TerrainSite> {
    match name {
        "Earth" => vec![
            TerrainSite {
                name: "Kennedy Space Center",
                latitude_deg: 28.5721,
                longitude_deg: -80.6480,
                // ~17 m flat pad, then an ~11 km C1 grade. The procedural
                // Earth can differ substantially from the surveyed 2 m pad.
                radius_deg: 0.00015,
                blend_radius_deg: 0.1,
                elevation_m: 2.0,
            },
            TerrainSite {
                name: "RTLS Landing Pad",
                latitude_deg: 28.61,
                longitude_deg: -80.55,
                radius_deg: 0.00015,
                blend_radius_deg: 0.05,
                elevation_m: 3.0,
            },
            TerrainSite {
                name: "Drone Ship",
                latitude_deg: 28.50,
                longitude_deg: -80.05,
                radius_deg: 0.00025,
                blend_radius_deg: 0.03,
                elevation_m: 0.0,
            },
        ],
        "Moon" => vec![TerrainSite {
            name: "Lunar Landing Site",
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            radius_deg: 0.0003,
            blend_radius_deg: 0.02,
            elevation_m: 0.0,
        }],
        _ => vec![],
    }
}

fn with_sites(
    base: std::sync::Arc<dyn TerrainSource>,
    sites: Vec<TerrainSite>,
) -> std::sync::Arc<dyn TerrainSource> {
    if sites.is_empty() {
        base
    } else {
        std::sync::Arc::new(SiteAwareTerrainSource::new(base, sites))
    }
}

/// Select raw SRTM data for Earth when an application-validated local directory
/// is available. Missing tile coverage still uses `DemTerrainSource`'s seeded
/// procedural fallback, and no DEM sample passes through coarse erosion.
#[cfg(feature = "dem")]
pub fn terrain_source_for_with_srtm_dir(
    name: &str,
    data_dir: Option<&Path>,
) -> std::sync::Arc<dyn TerrainSource> {
    let Some(data_dir) = data_dir.filter(|dir| dir.is_dir()) else {
        return terrain_source_for(name);
    };
    if name != "Earth" {
        return terrain_source_for(name);
    }

    use crate::domain::services::dem_terrain_source::{DemTerrainConfig, DemTerrainSource};
    let source = Arc::new(DemTerrainSource::new(DemTerrainConfig {
        data_dir: Some(data_dir.to_path_buf()),
        ..DemTerrainConfig::default()
    }));
    let composed: Arc<dyn TerrainSource> = Arc::new(LayeredTerrainSource::new(
        zero_elevation_layer(),
        None,
        // SRTM posts are signed 16-bit meters; the embedded procedural fallback
        // is safely within this interval when a local tile is unavailable.
        Some(TerrainElevationLayer::new(
            source,
            ElevationBounds::new(i16::MIN as f64, i16::MAX as f64),
        )),
        None,
    ));
    with_sites(composed, terrain_sites(name))
}

/// Wrap an Earth base source with deterministic hydraulic/thermal erosion and
/// river carving (T2). The site layer sits *above* this so launch pads stay
/// flat regardless of erosion.
fn erode_earth(base: std::sync::Arc<dyn TerrainSource>) -> std::sync::Arc<dyn TerrainSource> {
    std::sync::Arc::new(ErodedTerrainSource::new(base, ErosionConfig::default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn procedural_regeneration_is_identical() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let a = source.height_m(12.34, -45.67);
        let b = source.height_m(12.34, -45.67);
        assert_eq!(a, b);
    }

    #[test]
    fn non_earth_bodies_do_not_receive_procedural_terrain() {
        let mars = terrain_source_for("Mars");
        let moon = terrain_source_for("Moon");

        assert_eq!(mars.height_m(12.34, -45.67), 0.0);
        assert_eq!(moon.height_m(12.34, -45.67), 0.0);
    }

    #[test]
    fn procedural_is_independent_of_evaluation_order() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let points = [(-30.0, 120.0), (30.0, -120.0), (0.0, 0.0)];
        let forward: Vec<f64> = points
            .iter()
            .map(|(la, lo)| source.height_m(*la, *lo))
            .collect();
        let reverse: Vec<f64> = points
            .iter()
            .rev()
            .map(|(la, lo)| source.height_m(*la, *lo))
            .collect();
        assert_eq!(forward[0], reverse[2]);
        assert_eq!(forward[2], reverse[0]);
    }

    #[test]
    fn different_seeds_differ() {
        let a = ProceduralTerrainSource::new(1, 2_500.0, 1_200.0, 0);
        let b = ProceduralTerrainSource::new(2, 2_500.0, 1_200.0, 0);
        let (la, lo) = (10.0, 20.0);
        assert_ne!(a.height_m(la, lo), b.height_m(la, lo));
    }

    #[test]
    fn heightmap_source_samples_grid() {
        let size = 3u32;
        // Corner values 0..=1 for a bilinear sanity check.
        let data = vec![0.0, 0.5, 1.0, 0.0, 0.5, 1.0, 0.0, 0.5, 1.0];
        let source = HeightmapTerrainSource {
            size_px: size,
            data,
            lat_min_deg: -1.0,
            lat_max_deg: 1.0,
            lon_min_deg: -1.0,
            lon_max_deg: 1.0,
            height_min_m: -100.0,
            height_max_m: 100.0,
        };
        // Center of the grid → normalized 0.5 → 0 m.
        let h = source.height_m(0.0, 0.0);
        assert!((h - 0.0).abs() < 1e-6);
        // Top-right corner → normalized 1.0 → +100 m.
        assert!((source.height_m(1.0, 1.0) - 100.0).abs() < 1e-6);
        // Bottom-left → -100 m.
        assert!((source.height_m(-1.0, -1.0) - (-100.0)).abs() < 1e-6);
    }

    #[test]
    fn source_swap_preserves_interface() {
        let procedural: &dyn TerrainSource = &ProceduralTerrainSource::default();
        let dem: &dyn TerrainSource = &PlanetaryDemSource;
        // Both expose the same height interface; swap is a type-level change.
        let _ = procedural.height_m(0.0, 0.0);
        let _ = dem.height_m(0.0, 0.0);
        assert_eq!(PlanetaryDemSource.height_m(0.0, 0.0), 0.0);
    }

    #[test]
    fn site_aware_source_flattens_launch_sites() {
        let source = terrain_source_for("Earth");
        // Inside the KSC site the height is the flat pad elevation.
        let ksc_height = source.height_m(28.5721, -80.6480);
        assert!((ksc_height - 2.0).abs() < 1e-9);
        // Far from any site the procedural base applies.
        let far = source.height_m(-40.0, 100.0);
        assert!(
            far.abs() > 10.0,
            "expected non-flat global terrain, got {far}"
        );
    }

    #[derive(Debug, Default)]
    struct CountingTerrainSource(AtomicUsize);

    impl TerrainSource for CountingTerrainSource {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.0.fetch_add(1, Ordering::Relaxed);
            10.0
        }
    }

    #[test]
    fn site_calibration_is_lazy_and_exact_pad_samples_do_not_touch_the_base() {
        let base = Arc::new(CountingTerrainSource::default());
        let source = SiteAwareTerrainSource::new(
            base.clone(),
            vec![TerrainSite {
                name: "test pad",
                latitude_deg: 0.0,
                longitude_deg: 0.0,
                radius_deg: 0.01,
                blend_radius_deg: 0.02,
                elevation_m: 2.0,
            }],
        );

        assert_eq!(base.0.load(Ordering::Relaxed), 0);
        assert_eq!(source.height_m(0.0, 0.0), 2.0);
        assert_eq!(base.0.load(Ordering::Relaxed), 0);

        let _ = source.height_m(0.015, 0.0);
        assert!(base.0.load(Ordering::Relaxed) >= 2);
    }

    #[test]
    fn ksc_spawn_surface_sample_matches_the_authoritative_pad_height() {
        let source = terrain_source_for("Earth");
        let launch_site = crate::domain::value_objects::launch_site_coordinates::predefined_sites::kennedy_space_center();
        let earth = crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
            .expect("Earth exists");
        let sample = crate::domain::services::terrain_collision::sample_surface(
            source.as_ref(),
            launch_site.latitude_deg as f64,
            launch_site.longitude_deg as f64,
            earth.radius_km as f64 * 1_000.0,
        );
        assert_eq!(
            sample.height_m, 2.0,
            "rocket spawning must use the authoritative KSC pad elevation"
        );
    }

    #[test]
    fn local_detail_is_deterministic_seam_safe_and_varied_nearby() {
        let base = std::sync::Arc::new(ProceduralTerrainSource::new(42, 0.0, 0.0, 0));
        let source = LocalDetailTerrainSource::new(base, 99);
        let point = (28.573, -80.647);
        assert_eq!(
            source.height_m(point.0, point.1),
            source.height_m(point.0, point.1)
        );

        let nearby: Vec<f64> = (0..8)
            .map(|step| source.height_m(point.0, point.1 + step as f64 * 0.0005))
            .collect();
        let range = nearby.iter().copied().fold(f64::NEG_INFINITY, f64::max)
            - nearby.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(range > 0.01, "expected local relief, got range {range}");
        assert!(
            nearby.iter().all(|height| (-36.0..=24.0).contains(height)),
            "local detail exceeded its bounded envelope: {nearby:?}"
        );

        let east = source.height_m(10.0, 179.9999);
        let west = source.height_m(10.0, -179.9999);
        assert!(
            (east - west).abs() < 1.0,
            "local detail must remain continuous across the longitude seam: {east} vs {west}"
        );
    }

    #[derive(Debug)]
    struct ConstantTerrain(f64);

    impl TerrainSource for ConstantTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.0
        }
    }

    #[test]
    fn layered_source_sums_explicit_contributions_with_conservative_bounds() {
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(10.0)),
                ElevationBounds::new(10.0, 10.0),
            ),
            Some(TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(-4.0)),
                ElevationBounds::new(-4.0, -4.0),
            )),
            Some(TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(3.0)),
                ElevationBounds::new(3.0, 3.0),
            )),
            Some(TerrainDetailLayer::new(
                Arc::new(ConstantTerrain(-2.0)),
                ElevationBounds::new(-2.0, 4.0),
                DetailLodFade::new(3, 6),
            )),
        );

        assert_eq!(source.height_m(12.0, -45.0), 7.0);
        assert_eq!(source.elevation_bounds_m(), ElevationBounds::new(7.0, 13.0));
    }

    #[test]
    fn detail_lod_fade_is_continuous_and_does_not_change_physical_height() {
        let fade = DetailLodFade::new(3, 6);
        assert_eq!(fade.weight_for_level(3), 0.0);
        assert_eq!(fade.weight_for_level(6), 1.0);
        assert!(fade.weight_for_level(4) > 0.0 && fade.weight_for_level(4) < 1.0);
        assert!(fade.weight_for_level(5) > fade.weight_for_level(4));
        assert!((fade.weight_for_level(4) + fade.weight_for_level(5) - 1.0).abs() < 1e-12);

        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ConstantTerrain(10.0)),
                ElevationBounds::new(10.0, 10.0),
            ),
            None,
            None,
            Some(TerrainDetailLayer::new(
                Arc::new(ConstantTerrain(4.0)),
                ElevationBounds::new(4.0, 4.0),
                fade,
            )),
        );
        assert_eq!(source.height_m(0.0, 0.0), 14.0);
        assert_eq!(source.height_m(0.0, 0.0), 14.0);
    }

    #[test]
    fn layered_source_is_deterministic_and_continuous_at_the_longitude_seam() {
        let make_source = || {
            let base = Arc::new(ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0));
            let detail = Arc::new(LocalDetailTerrainSource::new(
                Arc::new(FlatTerrainSource),
                99,
            ));
            LayeredTerrainSource::new(
                TerrainElevationLayer::new(base.clone(), base.elevation_bounds_m()),
                None,
                None,
                Some(TerrainDetailLayer::new(
                    detail,
                    LocalDetailTerrainSource::elevation_bounds_m(),
                    DetailLodFade::new(3, 6),
                )),
            )
        };
        let first = make_source();
        let second = make_source();
        let point = (10.0, 179.9999);
        assert_eq!(
            first.height_m(point.0, point.1),
            second.height_m(point.0, point.1)
        );

        let east = first.height_m(10.0, 180.0);
        let west = first.height_m(10.0, -180.0);
        assert!(
            (east - west).abs() < 1e-6,
            "layered source must be continuous at the longitude seam: {east} vs {west}"
        );
    }

    #[test]
    fn collision_and_render_samples_agree_on_composed_height() {
        let detail = Arc::new(LocalDetailTerrainSource::new(
            Arc::new(FlatTerrainSource),
            99,
        ));
        let source = LayeredTerrainSource::new(
            TerrainElevationLayer::new(
                Arc::new(ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0)),
                ElevationBounds::new(-2_500.0, 3_700.0),
            ),
            None,
            None,
            Some(TerrainDetailLayer::new(
                detail,
                LocalDetailTerrainSource::elevation_bounds_m(),
                DetailLodFade::new(3, 6),
            )),
        );
        let (lat, lon) = (33.0, -110.0);
        let render_height = source.height_m(lat, lon);
        let collision = crate::domain::services::terrain_collision::sample_surface(
            &source,
            lat,
            lon,
            6_371_000.0,
        );
        assert_eq!(collision.height_m, render_height);
    }

    #[cfg(feature = "dem")]
    #[test]
    fn unavailable_srtm_directory_keeps_the_composed_earth_fallback() {
        let expected = terrain_source_for("Earth");
        let fallback = terrain_source_for_with_srtm_dir("Earth", None);
        let point = (28.5721, -80.6480);
        assert_eq!(
            fallback.height_m(point.0, point.1),
            expected.height_m(point.0, point.1)
        );
    }

    #[derive(Debug)]
    struct SlopedTerrain;

    impl TerrainSource for SlopedTerrain {
        fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
            latitude_deg * 100.0 + longitude_deg * 20.0
        }
    }

    #[test]
    fn site_pad_is_flat_and_grades_continuously_to_calibrated_base() {
        let site = TerrainSite {
            name: "Test pad",
            latitude_deg: 10.0,
            longitude_deg: 20.0,
            radius_deg: 0.001,
            blend_radius_deg: 0.02,
            elevation_m: 7.0,
        };
        let base = std::sync::Arc::new(SlopedTerrain);
        let source = SiteAwareTerrainSource::new(base.clone(), vec![site]);

        assert_eq!(source.height_m(10.0005, 20.0), 7.0);
        let outer = 10.0 + site.blend_radius_deg;
        assert!((source.height_m(outer, 20.0) - base.height_m(outer, 20.0)).abs() < 1e-9);

        let inside = source.height_m(outer - 0.000001, 20.0);
        let outside = source.height_m(outer + 0.000001, 20.0);
        assert!(
            (inside - outside).abs() < 0.01,
            "pad boundary must be continuous: {inside} vs {outside}"
        );
    }

    #[derive(Debug)]
    struct SteepOffsetTerrain;

    impl TerrainSource for SteepOffsetTerrain {
        fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
            2_500.0 + (latitude_deg - 10.0) * 30_000.0 + (longitude_deg - 20.0) * 4_000.0
        }
    }

    #[test]
    fn calibrated_site_avoids_a_cliff_at_the_pad_and_grade_boundaries() {
        let site = TerrainSite {
            name: "Calibrated test pad",
            latitude_deg: 10.0,
            longitude_deg: 20.0,
            radius_deg: 0.001,
            blend_radius_deg: 0.1,
            elevation_m: 7.0,
        };
        let base = std::sync::Arc::new(SteepOffsetTerrain);
        let source = SiteAwareTerrainSource::new(base.clone(), vec![site]);
        let step_deg = 0.00001;

        assert_eq!(source.height_m(10.0, 20.0), site.elevation_m);
        assert_eq!(
            source.height_m(10.0 + site.radius_deg * 0.5, 20.0),
            site.elevation_m
        );

        let pad_edge_inside = source.height_m(10.0 + site.radius_deg, 20.0);
        let pad_edge_outside = source.height_m(10.0 + site.radius_deg + step_deg, 20.0);
        assert!(
            (pad_edge_outside - pad_edge_inside).abs() < 0.01,
            "C1 pad edge must not become a shelf: {pad_edge_inside} vs {pad_edge_outside}"
        );

        let grade_edge = 10.0 + site.blend_radius_deg;
        let before_grade_edge = source.height_m(grade_edge - step_deg, 20.0);
        let after_grade_edge = source.height_m(grade_edge + step_deg, 20.0);
        assert!(
            (source.height_m(grade_edge - step_deg, 20.0)
                - base.height_m(grade_edge - step_deg, 20.0))
            .abs()
                < 0.01,
            "grade must converge to the base without a hard wall"
        );
        assert!(
            (after_grade_edge - before_grade_edge - 2.0 * step_deg * 30_000.0).abs() < 0.01,
            "grade boundary must retain the base terrain slope: {before_grade_edge} vs {after_grade_edge}"
        );
        assert_eq!(
            source.height_m(grade_edge + 0.01, 20.0),
            base.height_m(grade_edge + 0.01, 20.0),
            "terrain relief outside the grade must remain the base terrain"
        );
    }

    #[test]
    fn crater_height_is_a_depression() {
        // At the crater center the height is the negative depth (bowl).
        let h = crater_height(10.0, 10.0, 10.0, 10.0, 3.0, 500.0);
        assert!((h + 500.0).abs() < 1e-6);
        // Far away the crater contributes nothing.
        assert_eq!(crater_height(10.0, 40.0, 10.0, 10.0, 3.0, 500.0), 0.0);
    }

    #[test]
    fn longitude_noise_is_seamless() {
        let source = ProceduralTerrainSource::default();
        let a = source.height_m(10.0, 179.5);
        let b = source.height_m(10.0, -179.5);
        // Points 1° apart across the ±180° seam stay continuous (3D noise).
        assert!((a - b).abs() < 800.0, "seam discontinuity: {a} vs {b}");
        // ...and are far closer than two distant longitudes.
        let far = source.height_m(10.0, 20.0);
        assert!(
            (a - far).abs() > 5.0,
            "expected terrain to vary between distant longitudes: {a} vs {far}"
        );
    }

    #[test]
    fn domain_warped_height_stays_deterministic_and_bounded() {
        // Adjacent points are continuous; amplitude stays within the configured
        // envelope (rolling ±amplitude + mountains·mountain_amplitude).
        let source = ProceduralTerrainSource::new(99, 2_000.0, 800.0, 0);
        let a = source.height_m(36.5, -90.4);
        let b = source.height_m(36.5, -90.4);
        assert_eq!(a, b, "height must be deterministic");
        let h = source.height_m(10.0, 20.0);
        assert!(h.is_finite());
        // Warping never blows past the amplitude + mountain envelope.
        for (la, lo) in [(-20.0, 30.0), (50.0, -120.0), (0.0, 0.0), (80.0, 90.0)] {
            let v = source.height_m(la, lo);
            assert!(
                v.abs() <= 2000.0 + 800.0 + 1.0,
                "height {v} exceeded envelope at ({la},{lo})"
            );
        }
    }

    #[test]
    fn moisture_is_normalized_deterministic() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        for (la, lo) in [(0.0, 0.0), (30.0, -60.0), (-45.0, 120.0)] {
            let m = source.moisture(la, lo);
            assert!((0.0..=1.0).contains(&m), "moisture {m} out of range");
            assert_eq!(m, source.moisture(la, lo));
        }
    }

    #[test]
    fn surface_appearance_varies_continuously() {
        // Wet grassland → arid desert: distinct albedo.
        let wet = surface_appearance(300.0, 0.8, 0.5, 5.0);
        let dry = surface_appearance(300.0, 0.05, 0.5, 5.0);
        assert_ne!(wet.albedo, dry.albedo, "moisture must change the biome");
        // Snow line shifts toward white above 5 km, and is rougher below.
        let low = surface_appearance(1_000.0, 0.5, 0.5, 10.0);
        let high = surface_appearance(5_500.0, 0.5, 0.5, 10.0);
        assert!(
            high.albedo[0] > 0.9 && high.albedo[1] > 0.9,
            "snow must be near-white"
        );
        assert!(high.roughness < low.roughness, "snow is less rough");
        // Steep faces read as bare rock.
        let cliff = surface_appearance(1_000.0, 0.5, 0.5, 60.0);
        assert!(
            (cliff.albedo[0] - 0.45).abs() < 0.08 && (cliff.albedo[1] - 0.42).abs() < 0.08,
            "cliff should trend toward rocky grey {:?}",
            cliff.albedo
        );
        // Seafloor below sea level.
        assert_eq!(surface_appearance(-100.0, 0.5, 0.5, 0.0).metallic, 0.0);
    }

    #[test]
    fn river_appearance_is_darker_smoother_and_blue_shifted() {
        let ground = surface_appearance(300.0, 0.5, 0.5, 5.0);
        let river = with_river_appearance(ground, 1.0);

        assert!(river.albedo[0] < ground.albedo[0]);
        assert!(river.albedo[2] > river.albedo[1]);
        assert!(river.roughness < ground.roughness);
    }

    #[test]
    fn slope_deg_at_is_finite_and_zero_on_flat() {
        let flat = crate::domain::services::terrain_source::HeightmapTerrainSource {
            size_px: 3,
            data: vec![5.0; 9],
            lat_min_deg: -1.0,
            lat_max_deg: 1.0,
            lon_min_deg: -1.0,
            lon_max_deg: 1.0,
            height_min_m: 0.0,
            height_max_m: 10.0,
        };
        let s = slope_deg_at(&flat, 0.0, 0.0);
        assert!(s.abs() < 1e-6, "flat terrain must have ~0 slope, got {s}");
        let proc = ProceduralTerrainSource::default();
        assert!(slope_deg_at(&proc, 10.0, 20.0).is_finite());
    }
}
