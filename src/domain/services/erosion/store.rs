//! Persistent, self-validating on-disk cache for baked erosion tiles.
//!
//! Erosion rasters are a pure, deterministic function of the base terrain, the
//! [`ErosionConfig`](super::ErosionConfig), and the tile key. That makes them
//! ideal to preprocess once and reuse, so an "ultra" quality bake costs disk
//! space instead of runtime CPU. This store is the persistence half of the
//! existing offline path ([`bake_tile_containing`](super::ErodedTerrainSource::bake_tile_containing)
//! and [`install_baked_tile`](super::ErodedTerrainSource::install_baked_tile)).
//!
//! Every tile records a **bake signature** (the configuration plus a caller
//! namespace) and a **base probe** (a coarse grid of pre-erosion base heights
//! sampled across the tile). A hit is accepted only when the signature matches,
//! the tile key and bounds agree with the request, the payload checksum and
//! values are valid, and the probe still matches the live base. A changed
//! quality configuration, seed, DEM, or a newly resident elevation payload can
//! therefore never serve a stale raster.
//!
//! The probe is a practical staleness guard, not a cryptographic content hash:
//! a base that changes only between the sampled grid points would not be
//! detected. The store never owns terrain authority; a mismatch is a miss and
//! the runtime re-bakes.

use super::HeightRaster;
use crate::domain::services::terrain_source::TerrainSource;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MAGIC: [u8; 4] = *b"CET2";
const VERSION: u32 = 2;
/// Grid dimension of the base probe stored with each tile.
pub const PROBE_SIDE: usize = 4;
/// Number of probe samples: `PROBE_SIDE * PROBE_SIDE`.
pub const PROBE_LEN: usize = PROBE_SIDE * PROBE_SIDE;
/// magic + version + signature + key(2×i64) + 4 bounds + width + height +
/// probe_side + checksum + probe.
const HEADER_BYTES: usize = 4 + 4 + 8 + 8 + 8 + 8 * 4 + 4 + 4 + 4 + 8 + PROBE_LEN * 4;
const CHANNELS: usize = 3;
/// Bytes written per baked tile at a given resolution.
pub fn tile_bytes(resolution: u32) -> u64 {
    HEADER_BYTES as u64 + CHANNELS as u64 * resolution as u64 * resolution as u64 * 4
}

/// The pre-erosion base height (metres) sampled on a [`PROBE_SIDE`] grid over a
/// tile, in row-major order from `(lat_min, lon_min)` to `(lat_max, lon_max)`.
/// Stored with each tile so a cache hit can be rejected when the authoritative
/// base has changed underneath it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseProbe(pub [f32; PROBE_LEN]);

impl BaseProbe {
    /// Sample the base on the probe grid spanning the tile bounds.
    pub fn sample(
        base: &dyn TerrainSource,
        lat_min: f64,
        lat_max: f64,
        lon_min: f64,
        lon_max: f64,
    ) -> Self {
        let mut samples = [0.0f32; PROBE_LEN];
        let last = (PROBE_SIDE - 1) as f64;
        for row in 0..PROBE_SIDE {
            for column in 0..PROBE_SIDE {
                let latitude_deg = lat_min + (lat_max - lat_min) * (row as f64 / last);
                let longitude_deg = lon_min + (lon_max - lon_min) * (column as f64 / last);
                samples[row * PROBE_SIDE + column] =
                    base.height_m(latitude_deg, longitude_deg) as f32;
            }
        }
        Self(samples)
    }

    /// Compare within a tolerance that absorbs f32 rounding but rejects a
    /// genuinely different base sample.
    pub fn matches(self, other: BaseProbe) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .all(|(a, b)| (a - b).abs() <= 1e-3)
    }
}

/// A read-through/write-through disk cache for deterministic erosion rasters.
/// It never owns terrain authority: a miss simply falls back to the runtime
/// bake, and a mismatched signature or base probe is treated as a miss.
#[derive(Debug)]
pub struct ErosionTileStore {
    root: PathBuf,
    signature: u64,
    /// Distinguishes temporary files from concurrent writers so two processes
    /// baking the same tile never interleave writes into one temp file.
    temp_counter: AtomicU64,
}

impl ErosionTileStore {
    /// Open (creating if needed) a cache rooted at `root` for one bake
    /// identity. `signature` must come from
    /// [`ErosionConfig::bake_signature`](super::ErosionConfig::bake_signature)
    /// so tiles from a different quality, seed, or namespace are never mixed.
    pub fn open(root: impl Into<PathBuf>, signature: u64) -> io::Result<Self> {
        // Namespace by signature so two qualities (or two DEMs) never share
        // files, and a stale signature cannot even be discovered.
        let root = root.into().join(format!("{signature:016x}"));
        fs::create_dir_all(&root)?;
        Ok(Self {
            root,
            signature,
            temp_counter: AtomicU64::new(0),
        })
    }

    /// Open the cache and prune it down to `max_bytes` before use. Existing
    /// caches can otherwise grow without bound; the oldest tiles (by mtime) are
    /// discarded first because they are the least likely to be revisited.
    pub fn open_with_budget(
        root: impl Into<PathBuf>,
        signature: u64,
        max_bytes: u64,
    ) -> io::Result<Self> {
        let store = Self::open(root, signature)?;
        store.enforce_budget(max_bytes)?;
        Ok(store)
    }

    /// Root directory of the cache.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path_for(&self, tx: i64, ty: i64) -> PathBuf {
        self.root.join(format!("{tx}_{ty}.cet1"))
    }

    /// Whether a tile file exists (without validating its contents).
    pub fn contains(&self, tx: i64, ty: i64) -> bool {
        self.path_for(tx, ty).is_file()
    }

    /// Load and validate a baked tile. Returns `None` (a miss) for a missing
    /// file, a corrupt header, a signature mismatch, a key/bounds/dimension
    /// disagreement, a payload checksum failure, or non-finite values. The
    /// caller compares the returned [`BaseProbe`] against the live base before
    /// trusting the tile.
    pub fn load(
        &self,
        tx: i64,
        ty: i64,
        resolution: u32,
        tile_deg: f64,
    ) -> Option<(HeightRaster, BaseProbe)> {
        let path = self.path_for(tx, ty);
        // Reject a file whose size cannot match before reading it, so a corrupt
        // or hostile file cannot force a large allocation.
        if fs::metadata(&path).ok()?.len() != tile_bytes(resolution) {
            return None;
        }
        let bytes = fs::read(&path).ok()?;
        Self::decode(&bytes, self.signature, tx, ty, resolution, tile_deg)
    }

    fn decode(
        bytes: &[u8],
        signature: u64,
        tx: i64,
        ty: i64,
        resolution: u32,
        tile_deg: f64,
    ) -> Option<(HeightRaster, BaseProbe)> {
        if bytes.len() < HEADER_BYTES || bytes[..4] != MAGIC {
            return None;
        }
        let read_u32 = |offset: usize| -> Option<u32> {
            Some(u32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        let read_u64 = |offset: usize| -> Option<u64> {
            Some(u64::from_le_bytes(
                bytes.get(offset..offset + 8)?.try_into().ok()?,
            ))
        };
        let read_i64 = |offset: usize| -> Option<i64> {
            Some(i64::from_le_bytes(
                bytes.get(offset..offset + 8)?.try_into().ok()?,
            ))
        };
        let read_f64 = |offset: usize| -> Option<f64> {
            Some(f64::from_le_bytes(
                bytes.get(offset..offset + 8)?.try_into().ok()?,
            ))
        };
        let read_f32 = |offset: usize| -> Option<f32> {
            Some(f32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };

        if read_u32(4)? != VERSION || read_u64(8)? != signature {
            return None;
        }
        if read_i64(16)? != tx || read_i64(24)? != ty {
            return None;
        }
        let lat_min = read_f64(32)?;
        let lat_max = read_f64(40)?;
        let lon_min = read_f64(48)?;
        let lon_max = read_f64(56)?;
        let width = read_u32(64)?;
        let height = read_u32(68)?;
        if read_u32(72)? as usize != PROBE_SIDE {
            return None;
        }
        let checksum = read_u64(76)?;

        let mut probe_samples = [0.0f32; PROBE_LEN];
        for (index, sample) in probe_samples.iter_mut().enumerate() {
            *sample = read_f32(84 + index * 4)?;
        }

        if width != resolution
            || height != resolution
            || (lat_max - lat_min - tile_deg).abs() > 1e-9
            || (lon_max - lon_min - tile_deg).abs() > 1e-9
        {
            return None;
        }

        let samples = (width as usize).checked_mul(height as usize)?;
        let payload_len = samples.checked_mul(4)? * CHANNELS;
        if bytes.len() != HEADER_BYTES + payload_len {
            return None;
        }
        let payload = &bytes[HEADER_BYTES..];
        if payload_checksum(payload) != checksum {
            return None;
        }

        let mut channels = [&[][..]; CHANNELS];
        for (index, channel) in channels.iter_mut().enumerate() {
            let start = index * samples * 4;
            *channel = &payload[start..start + samples * 4];
        }
        let decode_channel = |raw: &[u8]| -> Vec<f32> {
            raw.as_chunks::<4>()
                .0
                .iter()
                .map(|bytes| f32::from_le_bytes(*bytes))
                .collect()
        };
        let data = decode_channel(channels[0]);
        let flow = decode_channel(channels[1]);
        let moisture = decode_channel(channels[2]);
        if data
            .iter()
            .chain(flow.iter())
            .chain(moisture.iter())
            .any(|value| !value.is_finite())
            || moisture.iter().any(|value| !(0.0..=1.0).contains(value))
        {
            return None;
        }

        Some((
            HeightRaster {
                lat_min,
                lat_max,
                lon_min,
                lon_max,
                width,
                height,
                data,
                flow,
                moisture,
            },
            BaseProbe(probe_samples),
        ))
    }

    /// Persist a baked tile atomically (write-then-rename) so a crash cannot
    /// leave a half-written file that later reads as valid. The tile key is
    /// supplied explicitly rather than re-derived from the bounds.
    ///
    /// No `fsync` is issued: this is a regenerable cache, so losing the newest
    /// tiles on a crash is harmless, while syncing thousands of tiles would
    /// dominate bake time.
    pub fn save(&self, tx: i64, ty: i64, tile: &HeightRaster, probe: BaseProbe) -> io::Result<()> {
        if !raster_is_consistent(tile) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "refusing to persist an inconsistent erosion raster",
            ));
        }
        let path = self.path_for(tx, ty);
        let samples = (tile.width as usize) * (tile.height as usize);
        let mut buffer = Vec::with_capacity(HEADER_BYTES + CHANNELS * samples * 4);
        buffer.extend_from_slice(&MAGIC);
        buffer.extend_from_slice(&VERSION.to_le_bytes());
        buffer.extend_from_slice(&self.signature.to_le_bytes());
        buffer.extend_from_slice(&tx.to_le_bytes());
        buffer.extend_from_slice(&ty.to_le_bytes());
        buffer.extend_from_slice(&tile.lat_min.to_le_bytes());
        buffer.extend_from_slice(&tile.lat_max.to_le_bytes());
        buffer.extend_from_slice(&tile.lon_min.to_le_bytes());
        buffer.extend_from_slice(&tile.lon_max.to_le_bytes());
        buffer.extend_from_slice(&tile.width.to_le_bytes());
        buffer.extend_from_slice(&tile.height.to_le_bytes());
        buffer.extend_from_slice(&(PROBE_SIDE as u32).to_le_bytes());
        // The checksum field is written after the payload is serialized.
        let checksum_offset = buffer.len();
        buffer.extend_from_slice(&0u64.to_le_bytes());
        for value in probe.0 {
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        let payload_start = buffer.len();
        for channel in [&tile.data, &tile.flow, &tile.moisture] {
            for value in channel {
                buffer.extend_from_slice(&value.to_le_bytes());
            }
        }
        let checksum = payload_checksum(&buffer[payload_start..]);
        buffer[checksum_offset..checksum_offset + 8].copy_from_slice(&checksum.to_le_bytes());

        let nonce = self.temp_counter.fetch_add(1, Ordering::Relaxed);
        let temp = self
            .root
            .join(format!("{tx}_{ty}.{}.{nonce}.tmp", process::id()));
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&buffer)?;
        }
        fs::rename(&temp, &path)
    }

    /// Delete the oldest tiles (by modification time) until the cache is within
    /// `max_bytes`. Best-effort: a concurrent writer's file may be removed, but
    /// every tile is regenerable. Leftover temporary files are always removed.
    pub fn enforce_budget(&self, max_bytes: u64) -> io::Result<()> {
        let mut entries: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        let mut total: u64 = 0;
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) == Some("tmp") {
                let _ = fs::remove_file(&path);
                continue;
            }
            let size = metadata.len();
            let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
            total += size;
            entries.push((modified, size, path));
        }
        if total <= max_bytes {
            return Ok(());
        }
        entries.sort_by_key(|(modified, _, _)| *modified);
        // Leave headroom so the next run does not immediately re-prune.
        let target = max_bytes / 10 * 9;
        for (_, size, path) in entries {
            if total <= target {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}

fn payload_checksum(payload: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in payload {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
    }
    hash
}

fn raster_is_consistent(tile: &HeightRaster) -> bool {
    let expected = (tile.width as usize).saturating_mul(tile.height as usize);
    tile.width >= 2
        && tile.height >= 2
        && tile.data.len() == expected
        && tile.flow.len() == expected
        && tile.moisture.len() == expected
        && tile.lat_min.is_finite()
        && tile.lat_max.is_finite()
        && tile.lon_min.is_finite()
        && tile.lon_max.is_finite()
        && tile.lat_max > tile.lat_min
        && tile.lon_max > tile.lon_min
}

impl HeightRaster {
    /// The tile key `(tx, ty)` implied by this raster's bounds on the given grid.
    pub fn tile_key(&self, tile_deg: f64) -> (i64, i64) {
        (
            (self.lon_min / tile_deg).floor() as i64,
            (self.lat_min / tile_deg).floor() as i64,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::terrain_source::ProceduralTerrainSource;
    use std::env;

    fn raster(tile_deg: f64, resolution: u32, fill: f32) -> HeightRaster {
        let n = (resolution * resolution) as usize;
        HeightRaster {
            lat_min: 10.0,
            lat_max: 10.0 + tile_deg,
            lon_min: 20.0,
            lon_max: 20.0 + tile_deg,
            width: resolution,
            height: resolution,
            data: vec![fill; n],
            flow: vec![fill + 1.0; n],
            moisture: vec![0.25; n],
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let mut path = env::temp_dir();
        path.push(format!("cosmic_erosion_store_{name}_{}", process::id()));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn round_trips_every_channel_and_bounds() {
        let root = temp_dir("roundtrip");
        let store = ErosionTileStore::open(&root, 0xABCD).expect("open");
        let tile = raster(2.0, 4, 12.5);
        let probe = BaseProbe([1.0; PROBE_LEN]);
        let (tx, ty) = tile.tile_key(2.0);
        store.save(tx, ty, &tile, probe).expect("save");

        let (loaded, loaded_probe) = store.load(tx, ty, 4, 2.0).expect("hit");
        assert_eq!(loaded.data, tile.data);
        assert_eq!(loaded.flow, tile.flow);
        assert_eq!(loaded.moisture, tile.moisture);
        assert_eq!(loaded.lat_min, tile.lat_min);
        assert_eq!(loaded.lat_max, tile.lat_max);
        assert_eq!(loaded.lon_min, tile.lon_min);
        assert_eq!(loaded.lon_max, tile.lon_max);
        assert_eq!(loaded_probe, probe);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_a_different_signature() {
        let root = temp_dir("signature");
        let writer = ErosionTileStore::open(&root, 1).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let (tx, ty) = tile.tile_key(2.0);
        writer
            .save(tx, ty, &tile, BaseProbe([0.0; PROBE_LEN]))
            .expect("save");

        // A different signature resolves to a different namespace directory,
        // so the tile is simply not found.
        let reader = ErosionTileStore::open(&root, 2).expect("open");
        assert!(reader.load(tx, ty, 4, 2.0).is_none());

        // A signature mismatch in an existing file is also rejected.
        let bytes = fs::read(writer.path_for(tx, ty)).expect("read");
        assert!(ErosionTileStore::decode(&bytes, 2, tx, ty, 4, 2.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_a_resolution_mismatch() {
        let root = temp_dir("resolution");
        let store = ErosionTileStore::open(&root, 7).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let (tx, ty) = tile.tile_key(2.0);
        store
            .save(tx, ty, &tile, BaseProbe([0.0; PROBE_LEN]))
            .expect("save");
        assert!(store.load(tx, ty, 8, 2.0).is_none());
        assert!(store.load(tx, ty, 4, 1.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_a_wrong_tile_key() {
        let root = temp_dir("key");
        let store = ErosionTileStore::open(&root, 5).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let (tx, ty) = tile.tile_key(2.0);
        store
            .save(tx, ty, &tile, BaseProbe([0.0; PROBE_LEN]))
            .expect("save");
        // Reading the same bytes under a different key must miss.
        let bytes = fs::read(store.path_for(tx, ty)).expect("read");
        assert!(ErosionTileStore::decode(&bytes, 5, tx + 1, ty, 4, 2.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_a_corrupt_payload() {
        let root = temp_dir("checksum");
        let store = ErosionTileStore::open(&root, 9).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let (tx, ty) = tile.tile_key(2.0);
        store
            .save(tx, ty, &tile, BaseProbe([0.0; PROBE_LEN]))
            .expect("save");

        let mut bytes = fs::read(store.path_for(tx, ty)).expect("read");
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;
        assert!(ErosionTileStore::decode(&bytes, 9, tx, ty, 4, 2.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_truncated_and_oversized_files() {
        let root = temp_dir("length");
        let store = ErosionTileStore::open(&root, 11).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let (tx, ty) = tile.tile_key(2.0);
        store
            .save(tx, ty, &tile, BaseProbe([0.0; PROBE_LEN]))
            .expect("save");
        let bytes = fs::read(store.path_for(tx, ty)).expect("read");

        assert!(ErosionTileStore::decode(&bytes[..bytes.len() - 1], 11, tx, ty, 4, 2.0).is_none());
        let mut oversized = bytes.clone();
        oversized.push(0);
        assert!(ErosionTileStore::decode(&oversized, 11, tx, ty, 4, 2.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_tile_is_a_clean_miss() {
        let root = temp_dir("missing");
        let store = ErosionTileStore::open(&root, 0).expect("open");
        assert!(store.load(123, 456, 4, 2.0).is_none());
        assert!(!store.contains(123, 456));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_file_is_a_clean_miss() {
        let root = temp_dir("corrupt");
        let store = ErosionTileStore::open(&root, 3).expect("open");
        fs::write(store.path_for(0, 0), b"not a tile").expect("write");
        assert!(store.load(0, 0, 4, 2.0).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn base_probe_tolerance_accepts_rounding_but_not_a_different_base() {
        let a = BaseProbe([100.0; PROBE_LEN]);
        let mut close = a;
        close.0[0] = 100.0005;
        assert!(a.matches(close));
        let mut different = a;
        different.0[PROBE_LEN - 1] = 100.5;
        assert!(!a.matches(different));
    }

    #[test]
    fn base_probe_samples_the_live_base_across_the_tile() {
        let base = ProceduralTerrainSource::new(7, 2_000.0, 1_200.0, 0);
        let probe = BaseProbe::sample(&base, 10.0, 12.0, 20.0, 22.0);
        assert_eq!(probe.0[0], base.height_m(10.0, 20.0) as f32);
        assert_eq!(probe.0[PROBE_LEN - 1], base.height_m(12.0, 22.0) as f32);
    }

    #[test]
    fn enforce_budget_prunes_oldest_tiles() {
        let root = temp_dir("budget");
        let store = ErosionTileStore::open(&root, 21).expect("open");
        let tile = raster(2.0, 4, 1.0);
        let per_tile = tile_bytes(4);
        for tx in 0..10 {
            store
                .save(tx, 5, &tile, BaseProbe([0.0; PROBE_LEN]))
                .expect("save");
        }
        assert_eq!(fs::read_dir(store.root()).expect("read").count(), 10);

        store.enforce_budget(per_tile * 5).expect("prune");
        let remaining = fs::read_dir(store.root()).expect("read").count();
        assert!(remaining <= 5, "budget must bound residency: {remaining}");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn enforce_budget_removes_leftover_temp_files() {
        let root = temp_dir("temp");
        let store = ErosionTileStore::open(&root, 31).expect("open");
        let leftover = store.root().join("5_5.123.0.tmp");
        fs::write(&leftover, b"partial").expect("write");

        store.enforce_budget(u64::MAX).expect("prune");
        assert!(!leftover.exists(), "stale temp files must be removed");
        let _ = fs::remove_dir_all(&root);
    }
}
