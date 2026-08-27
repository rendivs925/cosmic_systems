//! Real planetary DEM terrain source (AGENTS.md sections 20-21).
//!
//! Implements [`TerrainSource`] for real planetary heightmap data:
//! - NASA SRTM (Shuttle Radar Topography Mission) for Earth
//! - LRO LOLA (Lunar Orbiter Laser Altimeter) for Moon
//! - MOLA (Mars Orbiter Laser Altimeter) for Mars
//!
//! Loads GeoTIFF/HGT tiles on demand with an LRU cache, integrated with the
//! terrain streaming memory budget. Falls back to procedural generation for
//! regions without DEM coverage.

#[cfg(feature = "dem")]
use crate::domain::services::terrain_source::{ProceduralTerrainSource, TerrainSource};
#[cfg(feature = "dem")]
use std::collections::HashMap;
#[cfg(feature = "dem")]
use std::path::{Path, PathBuf};
#[cfg(all(feature = "dem", test))]
use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(feature = "dem")]
use std::sync::Mutex;

/// DEM dataset type.
#[cfg(feature = "dem")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DemDataset {
    /// NASA SRTM (Earth) - 1 arc-second or 3 arc-second.
    Srtm1,
    Srtm3,
    /// LRO LOLA (Moon) - polar stereographic or geographic.
    LroLola,
    /// MOLA (Mars) - geographic.
    Mola,
}

/// Key for a DEM tile in the cache.
#[cfg(feature = "dem")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DemTileKey {
    pub dataset: DemDataset,
    pub tile_x: i32,
    pub tile_y: i32,
}

/// A loaded DEM tile with height data.
#[cfg(feature = "dem")]
#[derive(Debug, Clone)]
pub struct DemTile {
    pub key: DemTileKey,
    pub data: Vec<f32>, // Row-major height data in meters.
    pub width: u32,
    pub height: u32,
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
    pub last_access_frame: u64,
}

/// LRU cache for DEM tiles, integrated with terrain streaming memory budget.
#[cfg(feature = "dem")]
#[derive(Debug, Clone)]
pub struct DemTileCache {
    tiles: HashMap<DemTileKey, DemTile>,
    max_tiles: usize,
    frame: u64,
}

#[cfg(feature = "dem")]
impl DemTileCache {
    pub fn new(max_tiles: usize) -> Self {
        Self {
            tiles: HashMap::new(),
            max_tiles,
            frame: 0,
        }
    }

    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    pub fn get(&mut self, key: DemTileKey) -> Option<&DemTile> {
        if let Some(tile) = self.tiles.get_mut(&key) {
            tile.last_access_frame = self.frame;
            Some(tile)
        } else {
            None
        }
    }

    pub fn insert(&mut self, tile: DemTile) {
        if self.tiles.len() >= self.max_tiles {
            self.evict_lru();
        }
        self.tiles.insert(tile.key, tile);
    }

    fn evict_lru(&mut self) {
        if let Some(lru_key) = self
            .tiles
            .iter()
            .min_by_key(|(_, t)| t.last_access_frame)
            .map(|(k, _)| *k)
        {
            self.tiles.remove(&lru_key);
        }
    }

    pub fn len(&self) -> usize {
        self.tiles.len()
    }
}

/// Configuration for DEM terrain source.
#[cfg(feature = "dem")]
#[derive(Debug, Clone)]
pub struct DemTerrainConfig {
    pub cache_max_tiles: usize,
    pub fallback_to_procedural: bool,
    /// Directory of local DEM tiles (SRTM `.hgt` etc.). `None` disables
    /// on-disk loading and falls back to procedural generation.
    pub data_dir: Option<PathBuf>,
    /// Which dataset serves `height_m` queries (Earth defaults to SRTM3).
    pub dataset: DemDataset,
}

#[cfg(feature = "dem")]
impl Default for DemTerrainConfig {
    fn default() -> Self {
        Self {
            cache_max_tiles: 256,
            fallback_to_procedural: true,
            data_dir: None,
            dataset: DemDataset::Srtm3,
        }
    }
}

/// Real planetary DEM terrain source implementing the TerrainSource trait.
#[cfg(feature = "dem")]
#[derive(Debug)]
pub struct DemTerrainSource {
    config: DemTerrainConfig,
    /// Interior mutability so `height_m(&self)` can load and cache tiles on
    /// demand (the trait only hands out `&self`; see design.md). A `Mutex`
    /// keeps the source `Send + Sync` for `Arc` sharing with the ECS; tile
    /// access is intentionally coarse-grained (loading is rare, reads amortize
    /// against the LRU cache).
    cache: Mutex<DemTileCache>,
    procedural_fallback: ProceduralTerrainSource,
    #[cfg(test)]
    tile_loads: AtomicUsize,
}

#[cfg(feature = "dem")]
impl DemTerrainSource {
    pub fn new(config: DemTerrainConfig) -> Self {
        let cache_max_tiles = config.cache_max_tiles;
        let procedural_fallback = ProceduralTerrainSource::new(0xDEAD, 2500.0, 1200.0, 0);

        Self {
            config,
            cache: Mutex::new(DemTileCache::new(cache_max_tiles)),
            procedural_fallback,
            #[cfg(test)]
            tile_loads: AtomicUsize::new(0),
        }
    }

    /// The tile coordinates (`dataset`, `tile_x`, `tile_y`) covering a lat/lon.
    fn cover_tile(&self, lat: f64, lon: f64) -> (DemDataset, i32, i32) {
        let (tile_x, tile_y) = self.lat_lon_to_tile_xy(self.config.dataset, lat, lon);
        (self.config.dataset, tile_x, tile_y)
    }

    /// Convert lat/lon to tile coordinates for the given dataset.
    fn lat_lon_to_tile_xy(&self, dataset: DemDataset, lat: f64, lon: f64) -> (i32, i32) {
        match dataset {
            DemDataset::Srtm1 => {
                // 1-arc-second tiles: 1/3600° tiles would be absurdly many, so
                // SRTM1 is addressed by its post spacing within a 1° HGT grid.
                let tile_x = (lon / 1.0).floor() as i32;
                let tile_y = (lat / 1.0).floor() as i32;
                (tile_x, tile_y)
            }
            DemDataset::Srtm3 => {
                // 3-arc-second (1°) tiles.
                let tile_x = (lon / 1.0).floor() as i32;
                let tile_y = (lat / 1.0).floor() as i32;
                (tile_x, tile_y)
            }
            DemDataset::LroLola => {
                let tile_x = (lon / 1.0).floor() as i32;
                let tile_y = (lat / 1.0).floor() as i32;
                (tile_x, tile_y)
            }
            DemDataset::Mola => {
                let tile_x = (lon / 1.0).floor() as i32;
                let tile_y = (lat / 1.0).floor() as i32;
                (tile_x, tile_y)
            }
        }
    }

    /// Bilinear height of a loaded tile at a lat/lon, if the point is inside
    /// its geographic bounds (deterministic: pure integer plus a few f64
    /// multiply-adds, so identical inputs yield identical outputs).
    fn height_from_tile(tile: &DemTile, lat: f64, lon: f64) -> Option<f64> {
        let in_bounds = lat >= tile.lat_min
            && lat <= tile.lat_max
            && lon >= tile.lon_min
            && lon <= tile.lon_max;
        in_bounds.then(|| bilinear_height(tile, lat, lon))
    }

    /// Load a DEM tile from disk using the configured data directory. SRTM
    /// tiles are 1°×1° `.hgt` grids (`N28W081.hgt`, big-endian i16 posts at
    /// 3″ or 1″ spacing). Returns `None` when no data directory/coverage.
    fn load_tile(&self, dataset: DemDataset, tile_x: i32, tile_y: i32) -> Option<DemTile> {
        #[cfg(test)]
        self.tile_loads.fetch_add(1, Ordering::Relaxed);
        let dir = self.config.data_dir.as_ref()?;
        let path = match dataset {
            DemDataset::Srtm3 | DemDataset::Srtm1 => {
                let grid = if dataset == DemDataset::Srtm1 {
                    3601
                } else {
                    1201
                };
                let tile = self.srtm_tile_path(dir, grid, tile_x, tile_y);
                load_hgt_tile(tile, tile_x, tile_y, dataset, grid).ok()?
            }
            // LOLA / MOLA tile loading is not yet wired to a local data source;
            // the fallback chain covers those bodies' procedural source.
            DemDataset::LroLola | DemDataset::Mola => return None,
        };
        Some(path)
    }

    /// SRTM `.hgt` filename for a tile index, e.g. `(x=-81, y=28) → N28W081.hgt`.
    fn srtm_tile_path(&self, dir: &Path, _grid: u32, tile_x: i32, tile_y: i32) -> PathBuf {
        let lat = tile_y;
        let lon = tile_x;
        let ns = if lat >= 0 { 'N' } else { 'S' };
        let ew = if lon >= 0 { 'E' } else { 'W' };
        dir.join(format!("{ns}{:02}{ew}{:03}.hgt", lat.abs(), lon.abs()))
    }

    /// Insert a tile directly into the cache. Used by tests and by the data
    /// pipeline to warm the cache without an on-disk read.
    pub fn insert_tile(&self, tile: DemTile) {
        self.cache.lock().expect("dem cache lock").insert(tile);
    }

    #[cfg(test)]
    fn tile_load_count(&self) -> usize {
        self.tile_loads.load(Ordering::Relaxed)
    }
}

/// Bilinear interpolation of a DEM tile's row-major height grid at a lat/lon.
/// The grid covers `[lat_min, lat_max] × [lon_min, lon_max]` with `width`
/// columns (lon) and `height` rows (lat). Fully deterministic and pure.
#[cfg(feature = "dem")]
pub fn bilinear_height(tile: &DemTile, lat: f64, lon: f64) -> f64 {
    if tile.width < 2 || tile.height < 2 {
        return tile.data.first().copied().unwrap_or(0.0) as f64;
    }
    let width = tile.width as usize;
    let height = tile.height as usize;
    let span_lon = tile.lon_max - tile.lon_min;
    let span_lat = tile.lat_max - tile.lat_min;
    let fx = if span_lon.abs() > 1e-12 {
        (lon - tile.lon_min) / span_lon * (width - 1) as f64
    } else {
        0.0
    };
    let fy = if span_lat.abs() > 1e-12 {
        (lat - tile.lat_min) / span_lat * (height - 1) as f64
    } else {
        0.0
    };
    let x0 = (fx.floor() as usize).min(width - 1);
    let y0 = (fy.floor() as usize).min(height - 1);
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let dx = (fx - x0 as f64).clamp(0.0, 1.0);
    let dy = (fy - y0 as f64).clamp(0.0, 1.0);

    let h00 = tile.data[y0 * width + x0] as f64;
    let h10 = tile.data[y0 * width + x1] as f64;
    let h01 = tile.data[y1 * width + x0] as f64;
    let h11 = tile.data[y1 * width + x1] as f64;

    let top = h00 * (1.0 - dx) + h10 * dx;
    let bottom = h01 * (1.0 - dx) + h11 * dx;
    top * (1.0 - dy) + bottom * dy
}

/// Parse an SRTM `.hgt` tile (big-endian signed 16-bit posts, `grid × grid`
/// rows, no header) into a [`DemTile`] covering a 1°×1° cell whose south-west
/// corner is `(tile_y, tile_x)`. Deterministic byte-for-byte.
#[cfg(feature = "dem")]
pub fn load_hgt_tile(
    path: PathBuf,
    tile_x: i32,
    tile_y: i32,
    dataset: DemDataset,
    grid: u32,
) -> Result<DemTile, String> {
    let bytes = std::fs::read(&path).map_err(|e| format!("hgt read {}: {e}", path.display()))?;
    let posts = (grid as usize).checked_pow(2).ok_or("hgt grid overflow")?;
    if bytes.len() != posts * 2 {
        return Err(format!(
            "hgt {}: expected {posts}*2 bytes, got {}",
            path.display(),
            bytes.len()
        ));
    }
    let mut data = Vec::with_capacity(posts);
    for chunk in bytes.chunks_exact(2) {
        let h = i16::from_be_bytes([chunk[0], chunk[1]]) as f32;
        data.push(h);
    }
    Ok(DemTile {
        key: DemTileKey {
            dataset,
            tile_x,
            tile_y,
        },
        data,
        width: grid,
        height: grid,
        lat_min: tile_y as f64,
        lat_max: tile_y as f64 + 1.0,
        lon_min: tile_x as f64,
        lon_max: tile_x as f64 + 1.0,
        last_access_frame: 0,
    })
}

#[cfg(feature = "dem")]
impl TerrainSource for DemTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let (dataset, tile_x, tile_y) = self.cover_tile(latitude_deg, longitude_deg);
        let key = DemTileKey {
            dataset,
            tile_x,
            tile_y,
        };
        let cached_height = {
            let mut cache = self.cache.lock().expect("dem cache lock");
            cache.tick();
            cache
                .get(key)
                .and_then(|t| Self::height_from_tile(t, latitude_deg, longitude_deg))
        };

        let height = cached_height.or_else(|| {
            // Loading occurs only after a cache miss. Re-locking also handles a
            // concurrent insertion without replacing its resident tile.
            let loaded = self.load_tile(dataset, tile_x, tile_y)?;
            let mut cache = self.cache.lock().expect("dem cache lock");
            cache.tick();
            if cache.get(key).is_none() {
                cache.insert(loaded);
            }
            cache
                .get(key)
                .and_then(|t| Self::height_from_tile(t, latitude_deg, longitude_deg))
        });

        match height {
            Some(h) => h,
            None => {
                if self.config.fallback_to_procedural {
                    self.procedural_fallback
                        .height_m(latitude_deg, longitude_deg)
                } else {
                    0.0
                }
            }
        }
    }

    fn overview_height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        // A map preview must never synchronously load a grid of DEM tiles.
        self.procedural_fallback
            .overview_height_m(latitude_deg, longitude_deg)
    }

    fn overview_moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.procedural_fallback
            .overview_moisture(latitude_deg, longitude_deg)
    }

    fn overview_slope_deg(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.procedural_fallback
            .overview_slope_deg(latitude_deg, longitude_deg)
    }
}

// Non-dem feature stub.
#[cfg(not(feature = "dem"))]
#[derive(Debug, Clone)]
pub struct DemTerrainSource;

#[cfg(not(feature = "dem"))]
impl DemTerrainSource {
    pub fn new(_config: ()) -> Self {
        Self
    }
}

#[cfg(not(feature = "dem"))]
impl crate::domain::services::terrain_source::TerrainSource for DemTerrainSource {
    fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }

    fn overview_height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
        0.0
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "dem")]
    use super::*;

    #[cfg(feature = "dem")]
    #[test]
    fn dem_tile_cache_lru_eviction() {
        let mut cache = DemTileCache::new(2);
        cache.tick();

        let key1 = DemTileKey {
            dataset: DemDataset::Srtm3,
            tile_x: 0,
            tile_y: 0,
        };
        let key2 = DemTileKey {
            dataset: DemDataset::Srtm3,
            tile_x: 1,
            tile_y: 0,
        };
        let key3 = DemTileKey {
            dataset: DemDataset::Srtm3,
            tile_x: 2,
            tile_y: 0,
        };

        cache.insert(DemTile {
            key: key1,
            data: vec![],
            width: 1,
            height: 1,
            lat_min: 0.0,
            lat_max: 1.0,
            lon_min: 0.0,
            lon_max: 1.0,
            last_access_frame: cache.frame,
        });
        cache.insert(DemTile {
            key: key2,
            data: vec![],
            width: 1,
            height: 1,
            lat_min: 1.0,
            lat_max: 2.0,
            lon_min: 0.0,
            lon_max: 1.0,
            last_access_frame: cache.frame,
        });

        assert_eq!(cache.len(), 2);

        // Add third tile - should evict LRU (one of key1 or key2).
        cache.tick();
        cache.insert(DemTile {
            key: key3,
            data: vec![],
            width: 1,
            height: 1,
            lat_min: 2.0,
            lat_max: 3.0,
            lon_min: 0.0,
            lon_max: 1.0,
            last_access_frame: cache.frame,
        });

        assert_eq!(cache.len(), 2);
        // One of key1 or key2 should be evicted (LRU), but not key3.
        let key1_exists = cache.get(key1).is_some();
        let key2_exists = cache.get(key2).is_some();
        let key3_exists = cache.get(key3).is_some();
        assert!(key3_exists, "key3 should exist after insertion");
        assert!(
            !(key1_exists && key2_exists),
            "one of key1 or key2 should be evicted"
        );
        assert!(
            key1_exists || key2_exists,
            "at least one of key1 or key2 should remain"
        );
    }

    /// Build a small synthetic tile covering a known 1°×1° cell.
    #[cfg(feature = "dem")]
    fn sample_tile(tile_x: i32, tile_y: i32, width: u32, height: u32) -> DemTile {
        let data = (0..width * height).map(|i| i as f32 * 10.0).collect();
        DemTile {
            key: DemTileKey {
                dataset: DemDataset::Srtm3,
                tile_x,
                tile_y,
            },
            data,
            width,
            height,
            lat_min: tile_y as f64,
            lat_max: tile_y as f64 + 1.0,
            lon_min: tile_x as f64,
            lon_max: tile_x as f64 + 1.0,
            last_access_frame: 0,
        }
    }

    #[cfg(feature = "dem")]
    #[test]
    fn bilinear_interpolates_between_grid_posts() {
        // 3×3 tile, row-major (data = row*width + col).
        let tile = sample_tile(0, 0, 3, 3);
        // The four corners are 0, 20 (row 0: 0,10,20), 40,60 (row 2: 40,50,60).
        // The centre grid post (1,1) = 40.
        let centre = bilinear_height(&tile, 0.5, 0.5);
        assert!((centre - 40.0).abs() < 1e-9);
        // Query the top-left post (0,2) exactly → 20.
        let post = bilinear_height(&tile, 0.0, 1.0);
        assert!((post - 20.0).abs() < 1e-9);
    }

    #[cfg(feature = "dem")]
    #[test]
    fn injected_tile_drives_deterministic_height_queries() {
        // Spec "deterministic height queries": inject an SRTM3 tile covering
        // KSC (lat 28..29, lon -81..-80) and query an arbitrary point inside;
        // the result must be stable across calls and differ between two
        // distinct points.
        let source = DemTerrainSource::new(DemTerrainConfig {
            cache_max_tiles: 4,
            fallback_to_procedural: true,
            data_dir: None,
            dataset: DemDataset::Srtm3,
        });
        source.insert_tile(sample_tile(-81, 28, 4, 4));

        // KSC is at (28.57, -80.65): inside tile (-81..-80, 28..29).
        let a = source.height_m(28.57, -80.65);
        let b = source.height_m(28.57, -80.65);
        assert_eq!(a, b, "same lat/lon must be bitwise identical");
        // Distinct points sample distinct heights.
        let c = source.height_m(28.58, -80.66);
        assert_ne!(a, c, "different points must differ on a smooth gradient");
        assert!(a.is_finite() && c.is_finite());
    }

    #[cfg(feature = "dem")]
    #[test]
    fn height_outside_coverage_falls_back_to_procedural() {
        // Spec "DEM + procedural fallback": a query far outside the injected
        // tile falls back to the procedural source (deterministic, finite).
        let source = DemTerrainSource::new(DemTerrainConfig::default());
        source.insert_tile(sample_tile(-81, 28, 4, 4));
        let outside = source.height_m(-33.9, 151.2); // Sydney — far from tile
        assert!(outside.is_finite());
        // Procedural generation is seeded → deterministic.
        assert_eq!(outside, source.height_m(-33.9, 151.2));
    }

    #[cfg(feature = "dem")]
    #[test]
    fn overview_sampling_does_not_load_dem_tiles() {
        let dir = std::env::temp_dir().join("dem_overview_test");
        std::fs::create_dir_all(&dir).unwrap();
        let source = DemTerrainSource::new(DemTerrainConfig {
            cache_max_tiles: 4,
            fallback_to_procedural: true,
            data_dir: Some(dir.clone()),
            dataset: DemDataset::Srtm3,
        });

        let _ = source.overview_height_m(28.5, -80.5);
        let _ = source.overview_moisture(28.5, -80.5);
        let _ = source.overview_slope_deg(28.5, -80.5);

        assert_eq!(source.tile_load_count(), 0);
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(feature = "dem")]
    #[test]
    fn hgt_tile_parses_big_endian_posts() {
        // Write a 2×2 .hgt (8 bytes big-endian) and load it: posts 1,2,3,4.
        let dir = std::env::temp_dir().join("dem_hgt_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("sample.hgt");
        let bytes: Vec<u8> = [1i16, 2, 3, 4]
            .into_iter()
            .flat_map(|h| h.to_be_bytes())
            .collect();
        std::fs::write(&path, &bytes).unwrap();

        let tile = load_hgt_tile(path, -81, 28, DemDataset::Srtm3, 2).unwrap();
        assert_eq!(tile.width, 2);
        assert_eq!(tile.height, 2);
        assert_eq!(tile.data, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(tile.lat_min, 28.0);
        assert_eq!(tile.lat_max, 29.0);
        assert_eq!(tile.lon_min, -81.0);
        assert_eq!(tile.lon_max, -80.0);

        // Deterministic: parsing the same bytes twice yields the same tile.
        assert_eq!(
            load_hgt_tile(
                std::env::temp_dir().join("dem_hgt_test/sample.hgt"),
                -81,
                28,
                DemDataset::Srtm3,
                2
            )
            .unwrap()
            .data,
            tile.data
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[cfg(feature = "dem")]
    #[test]
    fn height_m_loads_hgt_tile_from_data_dir() {
        // Real SRTM-processed flow: data_dir + a full 1201×1201 .hgt for the
        // tile covering KSC; the query serves the DEM-pinned height (all posts
        // at 1500 m ⇒ any query returns ~1500) rather than procedural.
        let dir = std::env::temp_dir().join("dem_hgt_flow");
        std::fs::create_dir_all(&dir).unwrap();
        // SRTM3 tile (-81, 28) is named N28W081.hgt.
        let path = dir.join("N28W081.hgt");
        let grid = 1201usize;
        let posts = grid * grid;
        let mut bytes = Vec::with_capacity(posts * 2);
        for _ in 0..posts {
            bytes.extend_from_slice(&1500i16.to_be_bytes());
        }
        std::fs::write(&path, &bytes).unwrap();

        let source = DemTerrainSource::new(DemTerrainConfig {
            cache_max_tiles: 4,
            fallback_to_procedural: true,
            data_dir: Some(dir.clone()),
            dataset: DemDataset::Srtm3,
        });
        // Query inside tile (-81..-80, 28..29).
        let h = source.height_m(28.5, -80.5);
        assert!(
            (h - 1500.0).abs() < 1.0,
            "expected DEM-backed height ~1500, got {h}"
        );
        // Deterministic across queries.
        assert_eq!(h, source.height_m(28.5, -80.5));
        assert_eq!(
            source.tile_load_count(),
            1,
            "resident tile must not be reparsed"
        );
        std::fs::remove_dir_all(dir).ok();
    }
}
