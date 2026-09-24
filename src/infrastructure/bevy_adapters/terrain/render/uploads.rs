//! Ready-patch upload queue and render-entity indexing.
//!
//! Bounds the CPU-to-GPU asset-creation backlog and coalesces repeated ready
//! notifications so a completed terrain batch cannot stall presentation.

use super::{TerrainPatchReady, MAX_PENDING_PATCH_UPLOADS};
use crate::domain::services::cube_sphere::TerrainPatch;
use crate::infrastructure::bevy_adapters::terrain::performance::TerrainPerformanceTelemetry;
use bevy::prelude::*;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

/// Identifies a terrain render entity independently for every planet. Patch
/// coordinates alone overlap between planets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) struct TerrainPatchRenderKey {
    pub(super) planet_entity: Entity,
    pub(super) patch: TerrainPatch,
}

impl From<&TerrainPatchReady> for TerrainPatchRenderKey {
    fn from(event: &TerrainPatchReady) -> Self {
        Self {
            planet_entity: event.planet_entity,
            patch: event.patch,
        }
    }
}

/// Direct lifecycle lookup avoids scanning every render entity per event.
#[derive(Resource, Default)]
pub(super) struct TerrainPatchRenderIndex(pub(super) HashMap<TerrainPatchRenderKey, Entity>);

/// Ready patches wait here until their CPU-to-GPU asset creation budget is
/// available. Messages expire after two frames, so the queue owns pending
/// uploads and coalesces repeated ready notifications.
#[derive(Resource, Default)]
pub(super) struct PendingTerrainPatchUploads {
    pub(super) queue: VecDeque<TerrainPatchReady>,
    pub(super) queued: HashSet<TerrainPatchRenderKey>,
    pub(super) needs_backfill: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerrainUploadEnqueueResult {
    Queued,
    Duplicate,
    Rejected,
}

#[derive(Default)]
pub(super) struct TerrainUploadBackfill {
    queued: usize,
    rejected: usize,
}

impl PendingTerrainPatchUploads {
    pub(super) fn retain_published_for_planet(
        &mut self,
        active_planet: Option<Entity>,
        published: &std::collections::BTreeSet<TerrainPatch>,
    ) {
        let before = self.queue.len();
        self.queue.retain(|event| {
            active_planet == Some(event.planet_entity) && published.contains(&event.patch)
        });
        // Rebuilding the dedup set is only necessary when the retain actually
        // removed queued work; the common case (nothing stale) skips the
        // per-frame allocation entirely.
        if self.queue.len() != before {
            self.queued = self.queue.iter().map(TerrainPatchRenderKey::from).collect();
        }
    }

    pub(super) fn enqueue(&mut self, event: TerrainPatchReady) -> TerrainUploadEnqueueResult {
        let key = TerrainPatchRenderKey::from(&event);
        if self.queued.contains(&key) {
            return TerrainUploadEnqueueResult::Duplicate;
        }
        if self.queue.len() >= MAX_PENDING_PATCH_UPLOADS {
            self.needs_backfill = true;
            return TerrainUploadEnqueueResult::Rejected;
        }
        self.queued.insert(key);
        self.queue.push_back(event);
        TerrainUploadEnqueueResult::Queued
    }

    pub(super) fn pop_front(&mut self) -> Option<TerrainPatchReady> {
        let event = self.queue.pop_front()?;
        self.queued.remove(&TerrainPatchRenderKey::from(&event));
        Some(event)
    }

    pub(super) fn backfill_published(
        &mut self,
        planet_entity: Entity,
        published: &std::collections::BTreeSet<TerrainPatch>,
        render_index: &TerrainPatchRenderIndex,
    ) -> TerrainUploadBackfill {
        let mut backfill = TerrainUploadBackfill::default();
        if !self.needs_backfill {
            return backfill;
        }

        self.needs_backfill = false;
        for patch in published.iter().copied() {
            let event = TerrainPatchReady {
                patch,
                planet_entity,
            };
            let key = TerrainPatchRenderKey::from(&event);
            if render_index.0.contains_key(&key) || self.queued.contains(&key) {
                continue;
            }
            match self.enqueue(event) {
                TerrainUploadEnqueueResult::Queued => backfill.queued += 1,
                TerrainUploadEnqueueResult::Duplicate => {}
                TerrainUploadEnqueueResult::Rejected => {
                    backfill.rejected += 1;
                    break;
                }
            }
        }
        backfill
    }
}

pub(super) fn enqueue_ready_uploads(
    events: &mut MessageReader<TerrainPatchReady>,
    pending_uploads: &mut PendingTerrainPatchUploads,
    published: &BTreeSet<TerrainPatch>,
    active_planet: Option<Entity>,
    render_index: &TerrainPatchRenderIndex,
    terrain_performance: &mut TerrainPerformanceTelemetry,
    instrumentation_enabled: bool,
) -> bool {
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.queue_start = pending_uploads.queue.len();
        record.queue_peak = record.queue_start;
    }
    pending_uploads.retain_published_for_planet(active_planet, published);
    for event in events.read().cloned() {
        if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
            record.ready_received += 1;
        }
        if active_planet == Some(event.planet_entity)
            && published.contains(&event.patch)
            && pending_uploads.enqueue(event) == TerrainUploadEnqueueResult::Rejected
        {
            if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
                record.ready_rejected += 1;
            }
        }
    }
    // A ready-event burst can exceed the bounded queue. Keep the recovery flag
    // until every published patch is queued or rendered; MessageReader cannot
    // replay the events that overflowed in an earlier frame.
    if pending_uploads.needs_backfill {
        let Some(planet_entity) = active_planet else {
            return false;
        };
        let backfill = pending_uploads.backfill_published(planet_entity, published, render_index);
        if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
            record.ready_backfilled += backfill.queued;
            record.ready_rejected += backfill.rejected;
        }
    }
    if let Some(record) = terrain_performance.current_mut(instrumentation_enabled) {
        record.queue_peak = record.queue_peak.max(pending_uploads.queue.len());
    }
    true
}
