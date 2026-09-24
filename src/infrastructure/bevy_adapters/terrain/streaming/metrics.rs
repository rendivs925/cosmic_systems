//! Terrain streaming telemetry snapshot types.

use super::selection::TerrainCullingStats;
use super::{TerrainCancellation, TerrainGenerationBatch, TerrainStreamingResource};
use crate::domain::services::cube_sphere::TerrainPatch;
use crate::infrastructure::bevy_adapters::terrain::imagery::ImageryMetrics;
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug)]
pub(super) struct PatchLevelDistribution(pub(super) BTreeMap<u32, usize>);

impl PatchLevelDistribution {
    pub(super) fn from_patches(patches: impl IntoIterator<Item = TerrainPatch>) -> Self {
        let mut levels = BTreeMap::new();
        for patch in patches {
            *levels.entry(patch.level).or_default() += 1;
        }
        Self(levels)
    }
}

#[derive(Debug)]
pub(super) struct TerrainStreamingMetrics {
    requested_tiles: usize,
    target_tiles: usize,
    visible_tiles: usize,
    blocked_target_tiles: usize,
    generated_tiles: usize,
    resident_tiles: usize,
    upload_backlog_tiles: usize,
    estimated_resident_mib: f64,
    budget_mib: f64,
    inflight_tiles: usize,
    oldest_inflight_ms: f64,
    cancelled_tiles: usize,
    evicted_tiles: usize,
    prelaunch: bool,
    focus_max_lod: u32,
    culling: TerrainCullingStats,
    main_thread_ms: f64,
    requested_lods: PatchLevelDistribution,
    target_lods: PatchLevelDistribution,
    visible_lods: PatchLevelDistribution,
    completed: TerrainGenerationBatch,
    imagery: ImageryMetrics,
}

impl TerrainStreamingMetrics {
    #[expect(
        clippy::too_many_arguments,
        reason = "Metrics intentionally capture the complete cadence-limited streaming snapshot."
    )]
    pub(super) fn capture(
        streaming: &TerrainStreamingResource,
        requested: &BTreeSet<TerrainPatch>,
        target: &BTreeSet<TerrainPatch>,
        completed: TerrainGenerationBatch,
        cancellation: TerrainCancellation,
        evicted_tiles: usize,
        prelaunch: bool,
        focus_max_lod: u32,
        culling: TerrainCullingStats,
        main_thread_ms: f64,
        imagery: ImageryMetrics,
    ) -> Self {
        let upload_backlog_tiles = streaming
            .generated
            .values()
            .filter(|cached| cached.surface.is_some())
            .count();
        Self {
            requested_tiles: requested.len(),
            target_tiles: target.len(),
            visible_tiles: streaming.published.len(),
            blocked_target_tiles: target.difference(&streaming.published).count(),
            generated_tiles: streaming.generated.len(),
            resident_tiles: streaming.manager.ready_patch_count(),
            upload_backlog_tiles,
            estimated_resident_mib: streaming.manager.resident_bytes() as f64 / (1024.0 * 1024.0),
            budget_mib: streaming.budget_bytes as f64 / (1024.0 * 1024.0),
            inflight_tiles: streaming.inflight.len(),
            oldest_inflight_ms: streaming
                .inflight
                .values()
                .map(|inflight| inflight.started_at.elapsed().as_secs_f64() * 1_000.0)
                .fold(0.0, f64::max),
            cancelled_tiles: cancellation.total(),
            evicted_tiles,
            prelaunch,
            focus_max_lod,
            culling,
            main_thread_ms,
            requested_lods: PatchLevelDistribution::from_patches(requested.iter().copied()),
            target_lods: PatchLevelDistribution::from_patches(target.iter().copied()),
            visible_lods: PatchLevelDistribution::from_patches(streaming.published.iter().copied()),
            completed,
            imagery,
        }
    }

    pub(super) fn log(&self) {
        info!(
            target: "terrain_streaming",
            requested_tiles = self.requested_tiles,
            target_tiles = self.target_tiles,
            visible_tiles = self.visible_tiles,
            blocked_target_tiles = self.blocked_target_tiles,
            generated_tiles = self.generated_tiles,
            resident_tiles = self.resident_tiles,
            upload_backlog_tiles = self.upload_backlog_tiles,
            estimated_resident_mib = self.estimated_resident_mib,
            budget_mib = self.budget_mib,
            inflight_tiles = self.inflight_tiles,
            oldest_inflight_ms = self.oldest_inflight_ms,
            cancelled_tiles = self.cancelled_tiles,
            evicted_tiles = self.evicted_tiles,
            prelaunch = self.prelaunch,
            focus_max_lod = self.focus_max_lod,
            culling_candidates = self.culling.candidates,
            horizon_rejected = self.culling.horizon_rejected,
            frustum_rejected = self.culling.frustum_rejected,
            main_thread_ms = self.main_thread_ms,
            requested_lods = ?self.requested_lods.0,
            target_lods = ?self.target_lods.0,
            visible_lods = ?self.visible_lods.0,
            completed_batch_tiles = self.completed.completed_tiles,
            completed_batch_ms = self.completed.generation_ms,
            imagery_resident_tiles = self.imagery.resident_tiles,
            imagery_pending_tiles = self.imagery.pending_tiles,
            imagery_resident_mib = self.imagery.resident_mib,
            imagery_budget_mib = self.imagery.budget_mib,
            imagery_evicted_tiles = self.imagery.evicted_tiles,
            "Terrain streaming metrics"
        );
    }
}
