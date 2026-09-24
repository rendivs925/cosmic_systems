//! The cached, deterministic `ErodedTerrainSource`: bakes erosion tiles on
//! demand and implements the shared `TerrainSource` contract.

use super::simulate::{canonical_lat_lon, sample_channel};
use super::{erode_tile, ErosionConfig, HeightRaster};
use crate::domain::services::terrain_source::{ElevationBounds, TerrainSource};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};

pub(super) fn sample(raster: &HeightRaster, lat: f64, lon: f64) -> (f64, f64) {
    sample_channel(raster, &raster.data, lat, lon)
}

/// Return how much of an erosion raster contributes at an edge-factor value.
/// `sample` reports 0 at tile center and 1 at the boundary. The configured
/// feather is measured as a fraction of the full tile width, so the simulated
/// terrain occupies the interior and only blends to the analytic source near
/// the actual tile edge.
pub(super) fn erosion_weight(edge_factor: f64, edge_feather: f64) -> f64 {
    let distance_from_edge = (1.0 - edge_factor.clamp(0.0, 1.0)) * 0.5;
    let t = (distance_from_edge / edge_feather.max(f64::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// analytic source near tile boundaries so independent tiles stay continuous.
#[derive(Debug)]
pub struct ErodedTerrainSource {
    pub(super) base: Arc<dyn TerrainSource>,
    cfg: ErosionConfig,
    pub(super) cache: Mutex<TileCache>,
    in_flight: Mutex<HashMap<(i64, i64), Arc<TileBuild>>>,
}

/// One lock keeps LRU metadata and resident tiles coherent. The vector is the
/// complete recency order, so eviction never depends on hash-map iteration.
#[derive(Debug)]
pub(super) struct TileCache {
    pub(super) tiles: HashMap<(i64, i64), Arc<HeightRaster>>,
    order: Vec<(i64, i64)>,
}

impl TileCache {
    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self {
            tiles: HashMap::with_capacity(capacity),
            order: Vec::with_capacity(capacity),
        }
    }

    fn get(&mut self, key: (i64, i64)) -> Option<Arc<HeightRaster>> {
        let tile = self.tiles.get(&key).cloned();
        if tile.is_some() {
            self.touch(key);
        }
        tile
    }

    fn insert(&mut self, key: (i64, i64), tile: Arc<HeightRaster>, capacity: usize) {
        while self.tiles.len() >= capacity {
            let lru = self.order.remove(0);
            self.tiles.remove(&lru);
        }
        self.tiles.insert(key, tile);
        self.order.push(key);
    }

    fn touch(&mut self, key: (i64, i64)) {
        if let Some(index) = self.order.iter().position(|candidate| *candidate == key) {
            self.order.remove(index);
            self.order.push(key);
        }
    }
}

/// Shared completion state for one tile bake. It prevents concurrent geometry
/// jobs from simulating identical terrain while allowing distinct tiles to bake
/// independently.
#[derive(Debug, Default)]
struct TileBuild {
    tile: Mutex<Option<Arc<HeightRaster>>>,
    completed: Condvar,
}

impl ErodedTerrainSource {
    pub fn new(base: Arc<dyn TerrainSource>, cfg: ErosionConfig) -> Self {
        cfg.validate();
        let cache_capacity = cfg.cache_max_tiles;
        Self {
            base,
            cfg,
            // Raster buffers are retained by the bounded cache, so reserve all
            // metadata storage before the first terrain task reaches it.
            cache: Mutex::new(TileCache::with_capacity(cache_capacity)),
            in_flight: Mutex::new(HashMap::with_capacity(cache_capacity.min(8))),
        }
    }

    pub(super) fn tile_key(lat: f64, lon: f64, tile_deg: f64) -> (i64, i64) {
        let (lat, lon) = canonical_lat_lon(lat, lon);
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
        let (lat, lon) = canonical_lat_lon(lat, lon);
        let tile_deg = self.cfg.tile_deg;
        let (tx, ty) = Self::tile_key(lat, lon, tile_deg);

        // Check cache first.
        {
            let mut cache = self.cache.lock().expect("erosion cache lock");
            if let Some(tile) = cache.get((tx, ty)) {
                return tile;
            }
        }

        let (build, generates_tile) = {
            let mut in_flight = self.in_flight.lock().expect("erosion in-flight lock");
            match in_flight.get(&(tx, ty)) {
                Some(build) => (Arc::clone(build), false),
                None => {
                    let build = Arc::new(TileBuild::default());
                    in_flight.insert((tx, ty), Arc::clone(&build));
                    (build, true)
                }
            }
        };

        if !generates_tile {
            let mut tile = build.tile.lock().expect("erosion tile build lock");
            while tile.is_none() {
                tile = build.completed.wait(tile).expect("erosion tile build wait");
            }
            return Arc::clone(tile.as_ref().expect("completed erosion tile"));
        }

        // Generate (deterministic per tile) without holding the cache lock so
        // independent tiles continue to generate in parallel.
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

        self.cache.lock().expect("erosion cache lock").insert(
            (tx, ty),
            Arc::clone(&tile),
            self.cfg.cache_max_tiles,
        );

        {
            let mut completed_tile = build.tile.lock().expect("erosion tile build lock");
            *completed_tile = Some(Arc::clone(&tile));
            build.completed.notify_all();
        }
        self.in_flight
            .lock()
            .expect("erosion in-flight lock")
            .remove(&(tx, ty));
        tile
    }
}

impl TerrainSource for ErodedTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        let tile = self.get_tile(latitude_deg, longitude_deg);
        let (eroded, edge) = sample(&tile, latitude_deg, longitude_deg);
        let weight = erosion_weight(edge, self.cfg.edge_feather);
        if weight == 1.0 {
            eroded
        } else {
            let base_h = self.base.height_m(latitude_deg, longitude_deg);
            base_h + (eroded - base_h) * weight
        }
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        // Mesh generation intentionally uses the analytic base at every LOD to
        // keep patch edges identical, so streaming bounds follow that geometry.
        self.base.elevation_bounds_m()
    }

    fn mesh_height_m(&self, latitude_deg: f64, longitude_deg: f64, patch_level: u32) -> f64 {
        // Mesh edges must be independent of LOD: a different radial height at
        // the same geographic sample produces cracks that stitch indices cannot
        // close. Erosion remains exact for collision and close-range material
        // maps, while the macro mesh uses the deterministic analytic field.
        self.base
            .mesh_height_m(latitude_deg, longitude_deg, patch_level)
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        self.base.prepare_sample(latitude_deg, longitude_deg);
        self.get_tile(latitude_deg, longitude_deg);
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        let tile = self.get_tile(latitude_deg, longitude_deg);
        let (eroded, edge_factor) =
            sample_channel(&tile, &tile.moisture, latitude_deg, longitude_deg);
        let eroded = eroded.clamp(0.0, 1.0);
        let weight = erosion_weight(edge_factor, self.cfg.edge_feather);
        if weight == 1.0 {
            eroded
        } else {
            let base = self.base.moisture(latitude_deg, longitude_deg);
            base + (eroded - base) * weight
        }
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        let tile = self.get_tile(latitude_deg, longitude_deg);
        let (flow, edge) = sample_channel(&tile, &tile.flow, latitude_deg, longitude_deg);
        // A channel begins at the configured carving threshold and reaches full
        // visual strength at three times that flow, matching the carve cap.
        let normalized = (flow / f64::from(self.cfg.river_flow_threshold.max(f32::MIN_POSITIVE)))
            .log2()
            / 3.0f64.log2();
        let eroded = normalized.clamp(0.0, 1.0);
        let weight = erosion_weight(edge, self.cfg.edge_feather);
        let base = self.base.river_strength(latitude_deg, longitude_deg);
        base + (eroded - base) * weight
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        self.base.overview_height_m(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        self.base.overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (latitude_deg, longitude_deg) = canonical_lat_lon(latitude_deg, longitude_deg);
        self.base.overview_slope_deg(latitude_deg, longitude_deg)
    }

    fn zone_lat(&self, latitude_deg: f64) -> f64 {
        self.base.zone_lat(canonical_lat_lon(latitude_deg, 0.0).0)
    }
}
