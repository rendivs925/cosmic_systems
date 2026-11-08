//! Deterministic procedural terrain sources (continental fBm, orogeny,
//! ridged mountains, craters, and bounded local detail).

use super::{central_angle_deg, ss, ElevationBounds, TerrainSource, ValueNoise};
use crate::domain::math::DVec3;
use crate::domain::services::cube_sphere::{PatchGeometricError, TerrainPatch};

/// Feature-scale of the 3D value-noise field on the unit sphere: higher values
/// produce more, smaller features across the planet.
const NOISE_SCALE: f64 = 10.0;

/// Landscape scale of the wetness field that drives the biome/material law.
/// The geometric drainage troughs are ~200 m (see
/// [`ProceduralDetailSource::drainage_strength_for_direction`]); reusing that
/// signal directly as moisture makes the material ecotone finer than the small
/// per-patch weight map can resolve, so different mesh LODs alias it differently
/// and the ground seams across shared patch edges. The wetness is therefore a
/// low-frequency fractal field — one lattice cell of this scale is about 2 km on
/// Earth — that every LOD can represent and that stays continuous across patches,
/// while still spanning enough of `[0, 1]` for grass, soil and sand to appear.
const MOISTURE_NOISE_SCALE: f64 = 3000.0;

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
const SEED_CONTINENTS: u64 = 0x6A09_E667_F3BC_C909;
const SEED_OROGENY: u64 = 0xBB67_AE85_84CA_A73B;
const CONTINENTAL_SCALE: f64 = 1.35;
const CONTINENTAL_AMPLITUDE: f64 = 1.1;
const ROLLING_AMPLITUDE: f64 = 1.4;
const OROGENY_SCALE: f64 = 0.32;

/// Continental elevation curve thresholds on the normalized continental mask.
/// Below [`COAST_MASK`] is ocean basin, up to [`FOOTHILL_MASK`] is coastal
/// lowland, and above is the alpine ramp (Module 1, stage 4).
const COAST_MASK: f64 = 0.52;
const FOOTHILL_MASK: f64 = 0.75;

/// Piecewise monotonic remap of the normalized continental mask into a signed
/// elevation. The ocean segment is kept identical to the established linear
/// basin so coastlines and launch-site land/ocean classification never shift;
/// the land segment flattens coastal lowlands and then steepens toward the
/// alpine peaks (Module 1, stage 4). `C^0`-continuous at every join.
fn continental_curve(mask: f64) -> f64 {
    let mask = mask.clamp(0.0, 1.0);
    if mask < COAST_MASK {
        mask - COAST_MASK
    } else if mask < FOOTHILL_MASK {
        let t = (mask - COAST_MASK) / (FOOTHILL_MASK - COAST_MASK);
        // Gentle coastal lowland and rolling hills.
        0.15 * t * t
    } else {
        let t = (mask - FOOTHILL_MASK) / (1.0 - FOOTHILL_MASK);
        // Steepening alpine ramp toward the highest peaks.
        0.15 + 0.85 * t.powf(1.5)
    }
}

/// Typed, validated parameters for deterministic procedural terrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProceduralTerrainConfig {
    seed: u64,
    rolling_amplitude_m: f64,
    mountain_amplitude_m: f64,
    crater_count: u32,
}

impl ProceduralTerrainConfig {
    pub fn new(
        seed: u64,
        rolling_amplitude_m: f64,
        mountain_amplitude_m: f64,
        crater_count: u32,
    ) -> Self {
        assert!(
            rolling_amplitude_m.is_finite() && rolling_amplitude_m >= 0.0,
            "rolling terrain amplitude must be finite and non-negative"
        );
        assert!(
            mountain_amplitude_m.is_finite() && mountain_amplitude_m >= 0.0,
            "mountain terrain amplitude must be finite and non-negative"
        );
        Self {
            seed,
            rolling_amplitude_m,
            mountain_amplitude_m,
            crater_count,
        }
    }

    pub const fn earth() -> Self {
        Self {
            seed: 0xE4A7,
            rolling_amplitude_m: 2_500.0,
            mountain_amplitude_m: 1_200.0,
            crater_count: 0,
        }
    }
}

/// Procedural planet terrain: continental fBm, regional orogeny, ridged
/// mountains, and optional craters, all seeded and deterministic.
#[derive(Debug, Clone)]
pub struct ProceduralTerrainSource {
    config: ProceduralTerrainConfig,
    noise: ValueNoise,
}

impl Default for ProceduralTerrainSource {
    fn default() -> Self {
        Self {
            config: ProceduralTerrainConfig::new(1, 2_500.0, 1_200.0, 0),
            noise: ValueNoise,
        }
    }
}

impl ProceduralTerrainSource {
    pub fn new(seed: u64, amplitude_m: f64, mountain_amplitude_m: f64, crater_count: u32) -> Self {
        Self::from_config(ProceduralTerrainConfig::new(
            seed,
            amplitude_m,
            mountain_amplitude_m,
            crater_count,
        ))
    }

    pub fn from_config(config: ProceduralTerrainConfig) -> Self {
        Self {
            config,
            noise: ValueNoise,
        }
    }

    /// Conservative analytic envelope of the rolling, ridge, and crater terms.
    /// The ocean segment is unchanged from the linear basin (`-0.52`); the land
    /// segment reaches `1.0`, and the rolling term spans `[-0.5, 0.5]`.
    pub fn elevation_bounds_m(&self) -> ElevationBounds {
        let crater_depth_m = self.config.crater_count as f64 * 1_600.0;
        let crater_rim_m = self.config.crater_count as f64 * 480.0;
        ElevationBounds::new(
            -1.3 * self.config.rolling_amplitude_m - crater_depth_m,
            1.8 * self.config.rolling_amplitude_m + self.config.mountain_amplitude_m + crater_rim_m,
        )
    }

    fn crater_field(&self, lat: f64, lon: f64) -> f64 {
        if self.config.crater_count == 0 {
            return 0.0;
        }
        let mut total = 0.0;
        for i in 0..self.config.crater_count {
            let s = self
                .config
                .seed
                .wrapping_add(0xC3A5_C85C_97CB_3127)
                .wrapping_add(i as u64);
            let lat_c = self.noise.cell3(s, i as i64, 1, 0) * 180.0 - 90.0;
            let lon_c = self.noise.cell3(s, i as i64, 2, 0) * 360.0 - 180.0;
            let radius_deg = 0.5 + self.noise.cell3(s, i as i64, 3, 0) * 4.0;
            let depth_m = 100.0 + self.noise.cell3(s, i as i64, 4, 0) * 1_500.0;
            total += Self::crater_height(lat, lon, lat_c, lon_c, radius_deg, depth_m);
        }
        total
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
        // A bounded lip rises inside the outer band and returns to zero at the
        // crater boundary, preserving height and normal continuity outside it.
        let rim = ss(0.7, 0.85, t) * (1.0 - ss(0.85, 1.0, t)) * depth_m * 0.8;
        bowl + rim
    }

    pub(crate) fn direction(latitude_deg: f64, longitude_deg: f64) -> DVec3 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin())
    }

    fn warped_coordinates(&self, direction: DVec3) -> DVec3 {
        let base = direction * NOISE_SCALE;
        let warp = |seed| self.noise.fbm(seed, base.x, base.y, base.z, 2) - 0.5;
        DVec3::new(
            base.x + warp(self.config.seed ^ SEED_WARP_X) * WARP_STRENGTH,
            base.y + warp(self.config.seed ^ SEED_WARP_Y) * WARP_STRENGTH,
            base.z + warp(self.config.seed ^ SEED_WARP_Z) * WARP_STRENGTH,
        )
    }

    pub(crate) fn continental_mask(&self, direction: DVec3) -> f64 {
        let continental_fbm = self.noise.fbm(
            self.config.seed ^ SEED_CONTINENTS,
            direction.x * CONTINENTAL_SCALE,
            direction.y * CONTINENTAL_SCALE,
            direction.z * CONTINENTAL_SCALE,
            3,
        );
        ss(0.43, 0.62, continental_fbm)
    }

    pub(crate) fn mountain_region_mask(&self, direction: DVec3, continental_mask: f64) -> f64 {
        let orogeny_fbm = self.noise.fbm(
            self.config.seed ^ SEED_OROGENY,
            direction.x * OROGENY_SCALE,
            direction.y * OROGENY_SCALE,
            direction.z * OROGENY_SCALE,
            2,
        );
        // The continental mask begins below mean sea level. Gate orogeny on
        // established land so ridges cannot raise the ocean floor into visible
        // mountain chains before the continental shelf emerges.
        ss(0.52, 0.68, continental_mask) * ss(0.48, 0.66, orogeny_fbm)
    }
}

impl TerrainSource for ProceduralTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let direction = Self::direction(latitude_deg, longitude_deg);
        let p = self.warped_coordinates(direction);
        let continental_mask = self.continental_mask(direction);
        let mountain_region_mask = self.mountain_region_mask(direction, continental_mask);

        let continental_elevation = continental_curve(continental_mask)
            * CONTINENTAL_AMPLITUDE
            * self.config.rolling_amplitude_m;
        let rolling01 = self
            .noise
            .fbm(self.config.seed, p.x, p.y, p.z, 4)
            .clamp(0.0, 1.0);
        let rolling = rolling01.powf(SHAPE_POWER) - 0.5;
        let hills = rolling * ROLLING_AMPLITUDE * self.config.rolling_amplitude_m;
        let ridges =
            self.noise
                .ridged_noise_attenuated(self.config.seed.wrapping_add(7), p.x, p.y, p.z, 5);
        let mountains = ridges * mountain_region_mask * self.config.mountain_amplitude_m;
        let mut h = continental_elevation + hills + mountains;
        if self.config.crater_count > 0 {
            h += self.crater_field(latitude_deg, longitude_deg);
        }
        h
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        ProceduralTerrainSource::elevation_bounds_m(self)
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let p = Self::direction(latitude_deg, longitude_deg) * NOISE_SCALE;
        self.noise
            .fbm(self.config.seed ^ SEED_MOISTURE, p.x, p.y, p.z, 3)
            .clamp(0.0, 1.0)
    }

    /// Land cover for a fully procedural planet: moisture, thinned toward the
    /// cold poles and above the treeline so scatter follows the climate rather
    /// than the rendered color.
    fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let moisture = self.moisture(latitude_deg, longitude_deg);
        let cold = 1.0 - ss(45.0, 72.0, latitude_deg.abs());
        let altitude = 1.0 - ss(3_000.0, 4_500.0, self.height_m(latitude_deg, longitude_deg));
        (moisture * cold * altitude).clamp(0.0, 1.0)
    }
}

/// A graded flat zone around a launch site, in the terrain-radial
/// latitude/longitude convention used by [`TerrainSource`]. Detail is fully
/// suppressed inside `flat_radius_m` and restored smoothly by `blend_radius_m`,
/// so a vehicle always stands on a level prepared pad instead of a synthetic
/// 250 m ridge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadFlatZone {
    direction: DVec3,
    flat_cos: f64,
    blend_cos: f64,
}

impl PadFlatZone {
    /// Build a zone from terrain-radial coordinates. Radii are great-circle
    /// distances on a sphere of `reference_radius_m`.
    pub fn new(
        latitude_deg: f64,
        longitude_deg: f64,
        reference_radius_m: f64,
        flat_radius_m: f64,
        blend_radius_m: f64,
    ) -> Self {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        let reference_radius_m = reference_radius_m.max(1.0);
        let flat_angle = (flat_radius_m / reference_radius_m).clamp(0.0, std::f64::consts::PI);
        let blend_angle =
            (blend_radius_m / reference_radius_m).clamp(flat_angle, std::f64::consts::PI);
        Self {
            direction,
            flat_cos: flat_angle.cos(),
            blend_cos: blend_angle.cos(),
        }
    }

    /// Terrain-radial latitude/longitude of the zone centre, so a caller that
    /// needs the site's own composed height can sample it.
    pub(crate) fn terrain_lat_lon(&self) -> (f64, f64) {
        (
            self.direction.y.clamp(-1.0, 1.0).asin().to_degrees(),
            self.direction.z.atan2(self.direction.x).to_degrees(),
        )
    }

    /// `0` inside the graded pad, `1` beyond the blend, smooth in between.
    pub(crate) fn attenuation(&self, direction: DVec3) -> f64 {
        let cos = direction.dot(self.direction).clamp(-1.0, 1.0);
        if cos >= self.flat_cos {
            return 0.0;
        }
        if cos <= self.blend_cos {
            return 1.0;
        }
        let span = self.flat_cos - self.blend_cos;
        let t = ((self.flat_cos - cos) / span).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Half-width of the standard Earth pad flame trench on the pad's local +X
/// (east) axis, metres.
pub const LAUNCH_TRENCH_HALF_WIDTH_M: f64 = 7.0;
/// Near edge of the trench on the pad's local +Z (north) axis, metres.
///
/// It must clear the launch point by more than `surface_normal`'s 5 m
/// finite-difference footprint plus the blend, or the terrain normal under the
/// vehicle is measured across the excavation edge and the pad reads as a slope.
/// 9 m leaves 1.5 m of margin and also clears the hold-down clamps, so the
/// resting height, the launch attitude and every flight baseline are unchanged.
pub const LAUNCH_TRENCH_NEAR_Z_M: f64 = 9.0;
/// Far edge of the trench on the pad's local +Z axis, metres.
pub const LAUNCH_TRENCH_FAR_Z_M: f64 = 28.0;
/// Excavation depth below the levelled pad platform, metres.
pub const LAUNCH_TRENCH_DEPTH_M: f64 = 5.0;
/// Horizontal distance over which the excavation returns to the surrounding
/// grade, metres.
pub const LAUNCH_TRENCH_BLEND_M: f64 = 2.5;

/// Half-extent of the concrete hardstand under the launch facility, metres.
///
/// The hardstand is a structure, not terrain: it carries the flame-trench
/// opening, and the terrain mesh is cut away beneath it so the below-grade
/// trench can be seen at all. It must span
/// [`LAUNCH_HARDSTAND_CUTOUT_RADIUS_M`] with margin, or the cut shows as a
/// see-through hole in the ground.
pub const LAUNCH_HARDSTAND_HALF_EXTENT_M: f64 = 55.0;

/// Radius of the terrain-mesh cutout under the hardstand, metres.
///
/// A mesh cell is removed only when all four of its corners lie inside this cap,
/// so the hole is always a subset of it and the hardstand always closes it. The
/// same rule makes the *guaranteed* hole smaller than the cap: a cell whose
/// centre is within `radius - diagonal / 2` has every corner inside, so the hole
/// always covers that disc. The finest streamed Earth patch is 610.8 m across at
/// 33 samples, i.e. 19.09 m cells with a 27 m diagonal, which leaves 36.5 m of
/// guaranteed opening — enough to expose the trench, whose far edge is 28 m from
/// the pad centre.
pub const LAUNCH_HARDSTAND_CUTOUT_RADIUS_M: f64 = 50.0;

/// A rectangular excavation cut into a graded launch site, expressed in the
/// site's local tangent plane (`+x` east, `+z` north).
///
/// A launch pad is a construction, and its flame trench is part of that
/// construction rather than a terrain sample. The cut is applied *after* the
/// site grade, so it is always exactly `depth_m` below the levelled platform
/// and the rendered mesh, collision and radar altitude all see the same
/// excavation (AGENTS.md sections 9 and 20).
///
/// The near edge deliberately stays outside the vehicle's launch point: the
/// pad has no collision surface of its own, so the ground under the vehicle
/// must remain at grade or the resting height would move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadTrenchZone {
    direction: DVec3,
    east: DVec3,
    north: DVec3,
    reference_radius_m: f64,
    min_x_m: f64,
    max_x_m: f64,
    min_z_m: f64,
    max_z_m: f64,
    depth_m: f64,
    blend_m: f64,
}

impl PadTrenchZone {
    /// Build a zone from terrain-radial coordinates. Offsets are chord lengths
    /// on a sphere of `reference_radius_m`; for a pad-scale rectangle the
    /// chord/arc difference is far below the excavation's own tolerance.
    pub fn new(
        latitude_deg: f64,
        longitude_deg: f64,
        reference_radius_m: f64,
        x_range_m: (f64, f64),
        z_range_m: (f64, f64),
        depth_m: f64,
        blend_m: f64,
    ) -> Self {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        // The terrain frame's polar axis is +Y, so the local north tangent is
        // the polar axis with its radial component removed; east completes the
        // right-handed (east, up, north) pad triad.
        let polar = DVec3::new(0.0, 1.0, 0.0);
        let north = (polar - direction * polar.dot(direction)).normalize_or_zero();
        let north = if north.length_squared() > 0.5 {
            north
        } else {
            DVec3::X
        };
        let east = direction.cross(north).normalize_or_zero();
        Self {
            direction,
            east,
            north,
            reference_radius_m: reference_radius_m.max(1.0),
            min_x_m: x_range_m.0.min(x_range_m.1),
            max_x_m: x_range_m.0.max(x_range_m.1),
            min_z_m: z_range_m.0.min(z_range_m.1),
            max_z_m: z_range_m.0.max(z_range_m.1),
            depth_m: depth_m.max(0.0),
            blend_m: blend_m.max(0.0),
        }
    }

    /// Excavation depth removed at `direction`, metres. Fully `depth_m` inside
    /// the rectangle, zero beyond the blend, smooth in between.
    pub(crate) fn cut_m(&self, direction: DVec3) -> f64 {
        if self.depth_m <= 0.0 {
            return 0.0;
        }
        let relative = (direction - self.direction) * self.reference_radius_m;
        let x = relative.dot(self.east);
        let z = relative.dot(self.north);
        let outside_x = (self.min_x_m - x).max(x - self.max_x_m).max(0.0);
        let outside_z = (self.min_z_m - z).max(z - self.max_z_m).max(0.0);
        let outside = outside_x.hypot(outside_z);
        if outside <= 0.0 {
            return self.depth_m;
        }
        if self.blend_m <= 0.0 || outside >= self.blend_m {
            return 0.0;
        }
        let t = outside / self.blend_m;
        self.depth_m * (1.0 - t * t * (3.0 - 2.0 * t))
    }

    /// Depth of the excavation, metres.
    pub fn depth_m(&self) -> f64 {
        self.depth_m
    }
}

/// Deterministic bounded procedural detail contribution at Earth's surface:
/// ~250 m ridges plus ~100 m drainage-like troughs, sampled as seeded 3D value
/// noise on the unit sphere so it stays continuous across longitude and
/// cube-sphere face boundaries. Optional [`PadFlatZone`]s grade launch sites
/// level.
///
/// This is a *contribution* source: [`TerrainSource::height_m`] returns only the
/// detail offset relative to whatever base elevation the composition supplies,
/// so it can be installed as a [`TerrainDetailLayer`] over measured terrain.
/// Collision, mesh generation, and altitude all consume the same summed surface.
#[derive(Debug, Clone)]
pub struct ProceduralDetailSource {
    seed: u64,
    noise: ValueNoise,
    flat_zones: Vec<PadFlatZone>,
}

impl ProceduralDetailSource {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            noise: ValueNoise,
            flat_zones: Vec::new(),
        }
    }

    /// Detail source that grades the given launch sites level.
    pub fn with_flat_zones(seed: u64, flat_zones: Vec<PadFlatZone>) -> Self {
        Self {
            seed,
            noise: ValueNoise,
            flat_zones,
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
        let drainage = self.drainage_strength_for_direction(direction);
        let raw = ridges * 48.0 - drainage * 12.0;
        let attenuation = self
            .flat_zones
            .iter()
            .fold(1.0_f64, |acc, zone| acc.min(zone.attenuation(direction)));
        (raw * attenuation).clamp(
            Self::elevation_bounds_m().min_m,
            Self::elevation_bounds_m().max_m,
        )
    }

    fn drainage_strength_for_direction(&self, direction: DVec3) -> f64 {
        let drainage_noise = self.noise.value_noise3(
            self.seed ^ 0xD2A1_6A6E,
            direction.x * 60_000.0,
            direction.y * 60_000.0,
            direction.z * 60_000.0,
        );
        (1.0 - (drainage_noise * 2.0 - 1.0).abs()).powi(3)
    }
}

/// Bounded landscape-scale relief for the far mesh band (TM-02).
///
/// The fine detail octave is ~250 m and cannot be represented below LOD 11
/// without aliasing, so distant ground was flat. This source carries a ~4 km
/// wobble with the same deterministic noise and launch-site attenuation; it is
/// added only to the rendered mesh, on a band of LODs where a few kilometres is
/// representable.
#[derive(Debug)]
pub struct ProceduralFarDetailSource {
    seed: u64,
    noise: ValueNoise,
    flat_zones: Vec<PadFlatZone>,
}

impl ProceduralFarDetailSource {
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            noise: ValueNoise,
            flat_zones: Vec::new(),
        }
    }

    pub fn with_flat_zones(seed: u64, flat_zones: Vec<PadFlatZone>) -> Self {
        Self {
            seed,
            noise: ValueNoise,
            flat_zones,
        }
    }

    pub const fn elevation_bounds_m() -> ElevationBounds {
        ElevationBounds {
            min_m: -30.0,
            max_m: 30.0,
        }
    }

    fn detail_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        // ~4 km features at Earth's radius.
        let ridges = self.noise.ridged_noise(
            self.seed,
            direction.x * 1_600.0,
            direction.y * 1_600.0,
            direction.z * 1_600.0,
            3,
        ) - 0.5;
        let attenuation = self
            .flat_zones
            .iter()
            .fold(1.0_f64, |acc, zone| acc.min(zone.attenuation(direction)));
        (ridges * 60.0 * attenuation).clamp(
            Self::elevation_bounds_m().min_m,
            Self::elevation_bounds_m().max_m,
        )
    }
}

impl TerrainSource for ProceduralFarDetailSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.detail_m(latitude_deg, longitude_deg)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        Self::elevation_bounds_m()
    }

    fn patch_geometric_error(&self, _patch: &TerrainPatch) -> PatchGeometricError {
        let bounds = Self::elevation_bounds_m();
        PatchGeometricError::from_elevation_bounds(bounds.min_m, bounds.max_m)
    }
}

impl TerrainSource for ProceduralDetailSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.detail_m(latitude_deg, longitude_deg)
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        Self::elevation_bounds_m()
    }

    fn patch_geometric_error(&self, _patch: &TerrainPatch) -> PatchGeometricError {
        let bounds = Self::elevation_bounds_m();
        PatchGeometricError::from_elevation_bounds(bounds.min_m, bounds.max_m)
    }

    /// Landscape-scale wetness for the continuous biome/material law. Sampling a
    /// low-frequency fractal field (instead of the ~200 m drainage statistic)
    /// keeps the wetness resolvable by the per-patch weight map at every LOD,
    /// so the material ecotone does not alias into a LOD-dependent seam.
    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        let p = direction * MOISTURE_NOISE_SCALE;
        self.noise
            .fbm(self.seed ^ 0xD2A1_6A6E, p.x, p.y, p.z, 3)
            .clamp(0.0, 1.0)
    }

    // The detail noise is a moisture/trough signal, not accumulated flow.
    // Inherit zero river strength: only the composed hydrology layer can
    // designate standing channel water, otherwise every noise ridge floods.

    /// Broad climate cover for the procedural detail layer: humid tropics, dry
    /// subtropical belts, temperate and boreal forest, modulated by regional
    /// noise and biased greener along drainage. Presentation-only land cover,
    /// not measured vegetation data.
    fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let lat = latitude_deg.to_radians();
        let lon = longitude_deg.to_radians();
        let direction = DVec3::new(lat.cos() * lon.cos(), lat.sin(), lat.cos() * lon.sin());
        let abs_lat = latitude_deg.abs();
        let tropics = 1.0 - ss(12.0, 32.0, abs_lat);
        let subtropical_dry = ss(14.0, 26.0, abs_lat) * (1.0 - ss(34.0, 48.0, abs_lat));
        let temperate = ss(34.0, 50.0, abs_lat) * (1.0 - ss(58.0, 70.0, abs_lat));
        let boreal = ss(52.0, 64.0, abs_lat) * (1.0 - ss(66.0, 74.0, abs_lat));
        let base = (tropics * 0.95 + temperate * 0.8 + boreal * 0.55 - subtropical_dry * 0.5)
            .clamp(0.0, 1.0);
        let regional = self.noise.value_noise3(
            self.seed ^ 0x5EED_1EAF,
            direction.x * 9_000.0,
            direction.y * 9_000.0,
            direction.z * 9_000.0,
        );
        let drainage = self.drainage_strength_for_direction(direction);
        (base * (0.5 + 0.5 * regional) + drainage * 0.35).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continental_curve_preserves_the_ocean_and_reshapes_land() {
        // The ocean segment is exactly the previous linear basin, so coastlines
        // and launch-site land/ocean classification do not shift.
        assert!((continental_curve(0.30) - (0.30 - COAST_MASK)).abs() < 1e-12);
        // Coastal lowland is flatter than the old linear slope...
        let old_lowland = 0.60 - COAST_MASK;
        assert!(
            continental_curve(0.60) < old_lowland,
            "lowland must be flatter than the old linear form"
        );
        // ...and the alpine ramp is steeper near the peaks.
        let old_alpine = 0.95 - COAST_MASK;
        assert!(
            continental_curve(0.95) > old_alpine,
            "alpine ramp must be steeper than the old linear form"
        );
    }

    #[test]
    fn continental_curve_is_continuous_at_every_join() {
        let mut previous = continental_curve(0.0);
        for step in 1..=1000 {
            let mask = step as f64 / 1000.0;
            let current = continental_curve(mask);
            assert!(
                (current - previous).abs() < 0.02,
                "curve jumped at mask {mask}: {previous} -> {current}"
            );
            previous = current;
        }
    }

    /// LF-03: the excavation is full depth inside its rectangle, zero beyond the
    /// blend, and never cuts the launch point.
    #[test]
    fn pad_trench_cut_is_full_inside_and_blends_to_grade() {
        const RADIUS_M: f64 = 6_371_000.0;
        let zone = PadTrenchZone::new(0.0, 0.0, RADIUS_M, (-7.0, 7.0), (4.5, 28.0), 5.0, 2.5);
        // (east, north) offsets in metres from the site centre, as a terrain
        // direction in the same convention `height_m` samples.
        let at = |east_m: f64, north_m: f64| DVec3::new(RADIUS_M, north_m, east_m).normalize();
        assert!(
            zone.cut_m(at(0.0, 0.0)).abs() < 1e-9,
            "the launch point must never be excavated"
        );
        assert!((zone.cut_m(at(0.0, 10.0)) - 5.0).abs() < 1e-9);
        assert!((zone.cut_m(at(6.0, 20.0)) - 5.0).abs() < 1e-9);
        let mouth = zone.cut_m(at(0.0, 3.5));
        assert!(
            mouth > 0.0 && mouth < 5.0,
            "the mouth must blend instead of stepping, got {mouth}"
        );
        assert_eq!(
            zone.cut_m(at(10.0, 20.0)),
            0.0,
            "beyond the blend the surrounding grade is untouched"
        );
        assert!((zone.depth_m() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn self_attenuating_ridge_stays_bounded_and_deterministic() {
        let noise = ValueNoise;
        let a = noise.ridged_noise_attenuated(7, 1.3, -2.4, 0.9, 5);
        let b = noise.ridged_noise_attenuated(7, 1.3, -2.4, 0.9, 5);
        assert_eq!(a, b, "self-attenuated ridge must be deterministic");
        assert!((0.0..=1.0).contains(&a), "ridge out of range: {a}");
    }
}
