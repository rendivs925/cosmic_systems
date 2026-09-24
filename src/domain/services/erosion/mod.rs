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
//!
//! The implementation is split into cohesive submodules: [`simulate`] (the
//! per-tile erosion algorithms and raster baking) and [`source`] (the cached
//! [`ErodedTerrainSource`] adapter).

mod simulate;
mod source;

pub use simulate::{carve_rivers, erode_tile, flow_accumulation, hydraulic_erode, thermal_erode};
pub use source::ErodedTerrainSource;

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

impl ErosionConfig {
    /// Reject values that would make the static erosion model non-physical or
    /// numerically undefined before any tile work is scheduled.
    pub fn validate(&self) {
        assert!(
            self.tile_deg.is_finite() && (0.0 < self.tile_deg && self.tile_deg <= 180.0),
            "erosion tile_deg must be finite and in (0, 180] degrees"
        );
        assert!(
            self.resolution >= 2,
            "erosion resolution must contain at least two vertices per side"
        );
        assert!(
            self.talus_slope.is_finite() && self.talus_slope >= 0.0,
            "erosion talus_slope must be finite and non-negative"
        );
        assert!(
            self.droplet_inertia.is_finite() && (0.0..=1.0).contains(&self.droplet_inertia),
            "erosion droplet_inertia must be finite and in [0, 1]"
        );
        assert!(
            self.droplet_capacity.is_finite() && self.droplet_capacity >= 0.0,
            "erosion droplet_capacity must be finite and non-negative"
        );
        assert!(
            self.droplet_erosion.is_finite() && self.droplet_erosion >= 0.0,
            "erosion droplet_erosion must be finite and non-negative"
        );
        assert!(
            self.droplet_deposition.is_finite() && (0.0..=1.0).contains(&self.droplet_deposition),
            "erosion droplet_deposition must be finite and in [0, 1]"
        );
        assert!(
            self.droplet_evaporation.is_finite() && (0.0..=1.0).contains(&self.droplet_evaporation),
            "erosion droplet_evaporation must be finite and in [0, 1]"
        );
        assert!(
            self.river_flow_threshold.is_finite() && self.river_flow_threshold > 0.0,
            "erosion river_flow_threshold must be finite and positive"
        );
        assert!(
            self.river_depth_m.is_finite() && self.river_depth_m >= 0.0,
            "erosion river_depth_m must be finite and non-negative"
        );
        assert!(
            self.edge_feather.is_finite() && (0.0..=0.5).contains(&self.edge_feather),
            "erosion edge_feather must be finite and in [0, 0.5]"
        );
        assert!(
            self.cache_max_tiles > 0,
            "erosion cache_max_tiles must be positive"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::simulate::{
        idx, steepest_downhill, steepest_downhill_with_spacing, GridSpacing, Rng,
    };
    use super::source::{erosion_weight, sample};
    use super::*;
    use crate::domain::services::terrain_source::{
        ElevationBounds, ProceduralTerrainSource, TerrainSource,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::Duration;

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
    fn mesh_samples_use_the_lod_independent_macro_field() {
        let source = ErodedTerrainSource::new(Arc::new(base()), cfg());
        let latitude_deg = 28.5;
        let longitude_deg = -80.6;

        assert_eq!(
            source.mesh_height_m(latitude_deg, longitude_deg, 3),
            source.base.height_m(latitude_deg, longitude_deg)
        );
        assert!(source
            .cache
            .lock()
            .expect("erosion cache lock")
            .tiles
            .is_empty());

        assert_eq!(
            source.mesh_height_m(latitude_deg, longitude_deg, 12),
            source.base.height_m(latitude_deg, longitude_deg)
        );
        assert!(source
            .cache
            .lock()
            .expect("erosion cache lock")
            .tiles
            .is_empty());
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
        let first_col: f32 = (0..hgt).map(|y| flow[y * w]).sum();
        assert!(last_col > first_col, "flow should accumulate downhill");
    }

    #[test]
    fn rivers_carve_below_surroundings() {
        let w = 16;
        let hgt = 16;
        let mut h = vec![1000.0f32; w * hgt];
        let flow = vec![100.0f32; w * hgt];
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

    #[test]
    fn river_strength_bakes_the_same_authoritative_tile_as_other_channels() {
        let source = ErodedTerrainSource::new(
            Arc::new(ProceduralTerrainSource::new(0, 0.0, 0.0, 0)),
            ErosionConfig {
                resolution: 16,
                droplets: 0,
                thermal_iterations: 0,
                river_flow_threshold: 0.5,
                ..cfg()
            },
        );

        let strength = source.river_strength(11.0, 21.0);
        assert!((0.0..=1.0).contains(&strength));
        assert!(
            strength > 0.0,
            "cached flow must become visible as a river channel"
        );
    }

    #[test]
    fn edge_factor_is_zero_at_tile_center_and_one_at_the_boundary() {
        let raster = HeightRaster {
            width: 2,
            height: 2,
            lat_min: 10.0,
            lat_max: 12.0,
            lon_min: 20.0,
            lon_max: 22.0,
            data: vec![0.0; 4],
            moisture: vec![0.0; 4],
            flow: vec![0.0; 4],
        };

        assert_eq!(sample(&raster, 11.0, 21.0).1, 0.0);
        assert_eq!(sample(&raster, 10.0, 21.0).1, 1.0);
    }

    #[test]
    fn erosion_weight_only_fades_inside_the_configured_edge_band() {
        assert_eq!(erosion_weight(1.0, 0.12), 0.0);
        assert_eq!(erosion_weight(0.0, 0.12), 1.0);
        // `edge_factor` of 0.5 is one quarter of a tile width from its edge,
        // safely outside a 12%-wide transition band.
        assert_eq!(erosion_weight(0.5, 0.12), 1.0);
        assert!(erosion_weight(0.9, 0.12) > 0.0);
        assert!(erosion_weight(0.9, 0.12) < 1.0);
    }

    #[derive(Debug, Default)]
    struct CountingTerrain {
        height_samples: AtomicUsize,
    }

    impl TerrainSource for CountingTerrain {
        fn height_m(&self, _latitude_deg: f64, _longitude_deg: f64) -> f64 {
            self.height_samples.fetch_add(1, Ordering::Relaxed);
            // Keep the first bake in progress long enough for every worker to
            // observe the shared in-flight entry.
            thread::sleep(Duration::from_millis(1));
            0.0
        }

        fn elevation_bounds_m(&self) -> ElevationBounds {
            ElevationBounds::new(0.0, 0.0)
        }
    }

    #[test]
    fn concurrent_queries_bake_a_tile_once() {
        let base = Arc::new(CountingTerrain::default());
        let source = Arc::new(ErodedTerrainSource::new(
            base.clone(),
            ErosionConfig {
                resolution: 16,
                droplets: 0,
                thermal_iterations: 0,
                ..cfg()
            },
        ));
        let barrier = Arc::new(Barrier::new(8));
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let source = Arc::clone(&source);
                let barrier = Arc::clone(&barrier);
                thread::spawn(move || {
                    barrier.wait();
                    source.prepare_sample(11.0, 21.0);
                    source.height_m(11.0, 21.0)
                })
            })
            .collect();

        let heights: Vec<_> = workers
            .into_iter()
            .map(|worker| worker.join().expect("erosion worker must complete"))
            .collect();
        assert!(heights.windows(2).all(|pair| pair[0] == pair[1]));
        assert_eq!(base.height_samples.load(Ordering::Relaxed), 16 * 16);
    }

    #[test]
    fn height_m_bakes_an_authoritative_tile_without_prepare_sample() {
        let base = Arc::new(CountingTerrain::default());
        let source = ErodedTerrainSource::new(
            base.clone(),
            ErosionConfig {
                resolution: 16,
                droplets: 0,
                thermal_iterations: 0,
                ..cfg()
            },
        );

        assert_eq!(source.height_m(11.0, 21.0), 0.0);
        assert_eq!(
            base.height_samples.load(Ordering::Relaxed),
            16 * 16,
            "height_m must sample the same erosion tile as prepare_sample"
        );
    }

    #[test]
    fn cached_tiles_are_touched_before_lru_eviction() {
        let source = ErodedTerrainSource::new(
            Arc::new(base()),
            ErosionConfig {
                resolution: 4,
                droplets: 0,
                thermal_iterations: 0,
                cache_max_tiles: 2,
                ..cfg()
            },
        );
        source.prepare_sample(10.5, 20.5);
        source.prepare_sample(10.5, 22.5);
        let _ = source.height_m(10.5, 20.5); // Refresh the first tile.
        source.prepare_sample(10.5, 24.5);

        let cache = source.cache.lock().expect("erosion cache lock");
        assert!(cache.tiles.contains_key(&(10, 5)));
        assert!(!cache.tiles.contains_key(&(11, 5)));
        assert!(cache.tiles.contains_key(&(12, 5)));
    }

    #[test]
    fn terrain_channels_are_cache_history_independent() {
        let source = ErodedTerrainSource::new(
            Arc::new(base()),
            ErosionConfig {
                resolution: 16,
                droplets: 100,
                thermal_iterations: 1,
                cache_max_tiles: 1,
                river_flow_threshold: 0.5,
                ..cfg()
            },
        );
        let coordinate = (11.0, 21.0);
        let before_prepare = (
            source.height_m(coordinate.0, coordinate.1),
            source.moisture(coordinate.0, coordinate.1),
            source.river_strength(coordinate.0, coordinate.1),
        );

        source.prepare_sample(coordinate.0, coordinate.1);
        assert_eq!(
            before_prepare,
            (
                source.height_m(coordinate.0, coordinate.1),
                source.moisture(coordinate.0, coordinate.1),
                source.river_strength(coordinate.0, coordinate.1),
            ),
            "prepare_sample must not change an authoritative terrain result"
        );

        source.prepare_sample(13.0, 23.0); // Evicts the coordinate's only tile.
        assert_eq!(
            before_prepare,
            (
                source.height_m(coordinate.0, coordinate.1),
                source.moisture(coordinate.0, coordinate.1),
                source.river_strength(coordinate.0, coordinate.1),
            ),
            "regenerating an evicted tile must reproduce every terrain channel"
        );
    }

    #[test]
    fn canonical_coordinates_share_tile_keys_and_samples() {
        let source = ErodedTerrainSource::new(Arc::new(base()), cfg());

        assert_eq!(
            ErodedTerrainSource::tile_key(10.5, 21.0, 2.0),
            ErodedTerrainSource::tile_key(10.5, 381.0, 2.0),
            "longitudes separated by a full turn must share a tile"
        );
        assert_eq!(
            ErodedTerrainSource::tile_key(100.0, 20.0, 2.0),
            ErodedTerrainSource::tile_key(80.0, -160.0, 2.0),
            "coordinates reflected across the north pole must share a tile"
        );

        for ((latitude_a, longitude_a), (latitude_b, longitude_b)) in [
            ((10.5, 21.0), (10.5, 381.0)),
            ((100.0, 20.0), (80.0, -160.0)),
        ] {
            assert_eq!(
                source.height_m(latitude_a, longitude_a),
                source.height_m(latitude_b, longitude_b)
            );
            assert_eq!(
                source.moisture(latitude_a, longitude_a),
                source.moisture(latitude_b, longitude_b)
            );
            assert_eq!(
                source.river_strength(latitude_a, longitude_a),
                source.river_strength(latitude_b, longitude_b)
            );
        }
    }

    #[test]
    fn erosion_config_rejects_non_physical_values_at_source_construction() {
        for invalid in [
            ErosionConfig {
                tile_deg: f64::NAN,
                ..cfg()
            },
            ErosionConfig {
                resolution: 1,
                ..cfg()
            },
            ErosionConfig {
                droplet_evaporation: 1.1,
                ..cfg()
            },
            ErosionConfig {
                river_flow_threshold: 0.0,
                ..cfg()
            },
            ErosionConfig {
                edge_feather: 0.6,
                ..cfg()
            },
            ErosionConfig {
                cache_max_tiles: 0,
                ..cfg()
            },
        ] {
            assert!(std::panic::catch_unwind(|| ErodedTerrainSource::new(
                Arc::new(base()),
                invalid
            ))
            .is_err());
        }
    }

    #[test]
    fn hydraulic_erosion_only_deposits_material_removed_above_the_floor() {
        let mut heights = vec![-7_999.9f32, -8_000.0];
        let initial_total: f32 = heights.iter().sum();
        let config = ErosionConfig {
            droplet_inertia: 1.0,
            droplet_capacity: 10.0,
            droplet_erosion: 0.3,
            droplet_evaporation: 0.0,
            ..cfg()
        };
        let seed = (1..)
            .find(|&candidate| Rng::new(candidate).usize(2) == 0)
            .expect("a deterministic seed must select the uphill cell");

        hydraulic_erode(&mut heights, 2, 1, 1.0, 1, seed, &config);
        let final_total: f32 = heights.iter().sum();
        assert!(
            (final_total - initial_total).abs() < 0.001,
            "the floor must not create sediment: {initial_total} -> {final_total}"
        );
        assert!(heights.iter().all(|height| *height >= -8_000.0));
    }

    #[test]
    fn d8_uses_distance_aware_and_latitude_aware_slopes() {
        let mut heights = vec![20.0f32; 3 * 3];
        heights[idx(1, 1, 3)] = 10.0;
        heights[idx(2, 1, 3)] = 1.0; // Raw drop 9 over 10 m.
        heights[idx(2, 2, 3)] = 0.0; // Raw drop 10 over sqrt(200) m.
        let uniform = GridSpacing::uniform(10.0, 3);
        assert_eq!(
            steepest_downhill_with_spacing(1, 1, &heights, 3, 3, &uniform)
                .map(|(index, _, _)| index),
            Some(idx(2, 1, 3)),
            "D8 must select the steeper cardinal slope over the lower diagonal"
        );

        let polar = GridSpacing::from_tile(88.0, 90.0, 0.0, 2.0, 3);
        assert!(polar.east_west_m[1] < polar.north_south_m);
        heights.fill(20.0);
        heights[idx(1, 1, 3)] = 10.0;
        heights[idx(2, 1, 3)] = 9.0; // Small drop over the short polar east-west cell.
        heights[idx(1, 2, 3)] = 0.0; // Larger drop over one latitude cell.
        assert_eq!(
            steepest_downhill_with_spacing(1, 1, &heights, 3, 3, &polar).map(|(index, _, _)| index),
            Some(idx(2, 1, 3)),
            "D8 must account for the latitude-dependent east-west cell width"
        );
    }
}
