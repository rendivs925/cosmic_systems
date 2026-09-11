use bevy::prelude::*;
use std::collections::VecDeque;

const TERRAIN_ATTRIBUTION_HISTORY_CAPACITY: usize = 120;

/// CPU-side attribution for one terrain presentation frame. Upload timing ends
/// after Bevy asset submission; it is not a measurement of GPU completion.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TerrainFrameAttribution {
    pub frame_id: u64,
    pub completion_poll_ms: f64,
    pub viewport_culling_ms: f64,
    pub lod_selection_ms: f64,
    pub scheduling_ms: f64,
    pub task_admission_ms: f64,
    pub publication_ms: f64,
    pub eviction_ms: f64,
    pub cpu_mesh_construction_ms: f64,
    pub material_ms: f64,
    pub image_asset_creation_ms: f64,
    pub cpu_to_gpu_submission_ms: f64,
    pub activation_ms: f64,
    pub queue_start: usize,
    pub queue_end: usize,
    pub queue_peak: usize,
    pub ready_received: usize,
    pub ready_backfilled: usize,
    pub ready_rejected: usize,
    pub visible_candidates: usize,
    pub culled_nodes: usize,
    pub visible_patches: usize,
    pub lod_evaluations: usize,
    pub lod_splits: usize,
    pub lod_merges: usize,
    pub requests: usize,
    pub duplicate_requests: usize,
    pub tasks_started: usize,
    pub tasks_completed: usize,
    pub tasks_cancelled: usize,
    pub patches_published: usize,
    pub patches_activated: usize,
    pub patches_evicted: usize,
    pub mesh_assets_created: usize,
    pub material_assets_created: usize,
    pub image_assets_created: usize,
}

impl TerrainFrameAttribution {
    pub(crate) fn total_cpu_ms(self) -> f64 {
        self.completion_poll_ms
            + self.viewport_culling_ms
            + self.lod_selection_ms
            + self.scheduling_ms
            + self.task_admission_ms
            + self.publication_ms
            + self.eviction_ms
            + self.cpu_mesh_construction_ms
            + self.material_ms
            + self.image_asset_creation_ms
            + self.cpu_to_gpu_submission_ms
            + self.activation_ms
    }

    fn add_assign(&mut self, record: Self) {
        self.completion_poll_ms += record.completion_poll_ms;
        self.viewport_culling_ms += record.viewport_culling_ms;
        self.lod_selection_ms += record.lod_selection_ms;
        self.scheduling_ms += record.scheduling_ms;
        self.task_admission_ms += record.task_admission_ms;
        self.publication_ms += record.publication_ms;
        self.eviction_ms += record.eviction_ms;
        self.cpu_mesh_construction_ms += record.cpu_mesh_construction_ms;
        self.material_ms += record.material_ms;
        self.image_asset_creation_ms += record.image_asset_creation_ms;
        self.cpu_to_gpu_submission_ms += record.cpu_to_gpu_submission_ms;
        self.activation_ms += record.activation_ms;
        self.queue_start += record.queue_start;
        self.queue_end += record.queue_end;
        self.queue_peak = self.queue_peak.max(record.queue_peak);
        self.ready_received += record.ready_received;
        self.ready_backfilled += record.ready_backfilled;
        self.ready_rejected += record.ready_rejected;
        self.visible_candidates += record.visible_candidates;
        self.culled_nodes += record.culled_nodes;
        self.visible_patches += record.visible_patches;
        self.lod_evaluations += record.lod_evaluations;
        self.lod_splits += record.lod_splits;
        self.lod_merges += record.lod_merges;
        self.requests += record.requests;
        self.duplicate_requests += record.duplicate_requests;
        self.tasks_started += record.tasks_started;
        self.tasks_completed += record.tasks_completed;
        self.tasks_cancelled += record.tasks_cancelled;
        self.patches_published += record.patches_published;
        self.patches_activated += record.patches_activated;
        self.patches_evicted += record.patches_evicted;
        self.mesh_assets_created += record.mesh_assets_created;
        self.material_assets_created += record.material_assets_created;
        self.image_assets_created += record.image_assets_created;
    }
}

/// Bounded, opt-in terrain performance attribution consumed by the shared
/// cadence-limited performance logger.
#[derive(Resource)]
pub(crate) struct TerrainPerformanceTelemetry {
    current: TerrainFrameAttribution,
    history: VecDeque<TerrainFrameAttribution>,
    next_frame_id: u64,
}

impl Default for TerrainPerformanceTelemetry {
    fn default() -> Self {
        Self {
            current: TerrainFrameAttribution::default(),
            history: VecDeque::with_capacity(TERRAIN_ATTRIBUTION_HISTORY_CAPACITY),
            next_frame_id: 0,
        }
    }
}

impl TerrainPerformanceTelemetry {
    pub(crate) fn begin_frame(&mut self, enabled: bool) {
        if !enabled {
            return;
        }
        self.current = TerrainFrameAttribution {
            frame_id: self.next_frame_id,
            ..default()
        };
        self.next_frame_id += 1;
    }

    pub(crate) fn current_mut(&mut self, enabled: bool) -> Option<&mut TerrainFrameAttribution> {
        enabled.then_some(&mut self.current)
    }

    pub(crate) fn finish_frame(&mut self, enabled: bool) {
        if !enabled {
            return;
        }
        if self.history.len() == TERRAIN_ATTRIBUTION_HISTORY_CAPACITY {
            self.history.pop_front();
        }
        self.history.push_back(self.current);
    }

    pub(crate) fn summary(&self) -> Option<TerrainPerformanceSummary> {
        let first = *self.history.front()?;
        let mut aggregate = first;
        let mut top = first;
        for record in self.history.iter().copied().skip(1) {
            aggregate.add_assign(record);
            if record.total_cpu_ms() > top.total_cpu_ms() {
                top = record;
            }
        }
        Some(TerrainPerformanceSummary {
            sample_count: self.history.len(),
            aggregate,
            top,
            percentiles: TerrainPhasePercentiles::from_history(&self.history),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainPerformanceSummary {
    pub sample_count: usize,
    pub aggregate: TerrainFrameAttribution,
    pub top: TerrainFrameAttribution,
    pub percentiles: TerrainPhasePercentiles,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TerrainPhasePercentiles {
    pub completion_poll_ms: TimingPercentiles,
    pub viewport_culling_ms: TimingPercentiles,
    pub lod_selection_ms: TimingPercentiles,
    pub scheduling_ms: TimingPercentiles,
    pub task_admission_ms: TimingPercentiles,
    pub publication_ms: TimingPercentiles,
    pub eviction_ms: TimingPercentiles,
    pub cpu_mesh_construction_ms: TimingPercentiles,
    pub material_ms: TimingPercentiles,
    pub image_asset_creation_ms: TimingPercentiles,
    pub cpu_to_gpu_submission_ms: TimingPercentiles,
    pub activation_ms: TimingPercentiles,
}

impl TerrainPhasePercentiles {
    fn from_history(history: &VecDeque<TerrainFrameAttribution>) -> Self {
        Self {
            completion_poll_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.completion_poll_ms),
            ),
            viewport_culling_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.viewport_culling_ms),
            ),
            lod_selection_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.lod_selection_ms),
            ),
            scheduling_ms: TimingPercentiles::from_values(history.iter().map(|r| r.scheduling_ms)),
            task_admission_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.task_admission_ms),
            ),
            publication_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.publication_ms),
            ),
            eviction_ms: TimingPercentiles::from_values(history.iter().map(|r| r.eviction_ms)),
            cpu_mesh_construction_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.cpu_mesh_construction_ms),
            ),
            material_ms: TimingPercentiles::from_values(history.iter().map(|r| r.material_ms)),
            image_asset_creation_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.image_asset_creation_ms),
            ),
            cpu_to_gpu_submission_ms: TimingPercentiles::from_values(
                history.iter().map(|r| r.cpu_to_gpu_submission_ms),
            ),
            activation_ms: TimingPercentiles::from_values(history.iter().map(|r| r.activation_ms)),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TimingPercentiles {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub p99_ms: f64,
}

impl TimingPercentiles {
    fn from_values(values: impl Iterator<Item = f64>) -> Self {
        let mut sorted: Vec<_> = values.collect();
        sorted.sort_by(f64::total_cmp);
        let percentile = |fraction: f64| {
            let index = ((sorted.len() as f64 * fraction).ceil() as usize).saturating_sub(1);
            sorted[index]
        };
        Self {
            p50_ms: percentile(0.50),
            p95_ms: percentile(0.95),
            p99_ms: percentile(0.99),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregation_sums_counters_and_retains_the_most_expensive_frame() {
        let mut telemetry = TerrainPerformanceTelemetry::default();
        telemetry.begin_frame(true);
        let first = telemetry.current_mut(true).unwrap();
        first.completion_poll_ms = 1.0;
        first.cpu_mesh_construction_ms = 1.0;
        first.queue_start = 2;
        first.queue_peak = 4;
        first.ready_received = 3;
        telemetry.finish_frame(true);

        telemetry.begin_frame(true);
        let second = telemetry.current_mut(true).unwrap();
        second.cpu_mesh_construction_ms = 3.0;
        second.queue_start = 1;
        second.queue_peak = 5;
        second.ready_rejected = 1;
        telemetry.finish_frame(true);

        let summary = telemetry.summary().unwrap();
        assert_eq!(summary.sample_count, 2);
        assert_eq!(summary.aggregate.queue_start, 3);
        assert_eq!(summary.aggregate.queue_peak, 5);
        assert_eq!(summary.aggregate.ready_received, 3);
        assert_eq!(summary.aggregate.ready_rejected, 1);
        assert_eq!(summary.aggregate.cpu_mesh_construction_ms, 4.0);
        assert_eq!(summary.top.cpu_mesh_construction_ms, 3.0);
        assert_eq!(summary.top.frame_id, 1);
        assert_eq!(summary.percentiles.cpu_mesh_construction_ms.p50_ms, 1.0);
        assert_eq!(summary.percentiles.cpu_mesh_construction_ms.p95_ms, 3.0);
    }

    #[test]
    fn frame_reset_and_queue_observation_do_not_leak_between_frames() {
        let mut telemetry = TerrainPerformanceTelemetry::default();
        telemetry.begin_frame(true);
        let record = telemetry.current_mut(true).unwrap();
        record.queue_start = 3;
        record.queue_peak = 6;
        record.queue_end = 4;
        record.ready_backfilled = 2;
        record.tasks_started = 2;
        record.mesh_assets_created = 3;
        telemetry.finish_frame(true);

        telemetry.begin_frame(true);
        let record = telemetry.current_mut(true).unwrap();
        record.queue_start = 4;
        record.queue_peak = 4;
        record.queue_end = 1;
        telemetry.finish_frame(true);

        let summary = telemetry.summary().unwrap();
        assert_eq!(summary.aggregate.queue_start, 7);
        assert_eq!(summary.aggregate.queue_end, 5);
        assert_eq!(summary.aggregate.queue_peak, 6);
        assert_eq!(summary.aggregate.ready_backfilled, 2);
        assert_eq!(summary.aggregate.tasks_started, 2);
        assert_eq!(summary.aggregate.mesh_assets_created, 3);
    }
}
