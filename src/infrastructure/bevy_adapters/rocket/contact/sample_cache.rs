//! Exact-sample LRU cache for repeated fixed-step terrain-contact probes.

use crate::domain::services::terrain_collision::{sample_surface, SurfaceSample};
use crate::domain::services::terrain_source::TerrainSource;
use bevy::prelude::{Entity, Resource};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const TERRAIN_SURFACE_SAMPLE_CACHE_CAPACITY: usize = 512;

/// Thread-safe exact-sample cache for repeated fixed-step contact probes.
///
/// The key uses the input f64 bit patterns rather than spatial quantization, so
/// a cached result is bit-identical to direct `TerrainSource` evaluation. It is
/// presentation-independent and does not alter the source or collision model.
#[derive(Clone, Resource)]
pub struct TerrainSurfaceSampleCache {
    entries: Arc<Mutex<TerrainSurfaceSampleLru>>,
}

impl Default for TerrainSurfaceSampleCache {
    fn default() -> Self {
        Self {
            entries: Arc::new(Mutex::new(TerrainSurfaceSampleLru::new(
                TERRAIN_SURFACE_SAMPLE_CACHE_CAPACITY,
            ))),
        }
    }
}

impl TerrainSurfaceSampleCache {
    pub(crate) fn sample(
        &self,
        planet: Entity,
        source: &dyn TerrainSource,
        latitude_deg: f64,
        longitude_deg: f64,
        radius_m: f64,
    ) -> SurfaceSample {
        let key = TerrainSurfaceSampleKey::new(planet, latitude_deg, longitude_deg, radius_m);
        if let Some(sample) = self.lock().get(&key) {
            return sample;
        }

        // Do not hold the lock through the multi-octave terrain evaluation.
        let sample = sample_surface(source, latitude_deg, longitude_deg, radius_m);
        let mut entries = self.lock();
        if let Some(existing) = entries.get(&key) {
            return existing;
        }
        entries.insert(key, sample);
        sample
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, TerrainSurfaceSampleLru> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TerrainSurfaceSampleKey {
    planet: Entity,
    latitude_bits: u64,
    longitude_bits: u64,
    radius_bits: u64,
}

impl TerrainSurfaceSampleKey {
    fn new(planet: Entity, latitude_deg: f64, longitude_deg: f64, radius_m: f64) -> Self {
        Self {
            planet,
            latitude_bits: latitude_deg.to_bits(),
            longitude_bits: longitude_deg.to_bits(),
            radius_bits: radius_m.to_bits(),
        }
    }
}

struct TerrainSurfaceSampleLru {
    /// Sample plus the monotonic use stamp; the largest stamp is the most
    /// recently used entry.
    samples: HashMap<TerrainSurfaceSampleKey, (SurfaceSample, u64)>,
    clock: u64,
    capacity: usize,
}

impl TerrainSurfaceSampleLru {
    fn new(capacity: usize) -> Self {
        Self {
            samples: HashMap::with_capacity(capacity),
            clock: 0,
            capacity,
        }
    }

    fn get(&mut self, key: &TerrainSurfaceSampleKey) -> Option<SurfaceSample> {
        self.clock += 1;
        let entry = self.samples.get_mut(key)?;
        entry.1 = self.clock;
        Some(entry.0)
    }

    fn insert(&mut self, key: TerrainSurfaceSampleKey, sample: SurfaceSample) {
        self.clock += 1;
        if let Some(entry) = self.samples.get_mut(&key) {
            *entry = (sample, self.clock);
            return;
        }
        if self.samples.len() >= self.capacity {
            // Touch is O(1); only a full cache scans for the least-recently
            // used stamp, which keeps hot fixed-step contact probes cheap.
            let oldest = self
                .samples
                .iter()
                .min_by_key(|(_, (_, stamp))| *stamp)
                .map(|(key, _)| *key);
            if let Some(oldest) = oldest {
                self.samples.remove(&oldest);
            }
        }
        self.samples.insert(key, (sample, self.clock));
    }
}
