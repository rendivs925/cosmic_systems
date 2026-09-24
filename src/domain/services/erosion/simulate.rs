//! The erosion *simulate* layer: droplet/hydraulic and thermal erosion, D8
//! flow accumulation, river carving, and per-tile raster baking. Deterministic
//! and independent of frame rate.

use super::{ErosionConfig, HeightRaster};
use crate::domain::services::terrain_source::TerrainSource;
use std::cell::RefCell;

thread_local! {
    // Flow routing is temporary tile-build state. Reusing it per worker avoids
    // a fresh index-vector allocation for every erosion raster.
    static FLOW_ORDER_SCRATCH: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// A deterministic xorshift64 PRNG so droplet erosion is fully reproducible
/// without pulling in a PRNG dependency or depending on `rand`'s version.
#[derive(Debug, Clone)]
pub(super) struct Rng(u64);

impl Rng {
    pub(super) fn new(seed: u64) -> Self {
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

    pub(super) fn usize(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

pub(super) fn idx(x: usize, y: usize, w: usize) -> usize {
    y * w + x
}

/// Grid distances used for D8 routing. Longitude spacing is row-specific so
/// tiles retain sensible east-west slopes as they approach a pole.
#[derive(Debug, Clone)]
pub(super) struct GridSpacing {
    pub(super) north_south_m: f64,
    pub(super) east_west_m: Vec<f64>,
}

impl GridSpacing {
    pub(super) fn uniform(spacing_m: f64, rows: usize) -> Self {
        Self {
            north_south_m: spacing_m,
            east_west_m: vec![spacing_m; rows],
        }
    }

    pub(super) fn from_tile(
        lat_min: f64,
        lat_max: f64,
        lon_min: f64,
        lon_max: f64,
        rows: usize,
    ) -> Self {
        let row_count = rows.max(2);
        let north_south_m = ((lat_max - lat_min) / (row_count - 1) as f64).abs() * 111_320.0;
        let longitude_step_deg = ((lon_max - lon_min) / (row_count - 1) as f64).abs();
        let east_west_m = (0..rows)
            .map(|y| {
                let lat = lat_min + (lat_max - lat_min) * y as f64 / (row_count - 1) as f64;
                (longitude_step_deg * 111_320.0 * lat.to_radians().cos().abs()).max(1.0)
            })
            .collect();
        Self {
            north_south_m: north_south_m.max(1.0),
            east_west_m,
        }
    }

    fn neighbor_distance_m(&self, y: usize, ny: usize, dx: i64, dy: i64) -> f64 {
        let north_south_m = if dy == 0 { 0.0 } else { self.north_south_m };
        let east_west_m = if dx == 0 {
            0.0
        } else {
            (self.east_west_m[y] + self.east_west_m[ny]) * 0.5
        };
        north_south_m.hypot(east_west_m).max(f64::EPSILON)
    }
}

/// Steepest-downhill neighbour of a cell. Returns its flat index and the drop

#[cfg(test)]
pub(super) fn steepest_downhill(
    x: usize,
    y: usize,
    h: &[f32],
    w: usize,
    hgt: usize,
) -> Option<(usize, f32)> {
    let spacing = GridSpacing::uniform(1.0, hgt);
    steepest_downhill_with_spacing(x, y, h, w, hgt, &spacing).map(|(index, drop, _)| (index, drop))
}

/// Distance-aware D8 choice. A lower diagonal is not selected over a steeper
/// cardinal descent merely because its raw height drop is larger.
pub(super) fn steepest_downhill_with_spacing(
    x: usize,
    y: usize,
    h: &[f32],
    w: usize,
    hgt: usize,
    spacing: &GridSpacing,
) -> Option<(usize, f32, f64)> {
    let mut best = None;
    let mut best_slope = 0.0f64;
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
            let distance_m = spacing.neighbor_distance_m(y, ny as usize, dx, dy);
            let slope = f64::from(drop) / distance_m;
            if slope > best_slope {
                best_slope = slope;
                best = Some((idx(nx as usize, ny as usize, w), drop, distance_m));
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
    let spacing = GridSpacing::uniform(spacing_m, hgt);
    thermal_erode_with_spacing(h, w, hgt, &spacing, talus_slope, iterations);
}

fn thermal_erode_with_spacing(
    h: &mut [f32],
    w: usize,
    hgt: usize,
    spacing: &GridSpacing,
    talus_slope: f64,
    iterations: u32,
) {
    for _ in 0..iterations {
        for y in 0..hgt {
            for x in 0..w {
                let i = idx(x, y, w);
                if let Some((n, drop, distance_m)) =
                    steepest_downhill_with_spacing(x, y, h, w, hgt, spacing)
                {
                    let slope = f64::from(drop) / distance_m;
                    if slope > talus_slope {
                        let amount = 0.5 * (f64::from(drop) - talus_slope * distance_m);
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
    let spacing = GridSpacing::uniform(spacing_m, hgt);
    hydraulic_erode_with_spacing(h, w, hgt, &spacing, droplets, seed, cfg);
}

fn hydraulic_erode_with_spacing(
    h: &mut [f32],
    w: usize,
    hgt: usize,
    spacing: &GridSpacing,
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
            let Some((n, drop, distance_m)) =
                steepest_downhill_with_spacing(x, y, h, w, hgt, spacing)
            else {
                break; // local minimum
            };
            let (nx, ny) = (n % w, n / w);
            let slope = f64::from(drop) / distance_m;
            velocity = (velocity + cfg.droplet_inertia * slope).clamp(0.0, 64.0);
            let capacity = velocity.max(0.1) * cfg.droplet_capacity;

            if sediment > capacity {
                let deposit = (sediment - capacity) * cfg.droplet_deposition;
                h[i] += deposit as f32;
                sediment -= deposit;
            } else {
                let requested = (capacity - sediment).min(cfg.droplet_erosion);
                // The sediment load must match the material actually removed
                // after applying the -8000 m floor.
                let before = h[i];
                h[i] = (before - requested as f32).max(-8000.0);
                sediment += f64::from((before - h[i]).max(0.0));
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
    let spacing = GridSpacing::uniform(1.0, hgt);
    flow_accumulation_with_spacing(h, w, hgt, &spacing)
}

fn flow_accumulation_with_spacing(
    h: &[f32],
    w: usize,
    hgt: usize,
    spacing: &GridSpacing,
) -> Vec<f32> {
    let mut acc = vec![1.0f32; w * hgt];
    FLOW_ORDER_SCRATCH.with(|scratch| {
        let mut order = scratch.borrow_mut();
        order.clear();
        order.extend(0..w * hgt);
        order.sort_by(|&a, &b| h[b].partial_cmp(&h[a]).unwrap_or(std::cmp::Ordering::Equal));
        for &i in order.iter() {
            if let Some((n, _, _)) =
                steepest_downhill_with_spacing(i % w, i / w, h, w, hgt, spacing)
            {
                acc[n] += acc[i];
            }
        }
    });
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
        moisture[i] = (moisture[i] + river_boost).clamp(0.0, 1.0);
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
            let (lat, lon) = canonical_lat_lon(lat, lon);
            let i = idx(x, y, res);
            h[i] = base.height_m(lat, lon) as f32;
            moisture[i] = base.moisture(lat, lon).clamp(0.0, 1.0) as f32;
        }
    }

    let spacing = GridSpacing::from_tile(lat_min, lat_max, lon_min, lon_max, res);

    thermal_erode_with_spacing(
        &mut h,
        res,
        res,
        &spacing,
        cfg.talus_slope,
        cfg.thermal_iterations,
    );
    hydraulic_erode_with_spacing(&mut h, res, res, &spacing, cfg.droplets, seed, cfg);

    let flow = flow_accumulation_with_spacing(&h, res, res, &spacing);
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
pub(super) fn sample_channel(
    raster: &HeightRaster,
    values: &[f32],
    lat: f64,
    lon: f64,
) -> (f64, f64) {
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
    let at = |x: usize, y: usize| values[y * w + x] as f64;
    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * dx;
    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * dx;
    let value = top + (bottom - top) * dy;

    // Distance to the nearest tile edge, in [0, 1].
    let fx_norm = (lon - raster.lon_min) / span_lon;
    let fy_norm = (lat - raster.lat_min) / span_lat;
    let nearest_edge_distance = fx_norm
        .min(1.0 - fx_norm)
        .min(fy_norm)
        .min(1.0 - fy_norm)
        .max(0.0);
    // The nearest-edge distance reaches 0.5 at the tile center. Normalize it
    // so the feather is zero in the interior and one at every boundary.
    let edge = (1.0 - 2.0 * nearest_edge_distance).clamp(0.0, 1.0);
    (value, edge)
}

/// Map equivalent geographic coordinates onto one stable representation before
/// they reach the tile cache or its raster sampler. Longitude uses [-180, 180)
/// and latitude is reflected across either pole, with the corresponding
/// half-turn in longitude.
pub(super) fn canonical_lat_lon(latitude_deg: f64, longitude_deg: f64) -> (f64, f64) {
    assert!(
        latitude_deg.is_finite() && longitude_deg.is_finite(),
        "terrain latitude and longitude must be finite"
    );
    let latitude_phase = (latitude_deg + 90.0).rem_euclid(360.0);
    let (latitude_deg, longitude_deg) = if latitude_phase <= 180.0 {
        (latitude_phase - 90.0, longitude_deg)
    } else {
        (270.0 - latitude_phase, longitude_deg + 180.0)
    };
    (
        latitude_deg,
        (longitude_deg + 180.0).rem_euclid(360.0) - 180.0,
    )
}
