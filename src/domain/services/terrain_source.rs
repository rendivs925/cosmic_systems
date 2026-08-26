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

    /// Normalized soil moisture in `[0, 1]` (drives vegetation/biome). Sources
    /// without a moisture model default to a neutral `0.5` so biomes still
    /// vary by elevation and latitude.
    fn moisture(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.5
    }

    /// Normalized latitude zone in `[0, 1]` (`0` = south pole, `1` = north
    /// pole), used to fade cold-biome coloring toward the poles. Default maps
    /// latitude linearly; planets cold/temperate callers may override.
    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        ((latitude_deg + 90.0) / 180.0).clamp(0.0, 1.0)
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
        let rolling01 = (self.noise.fbm(self.seed, p.x, p.y, p.z, 4)).clamp(-0.5, 0.5) + 0.5;
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

/// A flat detailed launch/landing site inside the global terrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainSite {
    pub name: &'static str,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub radius_deg: f64,
    pub elevation_m: f64,
}

impl TerrainSite {
    pub fn contains(&self, lat: f64, lon: f64) -> bool {
        central_angle_deg(lat, lon, self.latitude_deg, self.longitude_deg) <= self.radius_deg
    }
}

/// Detailed launch-site patches overlaid on a base terrain source. Sites
/// stay flat (localized objects) while the rest of the planet uses the base.
#[derive(Debug, Clone)]
pub struct SiteAwareTerrainSource {
    pub base: std::sync::Arc<dyn TerrainSource>,
    pub sites: Vec<TerrainSite>,
}

impl TerrainSource for SiteAwareTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        for site in &self.sites {
            if site.contains(latitude_deg, longitude_deg) {
                return site.elevation_m;
            }
        }
        self.base.height_m(latitude_deg, longitude_deg)
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

/// The shared terrain source for a planet by name: Earth gets flat detailed
/// launch sites over a procedural base; the Moon gets a cratered procedural
/// surface with a landing site. Other bodies use a plain procedural base.
/// When the `dem` feature is enabled, Earth uses SRTM DEM data with procedural fallback.
pub fn terrain_source_for(name: &str) -> std::sync::Arc<dyn TerrainSource> {
    let sites = match name {
        "Earth" => vec![
            TerrainSite {
                name: "Kennedy Space Center",
                latitude_deg: 28.5721,
                longitude_deg: -80.6480,
                radius_deg: 0.09,
                elevation_m: 2.0,
            },
            TerrainSite {
                name: "RTLS Landing Pad",
                latitude_deg: 28.61,
                longitude_deg: -80.55,
                radius_deg: 0.05,
                elevation_m: 3.0,
            },
            TerrainSite {
                name: "Drone Ship",
                latitude_deg: 28.50,
                longitude_deg: -80.05,
                radius_deg: 0.05,
                elevation_m: 0.0,
            },
        ],
        "Moon" => vec![TerrainSite {
            name: "Lunar Landing Site",
            latitude_deg: 0.0,
            longitude_deg: 0.0,
            radius_deg: 0.05,
            elevation_m: 0.0,
        }],
        _ => vec![],
    };

    #[cfg(feature = "dem")]
    {
        use crate::domain::services::dem_terrain_source::DemTerrainSource;
        if name == "Earth" {
            let dem = DemTerrainSource::new(
                crate::domain::services::dem_terrain_source::DemTerrainConfig::default(),
            );
            let dem_arc = std::sync::Arc::new(dem);
            let eroded = erode_earth(dem_arc);
            if !sites.is_empty() {
                return std::sync::Arc::new(SiteAwareTerrainSource {
                    base: eroded,
                    sites,
                });
            }
            return eroded;
        }
    }

    let base: std::sync::Arc<dyn TerrainSource> = match name {
        "Earth" => {
            let procedural =
                std::sync::Arc::new(ProceduralTerrainSource::new(0xE4A7, 2_500.0, 1_200.0, 0));
            erode_earth(procedural)
        }
        "Moon" => std::sync::Arc::new(ProceduralTerrainSource::new(0x4C55, 1_200.0, 500.0, 14)),
        _ => std::sync::Arc::new(ProceduralTerrainSource::new(0x5117, 2_000.0, 900.0, 0)),
    };
    if sites.is_empty() {
        base
    } else {
        std::sync::Arc::new(SiteAwareTerrainSource { base, sites })
    }
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

    #[test]
    fn procedural_regeneration_is_identical() {
        let source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let a = source.height_m(12.34, -45.67);
        let b = source.height_m(12.34, -45.67);
        assert_eq!(a, b);
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
