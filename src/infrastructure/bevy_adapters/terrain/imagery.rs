//! Earth imagery streaming and material upgrade (presentation-only).
//!
//! Imagery follows the existing terrain patch visibility: only published
//! patches request imagery, obsolete requests are cancelled, and imagery is
//! evicted with its patch. Geometry keeps priority: imagery is admitted within
//! its own bounded budget and never blocks terrain geometry or collision.
//! Imagery is never a terrain-height, altitude, or physics authority.

use crate::domain::services::cube_sphere::TerrainPatch;
use crate::domain::services::imagery_package::{EarthImageryPackage, ImageryResolution};
use crate::domain::services::imagery_tiles::cube_face_name;
use crate::domain::value_objects::imagery_manifest::EarthImageryManifest;
use crate::infrastructure::bevy_adapters::terrain::render::{
    build_terrain_material, TerrainMaterial, TerrainPatchRenderState,
};
use crate::infrastructure::bevy_adapters::terrain::streaming::TerrainStreamingResource;
use bevy::asset::{AssetServer, Handle, LoadState};
use bevy::image::Image;
use bevy::prelude::*;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

/// Manifest that describes the imagery package layout and provenance.
const EARTH_IMAGERY_MANIFEST: &str =
    include_str!("../../../../assets/configs/terrain/earth_imagery_v1.ron");
/// Conservative per-tile GPU/CPU estimate for a produced 256² RGBA tile.
const IMAGERY_TILE_BYTES: u64 = 256 * 256 * 4;

/// Imagery streaming configuration. Disabled or absent imagery falls back to
/// the existing global albedo without touching terrain or collision data.
#[derive(Resource)]
pub(crate) struct TerrainImageryConfig {
    pub(crate) enabled: bool,
    /// Asset-relative package root, under `assets/`.
    pub(crate) asset_root: String,
    pub(crate) budget_bytes: u64,
    pub(crate) max_uploads_per_frame: usize,
}

impl Default for TerrainImageryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            asset_root: "large_files/terrain/earth_imagery_v1".into(),
            budget_bytes: 64 * 1024 * 1024,
            max_uploads_per_frame: 4,
        }
    }
}

/// Cadence-limited imagery residency snapshot folded into terrain streaming
/// metrics.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ImageryMetrics {
    pub(crate) resident_tiles: usize,
    pub(crate) pending_tiles: usize,
    pub(crate) resident_mib: f64,
    pub(crate) budget_mib: f64,
    pub(crate) evicted_tiles: u64,
}

/// Resolved imagery state owned by the terrain streaming lifecycle.
#[derive(Resource, Default)]
pub(crate) struct TerrainImageryResource {
    package: Option<EarthImageryPackage>,
    handles: HashMap<TerrainPatch, Handle<Image>>,
    pending: BTreeSet<TerrainPatch>,
    ready: BTreeSet<TerrainPatch>,
    resident_bytes: u64,
    budget_bytes: u64,
    evictions: u64,
}

impl TerrainImageryResource {
    /// The ready imagery tile for a geometry patch, resolved through the package.
    /// A fine geometry patch uses the most detailed produced tile at or coarser
    /// than its level, so the imagery key is not the geometry patch itself.
    pub(crate) fn imagery_for_patch(&self, patch: TerrainPatch) -> Option<&Handle<Image>> {
        let package = self.package.as_ref()?;
        let ImageryResolution::Detailed { patch: tile, .. } = package.resolve(&patch) else {
            return None;
        };
        if !self.ready.contains(&tile) {
            return None;
        }
        self.handles.get(&tile)
    }

    pub(crate) fn metrics(&self) -> ImageryMetrics {
        ImageryMetrics {
            resident_tiles: self.ready.len(),
            pending_tiles: self.pending.len(),
            resident_mib: self.resident_bytes as f64 / (1024.0 * 1024.0),
            budget_mib: self.budget_bytes as f64 / (1024.0 * 1024.0),
            evicted_tiles: self.evictions,
        }
    }
}

/// Load the imagery package once. A missing, invalid, or unverified package is
/// reported and Earth keeps its existing global albedo.
pub(crate) fn load_earth_imagery_package(
    config: Res<TerrainImageryConfig>,
    mut imagery: ResMut<TerrainImageryResource>,
) {
    imagery.budget_bytes = config.budget_bytes;
    if !config.enabled {
        info!(target: "terrain_imagery", "Earth imagery is disabled; using global albedo fallback");
        return;
    }
    let manifest = match EarthImageryManifest::from_ron(EARTH_IMAGERY_MANIFEST) {
        Ok(manifest) => manifest,
        Err(error) => {
            info!(target: "terrain_imagery", "Earth imagery unavailable; using global albedo fallback: {error}");
            return;
        }
    };
    let root = PathBuf::from("assets").join(&config.asset_root);
    match EarthImageryPackage::load(&root, manifest) {
        Ok(package) => {
            info!(
                target: "terrain_imagery",
                "Earth imagery available: {} tiles, {} regions",
                package.tile_count(),
                package.regions().len()
            );
            imagery.package = Some(package);
        }
        Err(error) => {
            info!(target: "terrain_imagery", "Earth imagery unavailable; using global albedo fallback: {error}");
        }
    }
}

/// Request, poll, and evict imagery for published terrain patches.
pub(crate) fn stream_terrain_imagery(
    config: Res<TerrainImageryConfig>,
    streaming: Res<TerrainStreamingResource>,
    asset_server: Res<AssetServer>,
    mut imagery: ResMut<TerrainImageryResource>,
) {
    let Some(package) = imagery.package.as_ref() else {
        return;
    };
    // Desired imagery is exactly the resolution of the currently published
    // cover, so imagery follows the same visible-first priorities as geometry.
    let mut desired = BTreeSet::new();
    for patch in &streaming.published {
        if let ImageryResolution::Detailed { patch: tile, .. } = package.resolve(patch) {
            desired.insert(tile);
        }
    }

    // Cancel work for patches that are no longer visible.
    imagery.pending.retain(|patch| desired.contains(patch));

    // Admit a bounded number of new uploads per frame, within the imagery
    // budget. Geometry work is never delayed by imagery admission.
    let mut uploads = 0usize;
    for tile in &desired {
        if imagery.handles.contains_key(tile) {
            continue;
        }
        if !within_budget(
            imagery.resident_bytes,
            imagery.pending.len(),
            config.budget_bytes,
        ) || uploads >= config.max_uploads_per_frame
        {
            break;
        }
        let path = imagery_asset_path(&config.asset_root, tile);
        imagery.handles.insert(*tile, asset_server.load(path));
        imagery.pending.insert(*tile);
        uploads += 1;
    }

    // Poll in-flight loads.
    let pending: Vec<TerrainPatch> = imagery.pending.iter().copied().collect();
    for tile in pending {
        let Some(handle) = imagery.handles.get(&tile) else {
            imagery.pending.remove(&tile);
            continue;
        };
        let state = asset_server.load_state(handle.id());
        if state.is_loaded() {
            imagery.pending.remove(&tile);
            if imagery.ready.insert(tile) {
                imagery.resident_bytes += IMAGERY_TILE_BYTES;
            }
        } else if matches!(state, LoadState::Failed(_)) {
            imagery.pending.remove(&tile);
            imagery.handles.remove(&tile);
        }
    }

    // Evict imagery that is no longer visible and release its handle.
    let evict: Vec<TerrainPatch> = imagery
        .handles
        .keys()
        .copied()
        .filter(|patch| !desired.contains(patch))
        .collect();
    for tile in evict {
        imagery.handles.remove(&tile);
        imagery.pending.remove(&tile);
        if imagery.ready.remove(&tile) {
            imagery.resident_bytes = imagery.resident_bytes.saturating_sub(IMAGERY_TILE_BYTES);
        }
        imagery.evictions += 1;
    }
}

/// Upgrade a patch material once its detailed imagery tile is ready. Geometry
/// and collision are untouched; only the albedo source changes.
pub(crate) fn apply_terrain_imagery(
    imagery: Res<TerrainImageryResource>,
    mut materials: ResMut<Assets<TerrainMaterial>>,
    mut query: Query<(Entity, &mut TerrainPatchRenderState)>,
    mut commands: Commands,
) {
    if imagery.ready.is_empty() {
        return;
    }
    for (entity, mut state) in &mut query {
        let Some(handle) = imagery.imagery_for_patch(state.patch).cloned() else {
            continue;
        };
        if state.imagery_weight >= 1.0 && state.imagery_albedo == handle {
            continue;
        }
        state.imagery_albedo = handle.clone();
        state.imagery_weight = 1.0;
        let material = build_terrain_material(
            state.base_material.clone(),
            state.local_albedo.clone(),
            state.local_normal.clone(),
            state.local_detail_weight,
            state.global_albedo.clone(),
            state.imagery_albedo.clone(),
            state.imagery_weight,
        );
        let new_handle = materials.add(material);
        // Dropping the previous handle releases its material asset when nothing
        // else references it.
        let previous = std::mem::replace(&mut state.material_handle, new_handle.clone());
        drop(previous);
        commands.entity(entity).insert(MeshMaterial3d(new_handle));
    }
}

/// Admission check: one more tile may be requested only if the resident plus
/// in-flight tiles plus that tile stay within the imagery budget. This prevents
/// unbounded imagery residency without ever needing to evict visible imagery.
fn within_budget(resident_bytes: u64, pending: usize, budget_bytes: u64) -> bool {
    let reserved = resident_bytes + pending as u64 * IMAGERY_TILE_BYTES + IMAGERY_TILE_BYTES;
    reserved <= budget_bytes
}

fn imagery_asset_path(asset_root: &str, patch: &TerrainPatch) -> String {
    format!(
        "{asset_root}/tiles/{}/{}/{}_{}.png",
        cube_face_name(patch.face),
        patch.level,
        patch.tile_x,
        patch.tile_y
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::cube_sphere::CubeFace;

    #[test]
    fn imagery_admission_stays_within_budget() {
        // Empty budget admits nothing.
        assert!(!within_budget(0, 0, IMAGERY_TILE_BYTES - 1));
        // Exactly one tile fits.
        assert!(within_budget(0, 0, IMAGERY_TILE_BYTES));
        // Resident plus in-flight plus the new tile must all fit.
        assert!(!within_budget(
            IMAGERY_TILE_BYTES,
            1,
            2 * IMAGERY_TILE_BYTES
        ));
        assert!(within_budget(IMAGERY_TILE_BYTES, 0, 2 * IMAGERY_TILE_BYTES));
    }

    #[test]
    fn imagery_asset_path_matches_the_package_layout() {
        let patch = TerrainPatch {
            face: CubeFace::NegX,
            level: 12,
            tile_x: 1672,
            tile_y: 3797,
        };
        assert_eq!(
            imagery_asset_path("large_files/terrain/earth_imagery_v1", &patch),
            "large_files/terrain/earth_imagery_v1/tiles/neg_x/12/1672_3797.png"
        );
    }
}
