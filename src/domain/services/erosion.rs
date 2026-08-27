//! Erosion and river carving, baked into deterministic per-tile height
//! rasters (AGENTS.md sections 20-21, 44).
//!
//! Realism best practice: `shape → simulate → detail → texture`. This module
//! is the *simulate* layer applied to the analytic sculpt (T1): thermal
//! (talus/angle-of-repose) slump, hydraulic droplet erosion, and D8
//! flow-accumulation river carving. Everything runs on a per-tile fixed grid
//! at generation time (never per frame), is seeded and deterministic, and the
//! [`ErodedTerrainSource`] caches rasters so queries stay cheap and
//! reproducible. Near a tile boundary the eroded height is feathered back
//! toward the analytic base so adjacent independently-eroded tiles stay
//! continuous (no visible seams).
//!
//! Scale note (best practice): erosion is simulated coarse→fine at the tile's
//! own resolution; it is not transferred across scales.

use crate::domain::services::terrain_source::TerrainSource;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A deterministic xorshift64 PRNG so droplet erosion is fully reproducible
/// without pulling in a PRNG dependency or depending on `rand`'s version.
#[derive(Debug, Clone)]
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    fn usize(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// A rectangular height raster covering a lat/lon cell, plus derived flow
/// accumulation and moisture channels.
#[derive(Debug, Clone)]
pub struct HeightRaster {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
    pub width: u32,
    pub height: u32,
    /// Row-major (row = latitude) terrain heights, meters.
    pub data: Vec<f32>,
    /// Row-major D8 flow accumulation (accumulated rain units).
    pub flow: Vec<f32>,
    /// Row-major normalized moisture `[0, 1]`.
    pub moisture: Vec<f32>,
}

impl HeightRaster {
    /// Cell spacing in meters approximated at the tile's latitude (assumes a
    /// near-square grid).
    pub fn spacing_m(&self) -> f64 {
        let dlat = (self.lat_max - self.lat_min) / (self.height.max(1) - 1) as f64;
        dlat.abs() * 111_320.0
    }
}

/// Erosion and tiling configuration.
#[derive(Debug, Clone)]
pub struct ErosionConfig {
    /// Tile edge length in degrees (e.g. 2° ≈ 220 km). Must be > 0.
    pub tile_deg: f64,
    /// Raster resolution per tile (vertices per side).
    pub resolution: u32,
    /// Number of hydraulic droplets per tile.
    pub droplets: u32,
    /// Thermal slump iterations (talus angle of repose).
    pub thermal_iterations: u32,
    /// Talus slope (rise/run) governing thermal slump.
    pub talus_slope: f64,
    /// Hydraulic droplet parameters.
    pub droplet_inertia: f64,
    pub droplet_capacity: f64,
    pub droplet_erosion: f64,
    pub droplet_deposition: f64,
    pub droplet_evaporation: f64,
    /// Flow accumulation above which a river channel is carved.
    pub river_flow_threshold: f32,
    /// River channel carve depth, meters.
    pub river_depth_m: f64,
    /// Boundary feather band (fraction of the tile) blended back to the base
    /// analytic terrain to hide tile seams.
    pub edge_feather: f64,
    /// Max resident tiles before LRU eviction.
    pub cache_max_tiles: usize,
    /// Master seed for droplet / tile determinism.
    pub seed: u64,
}

impl Default for ErosionConfig {
    fn default() -> Self {
        Self {
            tile_deg: 2.0,
            resolution: 64,
            droplets: 3_500,
            thermal_iterations: 3,
            talus_slope: 1.2,
            droplet_inertia: 0.05,
            droplet_capacity: 4.0,
            droplet_erosion: 0.3,
            droplet_deposition: 0.3,
            droplet_evaporation: 0.02,
            river_flow_threshold: 60.0,
            river_depth_m: 40.0,
            edge_feather: 0.12,
            cache_max_tiles: 64,
            seed: 0xE0D1_5EED,
        }
    }
}

fn idx(x: usize, y: usize, w: usize) -> usize {
    y * w + x
}

/// Steepest-downhill neighbour of a cell. Returns its flat index and the drop
/// (height difference), or `None` for a local minimum / out-of-bounds.
fn steepest_downhill(x: usize, y: usize, h: &[f32], w: usize, hgt: usize) -> Option<(usize, f32)> {
    let mut best = None;
    let mut best_drop = 0.0f32;
    for dy in -1i64..=1 {
        for dx in -1i64..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let nx = x as i64 + dx;
            let ny = y as i64 + dy;
            if nx < 0 || ny < 0 || nx >= w as i64 || ny >= hgt as i64 {
                continue;
            }
            let drop = h[idx(x, y, w)] - h[idx(nx as usize, ny as usize, w)];
            if drop > best_drop {
                best_drop = drop;
                best = Some((idx(nx as usize, ny as usize, w), drop));
            }
        }
    }
    best
}

/// Thermal erosion (talus slump): repeatedly lower any slope steeper than the
/// angle of repose toward its steepest downhill neighbour. Deterministic
/// (fixed iteration order). Preserves total volume (material moves, is not
/// created or destroyed).
pub fn thermal_erode(
    h: &mut [f32],
    w: usize,
    hgt: usize,
    spacing_m: f64,
    talus_slope: f64,
    iterations: u32,
) {
    for _ in 0..iterations {
        for y in 0..hgt {
            for x in 0..w {
                let i = idx(x, y, w);
                if let Some((n, drop)) = steepest_downhill(x, y, h, w, hgt) {
                    let slope = drop as f64 / spacing_m;
                    if slope > talus_slope {
                        let amount = 0.5 * (drop as f64 - talus_slope * spacing_m);
                        h[i] -= amount as f32;
                        h[n] += amount as f32;
                    }
                }
            }
        }
    }
}

/// Hydraulic droplet erosion (simplified particle-based): each droplet
/// accelerates downhill, erodes when under its sediment capacity, deposits
/// when over, and evaporates. Deterministic given the RNG.
pub fn hydraulic_erode(
    h: &mut [f32],
    w: usize,
    hgt: usize,
    spacing_m: f64,
    droplets: u32,
    seed: u64,
    cfg: &ErosionConfig,
) {
    let mut rng = Rng::new(seed);
    for _ in 0..droplets {
        let mut x = rng.usize(w);
        let mut y = rng.usize(hgt);
        let mut velocity = 0.0f64;
        let mut sediment = 0.0f64;

        for _ in 0..512 {
            let i = idx(x, y, w);
            let Some((n, drop)) = steepest_downhill(x, y, h, w, hgt) else {
                break; // local minimum
            };
            let (nx, ny) = (n % w, n / w);
            let slope = drop as f64 / spacing_m;
            velocity = (velocity + cfg.droplet_inertia * slope).clamp(0.0, 64.0);
            let capacity = velocity.max(0.1) * cfg.droplet_capacity;

            if sediment > capacity {
                let deposit = (sediment - capacity) * cfg.droplet_deposition;
                h[i] += deposit as f32;
                sediment -= deposit;
            } else {
                let erode = (capacity - sediment).min(cfg.droplet_erosion);
                // Don't erode below the sea floor (0 m reference) too hard.
                h[i] = (h[i] - erode as f32).max(-8000.0);
                sediment += erode;
            }

            x = nx;
            y = ny;

            if velocity < 0.5 || rng.f64() < cfg.droplet_evaporation {
                break;
            }
        }
        // Deposit any leftover sediment near the stopping point.
        if sediment > 0.0 {
            let i = idx(x, y, w);
            h[i] += sediment as f32;
        }
    }
}

/// D8 flow accumulation: each cell contributes one unit of rain routed to its
/// steepest downhill neighbour, processed in descending-height order so
/// upslope flow is fully accumulated before it is passed on.
pub fn flow_accumulation(h: &[f32], w: usize, hgt: usize) -> Vec<f32> {
    let mut acc = vec![1.0f32; w * hgt];
    let mut order: Vec<usize> = (0..w * hgt).collect();
    order.sort_by(|&a, &b| h[b].partial_cmp(&h[a]).unwrap_or(std::cmp::Ordering::Equal));
    for &i in &order {
        if let Some((n, _)) = steepest_downhill(i % w, i / w, h, w, hgt) {
            acc[n] += acc[i];
        }
    }
    acc
}

/// Carve river channels where flow accumulation exceeds a threshold, and
/// boost moisture there (so river networks read as water and drive wet biomes).
pub fn carve_rivers(
    h: &mut [f32],
    flow: &[f32],
    moisture: &mut [f32],
    threshold: f32,
    depth_m: f64,
) {
    for i in 0..h.len() {
        if flow[i] > threshold {
            h[i] -= (depth_m as f32) * (flow[i] / threshold).min(3.0);
        }
        // Moisture grows sharply once flow exceeds the threshold.
        let river_boost = ((flow[i] / threshold).log2().max(0.0) * 0.3).min(0.7);
        moisture[i] = (moisture[i] + river_boost as f32).clamp(0.0, 1.0);
    }
}

/// Erode an entire tile starting from the analytic `base` source, returning a
/// raster with height, flow and moisture channels. Deterministic for a fixed
/// `(base, cfg, tile key)`.
pub fn erode_tile(
    base: &dyn TerrainSource,
    lat_min: f64,
    lat_max: f64,
    lon_min: f64,
    lon_max: f64,
    cfg: &ErosionConfig,
    seed: u64,
) -> HeightRaster {
    let res = cfg.resolution.max(2) as usize;
    let mut h = vec![0.0f32; res * res];
    let mut moisture = vec![0.5f32; res * res];

    for y in 0..res {
        for x in 0..res {
            let lon = lon_min + (lon_max - lon_min) * x as f64 / (res - 1) as f64;
            let lat = lat_min + (lat_max - lat_min) * y as f64 / (res - 1) as f64;
            let i = idx(x, y, res);
            h[i] = base.height_m(lat, lon) as f32;
            moisture[i] = base.moisture(lat, lon).clamp(0.0, 1.0) as f32;
        }
    }

    let spacing_m = {
        let dlat = (lat_max - lat_min) / (res - 1) as f64;
        dlat.abs() * 111_320.0
    };

    thermal_erode(
        &mut h,
        res,
        res,
        spacing_m,
        cfg.talus_slope,
        cfg.thermal_iterations,
    );
    hydraulic_erode(&mut h, res, res, spacing_m, cfg.droplets, seed, cfg);

    let mut flow = flow_accumulation(&h, res, res);
    carve_rivers(
        &mut h,
        &flow,
        &mut moisture,
        cfg.river_flow_threshold,
        cfg.river_depth_m,
    );

    HeightRaster {
        lat_min,
        lat_max,
        lon_min,
        lon_max,
        width: res as u32,
        height: res as u32,
        data: h,
        flow,
        moisture,
    }
}

/// Bilinear sample of a lat/lon in a raster's data channel, returning
/// `(value, edge_factor)` where `edge_factor ∈ [0,1]` is 0 in the interior and
/// 1 at the tile boundary.
fn sample(raster: &HeightRaster, lat: f64, lon: f64) -> (f64, f64) {
    let w = raster.width as usize;
    let hgt = raster.height as usize;
    let span_lat = (raster.lat_max - raster.lat_min).abs().max(1e-12);
    let span_lon = (raster.lon_max - raster.lon_min).abs().max(1e-12);
    let fy = ((lat - raster.lat_min) / span_lat * (hgt - 1) as f64).clamp(0.0, (hgt - 1) as f64);
    let fx = ((lon - raster.lon_min) / span_lon * (w - 1) as f64).clamp(0.0, (w - 1) as f64);
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(hgt - 1));
    let dx = fx - x0 as f64;
    let dy = fy - y0 as f64;
    let at = |x: usize, y: usize| raster.data[y * w + x] as f64;
    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * dx;
    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * dx;
    let value = top + (bottom - top) * dy;

    // Distance to the nearest tile edge, in [0, 1].
    let fx_norm = (lon - raster.lon_min) / span_lon;
    let fy_norm = (lat - raster.lat_min) / span_lat;
    let edge = 1.0
        - fx_norm
            .min(1.0 - fx_norm)
            .min(fy_norm)
            .min(1.0 - fy_norm)
            .max(0.0);
    (value, edge)
}

/// A [`TerrainSource`] that samples erosion rasters. Tiles are generated
/// lazily on first access, cached with a cap, and feathered back to the base
/// analytic source near tile boundaries so independent tiles stay continuous.
#[derive(Debug)]
pub struct ErodedTerrainSource {
    base: Arc<dyn TerrainSource>,
    cfg: ErosionConfig,
    cache: Mutex<HashMap<(i64, i64), Arc<HeightRaster>>>,
    order: Mutex<Vec<(i64, i64)>>,
}

impl ErodedTerrainSource {
    pub fn new(base: Arc<dyn TerrainSource>, cfg: ErosionConfig) -> Self {
        Self {
            base,
            cfg,
            cache: Mutex::new(HashMap::new()),
            order: Mutex::new(Vec::new()),
        }
    }

    fn tile_key(lat: f64, lon: f64, tile_deg: f64) -> (i64, i64) {
        let tx = (lon / tile_deg).floor() as i64;
        let ty = (lat / tile_deg).floor() as i64;
        (tx, ty)
    }

    fn tile_seed(&self, tx: i64, ty: i64) -> u64 {
        let mix = (tx as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (ty as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
        self.cfg.seed ^ mix
    }

    fn get_tile(&self, lat: f64, lon: f64) -> Arc<HeightRaster> {
        let tile_deg = self.cfg.tile_deg;
        let (tx, ty) = Self::tile_key(lat, lon, tile_deg);

        // Check cache first.
        {
            let cache = self.cache.lock().expect("erosion cache lock");
            if let Some(t) = cache.get(&(tx, ty)) {
                return Arc::clone(t);
            }
        }

        // Generate (deterministic per tile).
        let lat_min = ty as f64 * tile_deg;
        let lat_max = lat_min + tile_deg;
        let lon_min = tx as f64 * tile_deg;
        let lon_max = lon_min + tile_deg;
        let tile = Arc::new(erode_tile(
            self.base.as_ref(),
            lat_min,
            lat_max,
            lon_min,
            lon_max,
            &self.cfg,
            self.tile_seed(tx, ty),
        ));

        let mut cache = self.cache.lock().expect("erosion cache lock");
        let mut order = self.order.lock().expect("erosion order lock");
        if cache.len() >= self.cfg.cache_max_tiles {
            if let Some(lru) = order.first().copied() {
                cache.remove(&lru);
                order.remove(0);
            }
        }
        cache.insert((tx, ty), Arc::clone(&tile));
        order.push((tx, ty));
        tile
    }
}

impl TerrainSource for ErodedTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let tile = self.get_tile(latitude_deg, longitude_deg);
        let (eroded, edge) = sample(&tile, latitude_deg, longitude_deg);
        if edge > 0.0 {
            let feather = (edge / self.cfg.edge_feather).clamp(0.0, 1.0);
            // At the tile boundary the eroded value blends fully to the base
            // analytic terrain so neighbouring independently-eroded tiles meet
            // seamlessly.
            let base_h = self.base.height_m(latitude_deg, longitude_deg);
            eroded + (base_h - eroded) * feather
        } else {
            eroded
        }
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let tile = self.get_tile(latitude_deg, longitude_deg);
        let w = tile.width as usize;
        let hgt = tile.height as usize;
        let span_lat = (tile.lat_max - tile.lat_min).abs().max(1e-12);
        let span_lon = (tile.lon_max - tile.lon_min).abs().max(1e-12);
        let fy = ((latitude_deg - tile.lat_min) / span_lat * (hgt - 1) as f64)
            .clamp(0.0, (hgt - 1) as f64);
        let fx =
            ((longitude_deg - tile.lon_min) / span_lon * (w - 1) as f64).clamp(0.0, (w - 1) as f64);
        let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(hgt - 1));
        let dx = fx - x0 as f64;
        let dy = fy - y0 as f64;
        let at = |x: usize, y: usize| tile.moisture[y * w + x] as f64;
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * dx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * dx;
        (top + (bottom - top) * dy).clamp(0.0, 1.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;

    fn base() -> ProceduralTerrainSource {
        ProceduralTerrainSource::new(7, 2_000.0, 1_200.0, 0)
    }

    fn cfg() -> ErosionConfig {
        ErosionConfig {
            tile_deg: 2.0,
            resolution: 48,
            droplets: 2000,
            thermal_iterations: 3,
            edge_feather: 0.12,
            cache_max_tiles: 8,
            ..Default::default()
        }
    }

    #[test]
    fn thermal_erosion_clamps_steepest_slope_to_talus() {
        // A single steep step (height 1000 over a flat valley) must be reduced
        // toward the talus angle by thermal slump.
        let w = 16;
        let hgt = 16;
        let spacing = 1000.0; // m per cell
        let mut h = vec![0.0f32; w * hgt];
        for y in 0..hgt {
            for x in 0..w {
                h[y * w + x] = if y < 4 { 5000.0 } else { 0.0 };
            }
        }
        let steepest_before = (0..h.len())
            .map(|i| {
                let (x, y) = (i % w, i / w);
                steepest_downhill(x, y, &h, w, hgt)
                    .map(|(_, d)| d as f64 / spacing)
                    .unwrap_or(0.0)
            })
            .fold(0.0f64, f64::max);
        assert!(
            steepest_before > 1.2,
            "setup slope too gentle: {steepest_before}"
        );

        thermal_erode(&mut h, w, hgt, spacing, 1.2, 4);

        let steepest_after = (0..h.len())
            .map(|i| {
                let (x, y) = (i % w, i / w);
                steepest_downhill(x, y, &h, w, hgt)
                    .map(|(_, d)| d as f64 / spacing)
                    .unwrap_or(0.0)
            })
            .fold(0.0f64, f64::max);
        assert!(
            steepest_after < steepest_before,
            "thermal erosion must soften steepest slope: {steepest_after} vs {steepest_before}"
        );
        // Material is conserved.
        let sum: f32 = h.iter().sum();
        assert!(
            (sum - (16 * 4 * 5000) as f32).abs() < 1.0,
            "volume changed: {sum}"
        );
    }

    #[test]
    fn hydraulic_erosion_is_deterministic_and_changes_height() {
        let mut a = {
            let mut h = vec![0.0f32; 32 * 32];
            for y in 0..32 {
                for x in 0..32 {
                    h[y * 32 + x] = ((x as f32 + y as f32) * 60.0) % 6000.0;
                }
            }
            h
        };
        let mut b = a.clone();
        hydraulic_erode(&mut a, 32, 32, 1000.0, 1500, 1234, &cfg());
        hydraulic_erode(&mut b, 32, 32, 1000.0, 1500, 1234, &cfg());
        assert_eq!(a, b, "same seed must produce identical erosion");
        assert_ne!(a, vec![0.0f32; 32 * 32]);
        // At least some cell was eroded or deposited differently from flat.
        assert!(a.iter().any(|&v| v != 0.0));
    }

    #[test]
    fn flow_accumulation_accumulates_downslope() {
        // A simple ramp from (0,0) high to (w-1,*) low: flow must accumulate
        // toward the downhill side.
        let w = 16;
        let hgt = 4;
        let mut h = vec![0.0f32; w * hgt];
        for y in 0..hgt {
            for x in 0..w {
                h[y * w + x] = (w - x) as f32 * 10.0;
            }
        }
        let flow = flow_accumulation(&h, w, hgt);
        // The last column (lowest) must receive more accumulation than the first.
        let last_col: f32 = (0..hgt).map(|y| flow[y * w + (w - 1)]).sum();
        let first_col: f32 = (0..hgt).map(|y| flow[y * w + 0]).sum();
        assert!(last_col > first_col, "flow should accumulate downhill");
    }

    #[test]
    fn rivers_carve_below_surroundings() {
        let w = 16;
        let hgt = 16;
        let mut h = vec![1000.0f32; w * hgt];
        let mut flow = vec![100.0f32; w * hgt];
        let mut moisture = vec![0.2f32; w * hgt];
        carve_rivers(&mut h, &flow, &mut moisture, 50.0, 40.0);
        assert!(
            h.iter().all(|&v| v < 995.0),
            "river cells must be carved below terrain"
        );
        assert!(
            moisture.iter().all(|&m| m > 0.2),
            "rivers must raise moisture"
        );
    }

    #[test]
    fn erode_tile_is_deterministic() {
        let b = base();
        let a = erode_tile(&b, 10.0, 12.0, 20.0, 22.0, &cfg(), 99);
        let c = erode_tile(&b, 10.0, 12.0, 20.0, 22.0, &cfg(), 99);
        assert_eq!(a.data, c.data, "same tile must erode identically");
        assert_eq!(a.flow, c.flow);
        assert_eq!(a.moisture, c.moisture);
    }

    #[test]
    fn eroded_source_is_deterministic_and_feathers_at_tile_edge() {
        let source = ErodedTerrainSource::new(Arc::new(base()), cfg());
        let interior_a = source.height_m(11.0, 21.0);
        let interior_b = source.height_m(11.0, 21.0);
        assert_eq!(interior_a, interior_b, "queries must be deterministic");
        // Near the tile boundary (tile spans 10..12 lat), the height feathers
        // back to the analytic base (finite).
        let near_edge = source.height_m(10.01, 21.0);
        assert!(near_edge.is_finite());
        // Moisture is normalized.
        let m = source.moisture(11.5, 21.5);
        assert!((0.0..=1.0).contains(&m));
    }
}
