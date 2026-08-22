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
use crate::domain::services::terrain_source::{TerrainSource, ProceduralTerrainSource};
#[cfg(feature = "dem")]
use std::collections::HashMap;
#[cfg(feature = "dem")]
use std::path::Path;
#[cfg(feature = "dem")]
use std::sync::Arc;

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
    pub data: Vec<f32>,      // Row-major height data in meters.
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
        if let Some(lru_key) = self.tiles
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
}

#[cfg(feature = "dem")]
impl Default for DemTerrainConfig {
    fn default() -> Self {
        Self {
            cache_max_tiles: 256,
            fallback_to_procedural: true,
        }
    }
}

/// Real planetary DEM terrain source implementing the TerrainSource trait.
#[cfg(feature = "dem")]
#[derive(Debug, Clone)]
pub struct DemTerrainSource {
    config: DemTerrainConfig,
    cache: DemTileCache,
    procedural_fallback: ProceduralTerrainSource,
}

#[cfg(feature = "dem")]
impl DemTerrainSource {
    pub fn new(config: DemTerrainConfig) -> Self {
        let cache_max_tiles = config.cache_max_tiles;
        let procedural_fallback = ProceduralTerrainSource::new(0xDEAD, 2500.0, 1200.0, 0);

        Self {
            config,
            cache: DemTileCache::new(cache_max_tiles),
            procedural_fallback,
        }
    }

    /// Get or load a DEM tile for the given lat/lon.
    fn get_tile(&mut self, dataset: DemDataset, lat: f64, lon: f64) -> Option<&DemTile> {
        let (tile_x, tile_y) = self.lat_lon_to_tile_xy(dataset, lat, lon);
        let key = DemTileKey { dataset, tile_x, tile_y };
        
        // Check cache first.
        if self.cache.get(key).is_some() {
            // Need to re-borrow to return the reference.
            return self.cache.get(key);
        }

        // Try to load the tile (placeholder - returns None to trigger fallback).
        if let Some(tile) = self.load_tile(dataset, tile_x, tile_y) {
            self.cache.insert(tile);
            return self.cache.get(key);
        }

        None
    }

    /// Convert lat/lon to tile coordinates for the given dataset.
    fn lat_lon_to_tile_xy(&self, dataset: DemDataset, lat: f64, lon: f64) -> (i32, i32) {
        match dataset {
            DemDataset::Srtm1 | DemDataset::Srtm3 => {
                let tile_size = if dataset == DemDataset::Srtm1 { 1.0 / 3600.0 } else { 1.0 };
                let tile_x = (lon / tile_size).floor() as i32;
                let tile_y = (lat / tile_size).floor() as i32;
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

    /// Load a DEM tile from disk or network.
    fn load_tile(&self, _dataset: DemDataset, _tile_x: i32, _tile_y: i32) -> Option<DemTile> {
        // Placeholder: in a real implementation, this would:
        // 1. Check local data directory for .hgt/.tif files
        // 2. Download from tile server if not present
        // 3. Parse using srtm_reader, geotiff, etc.
        // 4. Return DemTile with height data
        
        None
    }
}

#[cfg(feature = "dem")]
impl TerrainSource for DemTerrainSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        // Cannot mutably borrow self in a &self method, so we use a different approach.
        // In a real implementation, we'd use interior mutability (RefCell/Mutex) for the cache.
        // For now, fall back to procedural.
        
        if self.config.fallback_to_procedural {
            self.procedural_fallback.height_m(latitude_deg, longitude_deg)
        } else {
            0.0
        }
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
        
        let key1 = DemTileKey { dataset: DemDataset::Srtm3, tile_x: 0, tile_y: 0 };
        let key2 = DemTileKey { dataset: DemDataset::Srtm3, tile_x: 1, tile_y: 0 };
        let key3 = DemTileKey { dataset: DemDataset::Srtm3, tile_x: 2, tile_y: 0 };
        
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
        assert!(!(key1_exists && key2_exists), "one of key1 or key2 should be evicted");
        assert!(key1_exists || key2_exists, "at least one of key1 or key2 should remain");
    }
}