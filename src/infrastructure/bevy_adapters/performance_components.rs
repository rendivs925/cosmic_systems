use bevy::prelude::*;
use std::collections::VecDeque;
use std::time::Instant;

const FRAME_TIME_SAMPLE_CAPACITY: usize = 600;

// Frame metrics used by the shared FPS UI.
#[derive(Resource)]
pub struct PerformanceStats {
    pub frame_time_ms: f32,
    pub fps_raw: f32,
    pub fps_smoothed: f32,
    pub fps_display: f32,
    pub(crate) last_frame_time: Option<Instant>,
    frame_time_samples_ms: VecDeque<f32>,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            frame_time_ms: 16.67,
            fps_raw: 60.0,
            fps_smoothed: 60.0,
            fps_display: 60.0,
            last_frame_time: None,
            frame_time_samples_ms: VecDeque::with_capacity(FRAME_TIME_SAMPLE_CAPACITY),
        }
    }
}

impl PerformanceStats {
    pub(crate) fn record_frame_time(&mut self, frame_time_ms: f32) {
        if !frame_time_ms.is_finite() || frame_time_ms <= 0.0 {
            return;
        }

        if self.frame_time_samples_ms.len() == FRAME_TIME_SAMPLE_CAPACITY {
            self.frame_time_samples_ms.pop_front();
        }
        self.frame_time_samples_ms.push_back(frame_time_ms);
    }

    pub(crate) fn frame_time_percentiles_ms(&self) -> Option<(f32, f32, f32)> {
        if self.frame_time_samples_ms.is_empty() {
            return None;
        }

        let mut sorted = self
            .frame_time_samples_ms
            .iter()
            .copied()
            .collect::<Vec<_>>();
        sorted.sort_by(f32::total_cmp);
        let percentile = |fraction: f32| {
            let index = ((sorted.len() as f32 * fraction).ceil() as usize).saturating_sub(1);
            sorted[index]
        };
        Some((percentile(0.50), percentile(0.95), percentile(0.99)))
    }

    pub(crate) fn frame_time_sample_count(&self) -> usize {
        self.frame_time_samples_ms.len()
    }
}
