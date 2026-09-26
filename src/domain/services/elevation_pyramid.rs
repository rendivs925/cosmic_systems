//! Versioned cube-sphere elevation tile payloads and the tile-backed terrain
//! source that composes them over a resident base (AGENTS.md 20-21, 44).
//!
//! A payload package is an offline-prepared, provenance-recorded cube-sphere
//! elevation pyramid. Each tile stores signed metre elevations for one
//! [`TerrainPatch`] and declares its coverage, elevation bounds, and geometric
//! error. The runtime [`TileElevationSource`] always exposes a resident base
//! and samples installed tiles only, so collision, radar altitude, and physics
//! never load, decode, or block on a payload.

use super::cube_sphere::{face_uv, PatchGeometricError, TerrainPatch};
use super::terrain_source::{ElevationBounds, TerrainSource};
use crate::domain::math::DVec3;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Current on-disk elevation payload format version.
pub const ELEVATION_PYRAMID_FORMAT_VERSION: u32 = 1;

/// File name of the package index inside a manifest root directory.
pub const ELEVATION_MANIFEST_FILE_NAME: &str = "manifest.ron";

/// Default bound on decoded elevation tiles held resident by a
/// [`TileElevationSource`]. Tiles are small (a few tens of KiB at the reviewed
/// 64x64 payload resolution), so this bounds the residency without competing
/// with the geometry cache budget.
pub const DEFAULT_ELEVATION_TILE_CAPACITY: usize = 64;

const TILE_MAGIC: [u8; 8] = *b"CSTILE\0\0";
const TILE_HEADER_BYTES: usize = 92;
/// Defensive upper bound so a corrupt manifest cannot drive an enormous shift
/// or tile index.
const MAX_PAYLOAD_LEVEL: u32 = 20;

/// Failures when validating or decoding an elevation payload package.
#[derive(Debug)]
pub enum ElevationPyramidError {
    Io(std::io::Error),
    InvalidFormat(String),
}

impl From<std::io::Error> for ElevationPyramidError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Address of one elevation payload tile within a package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElevationTileMetadata {
    pub patch: TerrainPatch,
    pub width: u32,
    pub height: u32,
    pub min_elevation_m: f64,
    pub max_elevation_m: f64,
    pub geometric_error_m: f64,
    pub payload_path: String,
    pub payload_sha256: String,
}

impl ElevationTileMetadata {
    /// Conservative geometric error for a patch fully covered by this tile.
    pub fn patch_geometric_error(&self) -> PatchGeometricError {
        PatchGeometricError::from_elevation_bounds(self.min_elevation_m, self.max_elevation_m)
    }
}

/// Provenance and tile index for one offline elevation payload package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElevationPyramidManifest {
    pub format_version: u32,
    pub body: String,
    pub coordinate_frame: String,
    pub vertical_datum: String,
    pub source: String,
    pub license: String,
    pub source_sha256: String,
    pub resolution_m: f64,
    pub tiles: Vec<ElevationTileMetadata>,
}

impl ElevationPyramidManifest {
    /// Parse a manifest from RON and validate it.
    pub fn from_ron_str(text: &str) -> Result<Self, ElevationPyramidError> {
        let manifest: Self = ron::from_str(text).map_err(|error| {
            ElevationPyramidError::InvalidFormat(format!("invalid elevation manifest: {error}"))
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Parse and validate a manifest at `path`.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ElevationPyramidError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_ron_str(&text)
    }

    /// Encode the manifest as RON.
    pub fn to_ron_string(&self) -> Result<String, ElevationPyramidError> {
        ron::ser::to_string_pretty(self, ron::ser::PrettyConfig::default()).map_err(|error| {
            ElevationPyramidError::InvalidFormat(format!("unable to encode manifest: {error}"))
        })
    }

    /// Reject malformed or non-physical metadata before any tile is used.
    pub fn validate(&self) -> Result<(), ElevationPyramidError> {
        let invalid = |message: &str| ElevationPyramidError::InvalidFormat(message.to_string());
        if self.format_version != ELEVATION_PYRAMID_FORMAT_VERSION {
            return Err(invalid("unsupported elevation payload version"));
        }
        if self.body.is_empty()
            || self.coordinate_frame.is_empty()
            || self.vertical_datum.is_empty()
            || self.source.is_empty()
            || self.license.is_empty()
        {
            return Err(invalid("elevation manifest provenance is incomplete"));
        }
        if !is_sha256_hex(&self.source_sha256) {
            return Err(invalid("elevation source checksum is invalid"));
        }
        if !self.resolution_m.is_finite() || self.resolution_m <= 0.0 {
            return Err(invalid("elevation resolution must be finite and positive"));
        }
        if self.tiles.is_empty() {
            return Err(invalid("elevation manifest contains no tiles"));
        }

        let mut seen: HashSet<TerrainPatch> = HashSet::with_capacity(self.tiles.len());
        for tile in &self.tiles {
            let span = 1u64 << tile.patch.level.min(63);
            if tile.patch.level > MAX_PAYLOAD_LEVEL
                || u64::from(tile.patch.tile_x) >= span
                || u64::from(tile.patch.tile_y) >= span
            {
                return Err(invalid("elevation tile address is out of range"));
            }
            if tile.width < 2 || tile.height < 2 {
                return Err(invalid(
                    "elevation tile must be at least two samples per side",
                ));
            }
            if !tile.min_elevation_m.is_finite()
                || !tile.max_elevation_m.is_finite()
                || tile.min_elevation_m > tile.max_elevation_m
            {
                return Err(invalid("elevation tile bounds are invalid"));
            }
            if !tile.geometric_error_m.is_finite() || tile.geometric_error_m < 0.0 {
                return Err(invalid("elevation tile geometric error is invalid"));
            }
            if tile.payload_path.is_empty() || !is_sha256_hex(&tile.payload_sha256) {
                return Err(invalid("elevation tile payload reference is invalid"));
            }
            if !seen.insert(tile.patch) {
                return Err(invalid("elevation manifest contains a duplicate tile"));
            }
        }
        Ok(())
    }

    /// Conservative global elevation interval across all declared tiles.
    pub fn global_bounds(&self) -> ElevationBounds {
        self.tiles.iter().fold(
            ElevationBounds::new(f64::INFINITY, f64::NEG_INFINITY),
            |bounds, tile| {
                ElevationBounds::new(
                    bounds.min_m.min(tile.min_elevation_m),
                    bounds.max_m.max(tile.max_elevation_m),
                )
            },
        )
    }

    /// The finest declared tile that covers `patch`, including the patch itself.
    ///
    /// A deeper patch must inherit the tightest available coverage so that LOD
    /// uses the payload's measured relief rather than a coarser ancestor's
    /// envelope.
    pub fn covering_tile(&self, patch: &TerrainPatch) -> Option<&ElevationTileMetadata> {
        self.tiles
            .iter()
            .filter(|tile| tile.patch.is_ancestor_of(patch))
            .max_by_key(|tile| tile.patch.level)
    }

    /// Resolve a tile's payload path relative to the package root.
    pub fn resolved_payload_path(
        &self,
        root: impl AsRef<Path>,
        tile: &ElevationTileMetadata,
    ) -> std::path::PathBuf {
        root.as_ref().join(&tile.payload_path)
    }
}

/// One decoded elevation payload tile: metre samples on a face-UV grid.
#[derive(Debug, Clone, PartialEq)]
pub struct ElevationTile {
    patch: TerrainPatch,
    width: u32,
    height: u32,
    min_elevation_m: f64,
    max_elevation_m: f64,
    geometric_error_m: f64,
    samples_m: Vec<f32>,
}

impl ElevationTile {
    pub fn from_samples(
        patch: TerrainPatch,
        width: u32,
        height: u32,
        min_elevation_m: f64,
        max_elevation_m: f64,
        geometric_error_m: f64,
        samples_m: Vec<f32>,
    ) -> Result<Self, ElevationPyramidError> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                ElevationPyramidError::InvalidFormat("tile dimensions overflow".into())
            })?;
        if width < 2
            || height < 2
            || samples_m.len() != expected
            || !min_elevation_m.is_finite()
            || !max_elevation_m.is_finite()
            || min_elevation_m > max_elevation_m
            || !geometric_error_m.is_finite()
            || geometric_error_m < 0.0
            || samples_m.iter().any(|value| !value.is_finite())
        {
            return Err(ElevationPyramidError::InvalidFormat(
                "invalid elevation tile samples".into(),
            ));
        }
        Ok(Self {
            patch,
            width,
            height,
            min_elevation_m,
            max_elevation_m,
            geometric_error_m,
            samples_m,
        })
    }

    pub fn patch(&self) -> TerrainPatch {
        self.patch
    }

    pub fn elevation_bounds_m(&self) -> (f64, f64) {
        (self.min_elevation_m, self.max_elevation_m)
    }

    pub fn geometric_error_m(&self) -> f64 {
        self.geometric_error_m
    }

    pub fn metadata(&self, payload_path: String) -> ElevationTileMetadata {
        ElevationTileMetadata {
            patch: self.patch,
            width: self.width,
            height: self.height,
            min_elevation_m: self.min_elevation_m,
            max_elevation_m: self.max_elevation_m,
            geometric_error_m: self.geometric_error_m,
            payload_path,
            payload_sha256: hex_sha256(&self.payload_bytes()),
        }
    }

    fn payload_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.samples_m.len() * 4);
        for value in &self.samples_m {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// Encode the tile with a fixed header and a payload checksum.
    pub fn to_bytes(&self) -> Vec<u8> {
        let payload = self.payload_bytes();
        let mut bytes = Vec::with_capacity(TILE_HEADER_BYTES + payload.len());
        bytes.extend_from_slice(&TILE_MAGIC);
        bytes.extend_from_slice(&ELEVATION_PYRAMID_FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&face_index(self.patch.face).to_le_bytes());
        bytes.extend_from_slice(&self.patch.level.to_le_bytes());
        bytes.extend_from_slice(&self.patch.tile_x.to_le_bytes());
        bytes.extend_from_slice(&self.patch.tile_y.to_le_bytes());
        bytes.extend_from_slice(&self.width.to_le_bytes());
        bytes.extend_from_slice(&self.height.to_le_bytes());
        bytes.extend_from_slice(&self.min_elevation_m.to_le_bytes());
        bytes.extend_from_slice(&self.max_elevation_m.to_le_bytes());
        bytes.extend_from_slice(&self.geometric_error_m.to_le_bytes());
        bytes.extend_from_slice(&Sha256::digest(&payload));
        bytes.extend_from_slice(&payload);
        bytes
    }

    /// Decode and checksum a tile payload, rejecting unsupported versions.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ElevationPyramidError> {
        if bytes.len() < TILE_HEADER_BYTES || bytes[..8] != TILE_MAGIC {
            return Err(ElevationPyramidError::InvalidFormat(
                "invalid elevation tile header".into(),
            ));
        }
        let u32_at =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let f64_at =
            |offset: usize| f64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap());
        if u32_at(8) != ELEVATION_PYRAMID_FORMAT_VERSION {
            return Err(ElevationPyramidError::InvalidFormat(
                "unsupported elevation tile version".into(),
            ));
        }
        let face = face_from_index(u32_at(12))?;
        let patch = TerrainPatch {
            face,
            level: u32_at(16),
            tile_x: u32_at(20),
            tile_y: u32_at(24),
        };
        let (width, height) = (u32_at(28), u32_at(32));
        let (min_elevation_m, max_elevation_m, geometric_error_m) =
            (f64_at(36), f64_at(44), f64_at(52));
        let count = (width as usize)
            .checked_mul(height as usize)
            .ok_or_else(|| {
                ElevationPyramidError::InvalidFormat("tile dimensions overflow".into())
            })?;
        let payload_bytes = count
            .checked_mul(4)
            .ok_or_else(|| ElevationPyramidError::InvalidFormat("tile payload overflows".into()))?;
        if bytes.len() != TILE_HEADER_BYTES + payload_bytes {
            return Err(ElevationPyramidError::InvalidFormat(
                "invalid elevation tile payload length".into(),
            ));
        }
        let payload = &bytes[TILE_HEADER_BYTES..];
        if Sha256::digest(payload).as_slice() != &bytes[60..92] {
            return Err(ElevationPyramidError::InvalidFormat(
                "elevation tile checksum mismatch".into(),
            ));
        }
        let samples_m = payload
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        Self::from_samples(
            patch,
            width,
            height,
            min_elevation_m,
            max_elevation_m,
            geometric_error_m,
            samples_m,
        )
    }

    /// Bilinear sample at face UV, or `None` outside the tile's bounds.
    pub fn sample_uv(&self, u: f64, v: f64) -> Option<f64> {
        let (u0, v0, u1, v1) = self.patch.uv_bounds();
        if !(u0..=u1).contains(&u) || !(v0..=v1).contains(&v) {
            return None;
        }
        let span_u = (u1 - u0).max(f64::EPSILON);
        let span_v = (v1 - v0).max(f64::EPSILON);
        let x = (u - u0) / span_u * (self.width - 1) as f64;
        let y = (v - v0) / span_v * (self.height - 1) as f64;
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = (
            (x0 + 1).min(self.width as usize - 1),
            (y0 + 1).min(self.height as usize - 1),
        );
        let at = |x: usize, y: usize| f64::from(self.samples_m[y * self.width as usize + x]);
        let (a, b, c, d) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
        let tx = x - x0 as f64;
        let ty = y - y0 as f64;
        Some((a + (b - a) * tx) * (1.0 - ty) + (c + (d - c) * tx) * ty)
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn face_index(face: super::cube_sphere::CubeFace) -> u32 {
    super::cube_sphere::CubeFace::ALL
        .iter()
        .position(|candidate| *candidate == face)
        .expect("cube face must be in CubeFace::ALL") as u32
}

fn face_from_index(index: u32) -> Result<super::cube_sphere::CubeFace, ElevationPyramidError> {
    super::cube_sphere::CubeFace::ALL
        .get(index as usize)
        .copied()
        .ok_or_else(|| ElevationPyramidError::InvalidFormat("invalid cube face index".into()))
}

/// Bounded, deterministic LRU of decoded elevation tiles. The vector is the
/// complete recency order so eviction never depends on hash-map iteration.
#[derive(Debug, Default)]
struct TileCache {
    tiles: HashMap<TerrainPatch, Arc<ElevationTile>>,
    order: Vec<TerrainPatch>,
    levels: BTreeSet<u32>,
}

impl TileCache {
    fn get(&mut self, patch: TerrainPatch) -> Option<Arc<ElevationTile>> {
        let tile = self.tiles.get(&patch).cloned();
        if tile.is_some() {
            if let Some(index) = self.order.iter().position(|candidate| *candidate == patch) {
                self.order.remove(index);
                self.order.push(patch);
            }
        }
        tile
    }

    fn insert(&mut self, tile: Arc<ElevationTile>, capacity: usize) {
        let patch = tile.patch();
        let capacity = capacity.max(1);
        while self.tiles.len() >= capacity && !self.tiles.contains_key(&patch) {
            let lru = self.order.remove(0);
            self.tiles.remove(&lru);
        }
        self.order.retain(|candidate| *candidate != patch);
        self.order.push(patch);
        self.tiles.insert(patch, tile);
        self.rebuild_levels();
    }

    fn rebuild_levels(&mut self) {
        self.levels = self.tiles.keys().map(|patch| patch.level).collect();
    }
}

/// A [`TerrainSource`] that composes a resident base with installed high
/// resolution payload tiles. `height_m` reads resident data only; it never
/// loads or decodes a tile, so fixed-step collision cannot block on payload I/O.
pub struct TileElevationSource {
    base: Arc<dyn TerrainSource>,
    manifest: ElevationPyramidManifest,
    /// Package root used to resolve each declared tile's relative payload path.
    /// Empty for an in-memory manifest whose paths already resolve.
    root: PathBuf,
    /// `patch -> manifest.tiles` index so a covering-tile lookup climbs at most
    /// one address per level instead of scanning the whole package index.
    tile_index: HashMap<TerrainPatch, usize>,
    /// Finest declared tile level; the anchor for direction-based lookups.
    max_tile_level: u32,
    cache: Mutex<TileCache>,
    capacity: usize,
}

impl Debug for TileElevationSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TileElevationSource")
            .field("body", &self.manifest.body)
            .field("tiles", &self.manifest.tiles.len())
            .field("capacity", &self.capacity)
            .finish()
    }
}

impl TileElevationSource {
    /// Build a source over a resident base. The manifest is validated here.
    pub fn new(
        base: Arc<dyn TerrainSource>,
        manifest: ElevationPyramidManifest,
        capacity: usize,
    ) -> Result<Self, ElevationPyramidError> {
        Self::with_root(base, manifest, PathBuf::new(), capacity)
    }

    /// Load a package from `root` (a directory holding `manifest.ron` and its
    /// relative tile payloads) over a resident base. The manifest is read and
    /// validated here; no tile is decoded.
    pub fn from_manifest_root(
        base: Arc<dyn TerrainSource>,
        root: impl AsRef<Path>,
        capacity: usize,
    ) -> Result<Self, ElevationPyramidError> {
        let root = root.as_ref().to_path_buf();
        let manifest =
            ElevationPyramidManifest::from_path(root.join(ELEVATION_MANIFEST_FILE_NAME))?;
        Self::with_root(base, manifest, root, capacity)
    }

    fn with_root(
        base: Arc<dyn TerrainSource>,
        manifest: ElevationPyramidManifest,
        root: PathBuf,
        capacity: usize,
    ) -> Result<Self, ElevationPyramidError> {
        manifest.validate()?;
        let tile_index = manifest
            .tiles
            .iter()
            .enumerate()
            .map(|(index, tile)| (tile.patch, index))
            .collect();
        let max_tile_level = manifest
            .tiles
            .iter()
            .map(|tile| tile.patch.level)
            .max()
            .unwrap_or(0);
        Ok(Self {
            base,
            manifest,
            root,
            tile_index,
            max_tile_level,
            cache: Mutex::new(TileCache::default()),
            capacity: capacity.max(1),
        })
    }

    pub fn manifest(&self) -> &ElevationPyramidManifest {
        &self.manifest
    }

    /// The finest declared tile that covers `patch`, or `None` when the package
    /// has no coverage for it. Climbing one level per lookup keeps this usable
    /// from the per-patch LOD error path even for large packages.
    pub fn covering_tile_for(&self, patch: &TerrainPatch) -> Option<&ElevationTileMetadata> {
        let mut current = Some(*patch);
        while let Some(candidate) = current {
            if let Some(index) = self.tile_index.get(&candidate) {
                return self.manifest.tiles.get(*index);
            }
            current = candidate.parent();
        }
        None
    }

    /// The finest declared tile covering a body-fixed direction.
    pub fn covering_tile_for_direction(&self, direction: DVec3) -> Option<&ElevationTileMetadata> {
        let patch = TerrainPatch::for_direction(direction, self.max_tile_level);
        self.covering_tile_for(&patch)
    }

    /// Package-relative payload path for the tile covering `patch`.
    pub fn resolved_payload_path(&self, patch: &TerrainPatch) -> Option<PathBuf> {
        self.covering_tile_for(patch)
            .map(|tile| self.manifest.resolved_payload_path(&self.root, tile))
    }

    pub fn is_resident(&self, patch: TerrainPatch) -> bool {
        self.cache
            .lock()
            .expect("elevation tile cache lock")
            .tiles
            .contains_key(&patch)
    }

    /// Install a decoded tile into the bounded resident set. Presentation and
    /// streaming call this from worker tasks; it never blocks a terrain query.
    pub fn install_tile(&self, tile: ElevationTile) {
        self.cache
            .lock()
            .expect("elevation tile cache lock")
            .insert(Arc::new(tile), self.capacity);
    }

    pub fn resident_tile_count(&self) -> usize {
        self.cache
            .lock()
            .expect("elevation tile cache lock")
            .tiles
            .len()
    }

    /// Sample the finest resident tile covering a body-fixed direction.
    pub fn sample_resident(&self, direction: DVec3) -> Option<f64> {
        let (face, u, v) = face_uv(direction);
        let mut cache = self.cache.lock().expect("elevation tile cache lock");
        let levels: Vec<u32> = cache.levels.iter().rev().copied().collect();
        for level in levels {
            let span = 1u64 << level.min(63);
            let tile_x = (u * span as f64).floor().clamp(0.0, (span - 1) as f64) as u32;
            let tile_y = (v * span as f64).floor().clamp(0.0, (span - 1) as f64) as u32;
            let patch = TerrainPatch {
                face,
                level,
                tile_x,
                tile_y,
            };
            if let Some(tile) = cache.get(patch) {
                if let Some(height_m) = tile.sample_uv(u, v) {
                    return Some(height_m);
                }
            }
        }
        None
    }
}

impl TerrainSource for TileElevationSource {
    fn height_m(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        let Some(direction) = terrain_direction(latitude_deg, longitude_deg) else {
            return self.base.height_m(latitude_deg, longitude_deg);
        };
        self.sample_resident(direction)
            .unwrap_or_else(|| self.base.height_m(latitude_deg, longitude_deg))
    }

    fn elevation_bounds_m(&self) -> ElevationBounds {
        let base = self.base.elevation_bounds_m();
        let payload = self.manifest.global_bounds();
        ElevationBounds::new(base.min_m.min(payload.min_m), base.max_m.max(payload.max_m))
    }

    fn patch_geometric_error(&self, patch: &TerrainPatch) -> PatchGeometricError {
        match self.covering_tile_for(patch) {
            Some(tile) => tile.patch_geometric_error(),
            None => self.base.patch_geometric_error(patch),
        }
    }

    fn mesh_height_m(&self, latitude_deg: f64, longitude_deg: f64, _patch_level: u32) -> f64 {
        self.height_m(latitude_deg, longitude_deg)
    }

    fn prepare_sample(&self, latitude_deg: f64, longitude_deg: f64) {
        self.base.prepare_sample(latitude_deg, longitude_deg);
    }

    fn moisture(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.moisture(latitude_deg, longitude_deg)
    }

    fn river_strength(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.river_strength(latitude_deg, longitude_deg)
    }

    fn vegetation_density(&self, latitude_deg: f64, longitude_deg: f64) -> f64 {
        self.base.vegetation_density(latitude_deg, longitude_deg)
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

/// Terrain-radial latitude/longitude to body-fixed direction, matching the DEM
/// source and mesh convention. Returns `None` for non-finite input.
fn terrain_direction(latitude_deg: f64, longitude_deg: f64) -> Option<DVec3> {
    if !latitude_deg.is_finite() || !longitude_deg.is_finite() {
        return None;
    }
    let latitude_rad = latitude_deg.clamp(-90.0, 90.0).to_radians();
    let longitude_rad = longitude_deg.to_radians();
    Some(DVec3::new(
        latitude_rad.cos() * longitude_rad.cos(),
        latitude_rad.sin(),
        latitude_rad.cos() * longitude_rad.sin(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::{CubeFace, TerrainPatch};
    use crate::domain::services::terrain_source::ProceduralTerrainSource;
    use std::sync::Arc;

    fn base() -> Arc<dyn TerrainSource> {
        Arc::new(ProceduralTerrainSource::new(7, 2_000.0, 1_200.0, 0))
    }

    fn tile_at(patch: TerrainPatch, height_m: f32, error_m: f64) -> ElevationTile {
        ElevationTile::from_samples(
            patch,
            3,
            3,
            f64::from(height_m),
            f64::from(height_m),
            error_m,
            vec![height_m; 9],
        )
        .expect("valid synthetic tile")
    }

    fn manifest_with(tiles: Vec<ElevationTileMetadata>) -> ElevationPyramidManifest {
        ElevationPyramidManifest {
            format_version: ELEVATION_PYRAMID_FORMAT_VERSION,
            body: "Earth".into(),
            coordinate_frame: "cube-sphere-face-uv".into(),
            vertical_datum: "MSL".into(),
            source: "test".into(),
            license: "test".into(),
            source_sha256: "a".repeat(64),
            resolution_m: 30.0,
            tiles,
        }
    }

    /// Pid-scoped scratch directory, matching the offline converter tests so
    /// repeated local runs start clean without a tempfile dependency.
    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elevation_pyramid_source_{}_{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch directory");
        dir
    }

    #[test]
    fn manifest_validation_rejects_version_missing_field_and_checksum() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let tile = tile_at(patch, 100.0, 5.0).metadata("tiles/x.cstile".into());

        let mut wrong_version = manifest_with(vec![tile.clone()]);
        wrong_version.format_version = 999;
        assert!(wrong_version.validate().is_err());

        let mut missing_field = manifest_with(vec![tile.clone()]);
        missing_field.body.clear();
        assert!(missing_field.validate().is_err());

        let mut bad_checksum = manifest_with(vec![tile.clone()]);
        bad_checksum.source_sha256 = "not-a-hash".into();
        assert!(bad_checksum.validate().is_err());

        assert!(manifest_with(vec![tile]).validate().is_ok());
    }

    #[test]
    fn manifest_validation_rejects_duplicate_and_out_of_range_tiles() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let tile = tile_at(patch, 100.0, 5.0).metadata("tiles/x.cstile".into());
        assert!(manifest_with(vec![tile.clone(), tile]).validate().is_err());

        let mut out_of_range = tile_at(patch, 100.0, 5.0).metadata("tiles/x.cstile".into());
        out_of_range.patch = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 9,
            tile_y: 0,
        };
        assert!(manifest_with(vec![out_of_range]).validate().is_err());
    }

    #[test]
    fn tile_roundtrip_is_byte_deterministic_and_rejects_corruption() {
        let patch = TerrainPatch {
            face: CubeFace::NegX,
            level: 12,
            tile_x: 409,
            tile_y: 951,
        };
        let tile = ElevationTile::from_samples(
            patch,
            2,
            2,
            0.0,
            300.0,
            12.0,
            vec![0.0, 100.0, 200.0, 300.0],
        )
        .expect("valid tile");
        let bytes = tile.to_bytes();
        assert_eq!(bytes, tile.to_bytes(), "encoding must be deterministic");
        let decoded = ElevationTile::from_bytes(&bytes).expect("roundtrip");
        assert_eq!(decoded, tile);

        let mut corrupted = bytes.clone();
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0xFF;
        assert!(ElevationTile::from_bytes(&corrupted).is_err());

        let mut wrong_version = bytes.clone();
        wrong_version[8..12].copy_from_slice(&999u32.to_le_bytes());
        assert!(ElevationTile::from_bytes(&wrong_version).is_err());
    }

    #[test]
    fn tile_bilinear_sample_interpolates_within_its_bounds() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let tile =
            ElevationTile::from_samples(patch, 2, 2, 0.0, 10.0, 1.0, vec![0.0, 10.0, 10.0, 20.0])
                .expect("valid tile");
        assert_eq!(tile.sample_uv(0.5, 0.5), Some(10.0));
        assert_eq!(tile.sample_uv(0.0, 0.0), Some(0.0));
        assert_eq!(tile.sample_uv(1.0, 1.0), Some(20.0));
    }

    #[test]
    fn source_falls_back_to_base_outside_resident_coverage() {
        let empty = TileElevationSource::new(base(), manifest_with(vec![]), 4);
        assert!(empty.is_err(), "an empty manifest must be rejected");

        // A manifest with an unrelated tile still validates and leaves the base
        // authoritative for positions the tile does not cover.
        let unrelated = TerrainPatch {
            face: CubeFace::NegZ,
            level: 5,
            tile_x: 0,
            tile_y: 0,
        };
        let manifest = manifest_with(vec![
            tile_at(unrelated, 123.0, 4.0).metadata("tiles/n.cstile".into())
        ]);
        let source = TileElevationSource::new(base(), manifest, 4).expect("valid manifest");
        let base_height = base().height_m(10.0, 20.0);
        assert_eq!(source.height_m(10.0, 20.0), base_height);
        assert_eq!(source.resident_tile_count(), 0);
    }

    #[test]
    fn installed_tile_takes_authority_within_its_patch() {
        let patch = TerrainPatch {
            face: CubeFace::PosZ,
            level: 0,
            tile_x: 0,
            tile_y: 0,
        };
        let manifest = manifest_with(vec![
            tile_at(patch, 999.0, 5.0).metadata("tiles/root.cstile".into())
        ]);
        let source = TileElevationSource::new(base(), manifest, 4).expect("valid manifest");

        // Before install the base is authoritative. (0°, 90°) maps to PosZ.
        let before = source.height_m(0.0, 90.0);
        assert_ne!(before, 999.0);

        source.install_tile(tile_at(patch, 999.0, 5.0));
        assert_eq!(source.height_m(0.0, 90.0), 999.0);
        assert_eq!(source.resident_tile_count(), 1);
        // The root patch covers every PosZ direction; other faces still fall
        // back to the base.
        assert_eq!(source.height_m(0.0, 100.0), 999.0);
        assert_ne!(source.height_m(0.0, 0.0), 999.0);
    }

    #[test]
    fn source_is_deterministic_and_geometric_error_uses_manifest() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let manifest = manifest_with(vec![
            tile_at(patch, 500.0, 42.0).metadata("tiles/root.cstile".into())
        ]);
        let source = TileElevationSource::new(base(), manifest, 2).expect("valid manifest");
        assert_eq!(source.height_m(12.0, -34.0), source.height_m(12.0, -34.0));

        let error = source.patch_geometric_error(&patch);
        assert_eq!(error.elevation_range_m, 0.0);
        assert_eq!(error.child_to_parent_deviation_m, 0.0);
    }

    #[test]
    fn cache_eviction_is_deterministic_and_never_changes_samples() {
        let patches = [
            TerrainPatch {
                face: CubeFace::PosZ,
                level: 1,
                tile_x: 0,
                tile_y: 0,
            },
            TerrainPatch {
                face: CubeFace::PosZ,
                level: 1,
                tile_x: 1,
                tile_y: 0,
            },
            TerrainPatch {
                face: CubeFace::PosZ,
                level: 1,
                tile_x: 0,
                tile_y: 1,
            },
        ];
        let tile_metas = patches
            .iter()
            .enumerate()
            .map(|(index, patch)| {
                tile_at(*patch, 100.0 + index as f32, 1.0).metadata(format!("tiles/{index}.cstile"))
            })
            .collect();
        let manifest = manifest_with(tile_metas);
        let source = TileElevationSource::new(base(), manifest, 2).expect("valid manifest");

        source.install_tile(tile_at(patches[0], 100.0, 1.0));
        source.install_tile(tile_at(patches[1], 101.0, 1.0));
        // Touch the second tile so it is the most recently used, leaving the
        // first as the eviction candidate.
        assert_eq!(
            source.sample_resident(crate::domain::services::cube_sphere::face_uv_to_direction(
                CubeFace::PosZ,
                0.75,
                0.25,
            ),),
            Some(101.0)
        );

        // Installing a third tile evicts the least-recently used tile (the
        // first installed) without disturbing the remaining residents.
        source.install_tile(tile_at(patches[2], 102.0, 1.0));
        assert_eq!(source.resident_tile_count(), 2);
        assert_eq!(
            source.sample_resident(crate::domain::services::cube_sphere::face_uv_to_direction(
                CubeFace::PosZ,
                0.25,
                0.25,
            ),),
            None,
            "the least-recently used tile must be evicted"
        );
        assert_eq!(
            source.sample_resident(crate::domain::services::cube_sphere::face_uv_to_direction(
                CubeFace::PosZ,
                0.75,
                0.25,
            ),),
            Some(101.0)
        );
        assert_eq!(
            source.sample_resident(crate::domain::services::cube_sphere::face_uv_to_direction(
                CubeFace::PosZ,
                0.25,
                0.75,
            ),),
            Some(102.0)
        );
    }

    #[test]
    fn adjacent_tile_samples_are_continuous_across_the_shared_edge() {
        use crate::domain::services::cube_sphere::{face_uv_to_direction, CubeFace};

        // A continuous analytic height field, sampled onto two horizontally
        // adjacent tiles. Both tiles compute identical values on the shared
        // edge, so crossing the boundary must not step.
        fn height_m(direction: DVec3) -> f32 {
            (500.0 * direction.z + 1_000.0) as f32
        }
        let make_tile = |tile_x: u32| {
            let patch = TerrainPatch {
                face: CubeFace::PosZ,
                level: 1,
                tile_x,
                tile_y: 0,
            };
            let (u0, v0, u1, v1) = patch.uv_bounds();
            let res = 3usize;
            let mut samples = Vec::with_capacity(res * res);
            for j in 0..res {
                for i in 0..res {
                    let u = u0 + (u1 - u0) * i as f64 / (res as f64 - 1.0);
                    let v = v0 + (v1 - v0) * j as f64 / (res as f64 - 1.0);
                    samples.push(height_m(face_uv_to_direction(patch.face, u, v)));
                }
            }
            ElevationTile::from_samples(patch, res as u32, res as u32, 0.0, 4_000.0, 2.0, samples)
                .expect("valid analytic tile")
        };

        let left = make_tile(0);
        let right = make_tile(1);
        let patch_left = left.patch();
        let patch_right = right.patch();
        let manifest = manifest_with(vec![
            left.metadata("tiles/left.cstile".into()),
            right.metadata("tiles/right.cstile".into()),
        ]);
        let source = TileElevationSource::new(base(), manifest, 4).expect("valid manifest");
        source.install_tile(left);
        source.install_tile(right);

        // The two tiles meet at face u = 0.5. Sample just inside each side.
        let epsilon = 1e-6;
        let left_direction = face_uv_to_direction(patch_left.face, 0.5 - epsilon, 0.25);
        let right_direction = face_uv_to_direction(patch_right.face, 0.5 + epsilon, 0.25);
        let left_height = source.sample_resident(left_direction).expect("left tile");
        let right_height = source.sample_resident(right_direction).expect("right tile");
        assert!(
            (left_height - right_height).abs() < 0.01,
            "tile seam is discontinuous: {left_height} vs {right_height}"
        );
    }

    #[test]
    fn collision_query_does_not_depend_on_concurrent_install() {
        use std::sync::Arc as StdArc;
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let manifest = manifest_with(vec![
            tile_at(patch, 777.0, 3.0).metadata("tiles/root.cstile".into())
        ]);
        let source =
            StdArc::new(TileElevationSource::new(base(), manifest, 4).expect("valid manifest"));

        // A worker installs a tile while the main thread queries height. The
        // query always returns a finite resident-or-base value and never blocks.
        let worker = {
            let source = StdArc::clone(&source);
            std::thread::spawn(move || {
                source.install_tile(tile_at(patch, 777.0, 3.0));
            })
        };
        for _ in 0..1000 {
            let height = source.height_m(0.0, 90.0);
            assert!(height.is_finite());
        }
        worker.join().expect("install worker completes");
        assert_eq!(source.height_m(0.0, 90.0), 777.0);
    }

    #[test]
    fn manifest_root_loads_relative_payloads_and_resolves_coverage() {
        let dir = scratch_dir("root");
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let tile = tile_at(patch, 321.0, 5.0);
        std::fs::create_dir_all(dir.join("tiles")).expect("tiles directory");
        std::fs::write(dir.join("tiles/root.cstile"), tile.to_bytes()).expect("tile payload");
        let manifest = manifest_with(vec![tile.metadata("tiles/root.cstile".into())]);
        std::fs::write(
            dir.join(ELEVATION_MANIFEST_FILE_NAME),
            manifest.to_ron_string().expect("manifest ron"),
        )
        .expect("manifest file");

        let source =
            TileElevationSource::from_manifest_root(base(), &dir, 4).expect("valid package root");
        assert_eq!(
            source.covering_tile_for(&patch).map(|meta| meta.patch),
            Some(patch)
        );
        let path = source
            .resolved_payload_path(&patch)
            .expect("resolved payload path");
        let decoded =
            ElevationTile::from_bytes(&std::fs::read(path).expect("tile bytes")).expect("decode");
        source.install_tile(decoded);
        assert_eq!(source.height_m(0.0, 90.0), 321.0);

        // A root with no manifest is an explicit error, never a silent package.
        let empty = scratch_dir("empty");
        assert!(TileElevationSource::from_manifest_root(base(), &empty, 4).is_err());

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn cache_hit_and_after_eviction_reload_are_identical() {
        let patches = [
            TerrainPatch::root(CubeFace::PosZ),
            TerrainPatch::root(CubeFace::PosX),
        ];
        let tile_metas = patches
            .iter()
            .map(|patch| tile_at(*patch, 250.0, 7.0).metadata("tile.cstile".into()))
            .collect();
        let source =
            TileElevationSource::new(base(), manifest_with(tile_metas), 1).expect("valid manifest");
        let direction =
            crate::domain::services::cube_sphere::face_uv_to_direction(CubeFace::PosZ, 0.5, 0.5);

        source.install_tile(tile_at(patches[0], 250.0, 7.0));
        let hit = source.sample_resident(direction);
        assert_eq!(hit, Some(250.0));

        // Capacity one: installing the sibling evicts the first tile.
        source.install_tile(tile_at(patches[1], 800.0, 7.0));
        assert_eq!(source.sample_resident(direction), None);

        // A fresh reload of the evicted tile reproduces the cache-hit sample.
        source.install_tile(tile_at(patches[0], 250.0, 7.0));
        assert_eq!(source.sample_resident(direction), hit);
    }

    #[test]
    fn adjacent_tiles_agree_along_their_shared_seam() {
        let left = TerrainPatch {
            face: CubeFace::PosZ,
            level: 1,
            tile_x: 0,
            tile_y: 0,
        };
        let right = TerrainPatch {
            face: CubeFace::PosZ,
            level: 1,
            tile_x: 1,
            tile_y: 0,
        };
        // Row-major samples. The right column of `left` equals the left column
        // of `right`, so the shared u = 0.5 edge is identical from both sides.
        let left_tile =
            ElevationTile::from_samples(left, 2, 2, 0.0, 10.0, 1.0, vec![0.0, 10.0, 0.0, 10.0])
                .expect("left tile");
        let right_tile =
            ElevationTile::from_samples(right, 2, 2, 10.0, 20.0, 1.0, vec![10.0, 20.0, 10.0, 20.0])
                .expect("right tile");

        for v in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let seam_left = left_tile.sample_uv(0.5, v * 0.5).expect("left seam");
            let seam_right = right_tile.sample_uv(0.5, v * 0.5).expect("right seam");
            assert_eq!(
                seam_left, seam_right,
                "adjacent tiles must not create a seam at v={v}"
            );
        }
    }

    #[test]
    fn payload_error_replaces_the_base_ceiling_for_covered_deep_patches() {
        let covered = TerrainPatch {
            face: CubeFace::PosZ,
            level: 2,
            tile_x: 1,
            tile_y: 1,
        };
        let mut tile = tile_at(covered, 0.0, 0.0).metadata("tiles/covered.cstile".into());
        tile.min_elevation_m = 100.0;
        tile.max_elevation_m = 140.0;
        let manifest = manifest_with(vec![tile]);
        let source = TileElevationSource::new(base(), manifest, 4).expect("valid manifest");

        // A patch far deeper than the payload level inherits the payload tile's
        // tight error instead of the resident metadata ceiling.
        let deep = TerrainPatch {
            face: CubeFace::PosZ,
            level: 12,
            tile_x: 1_500,
            tile_y: 1_500,
        };
        let error = source.patch_geometric_error(&deep);
        assert_eq!(error.elevation_range_m, 40.0);
        assert_eq!(error.child_to_parent_deviation_m, 40.0);

        // An uncovered patch keeps the resident conservative fallback.
        let uncovered = TerrainPatch::root(CubeFace::NegZ);
        assert_eq!(
            source.patch_geometric_error(&uncovered),
            base().patch_geometric_error(&uncovered)
        );
    }

    #[test]
    fn resident_tile_and_base_fallback_agree_on_authority() {
        let patch = TerrainPatch::root(CubeFace::PosZ);
        let manifest = manifest_with(vec![
            tile_at(patch, 640.0, 3.0).metadata("tiles/root.cstile".into())
        ]);
        let source = TileElevationSource::new(base(), manifest, 4).expect("valid manifest");
        let base_value = base().height_m(0.0, 90.0);

        assert_eq!(source.height_m(0.0, 90.0), base_value);
        source.install_tile(tile_at(patch, 640.0, 3.0));
        assert_eq!(source.height_m(0.0, 90.0), 640.0);
        assert_eq!(source.resident_tile_count(), 1);
    }
}
