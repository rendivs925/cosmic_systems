use bevy::prelude::*;
use std::collections::VecDeque;
use std::env;
use std::time::{Duration, Instant};

const FRAME_TIME_SAMPLE_CAPACITY: usize = 600;
const PERFORMANCE_METRICS_ENV: &str = "COSMIC_SYSTEMS_PERFORMANCE_METRICS";
const PERFORMANCE_METRICS_ENABLED_VALUE: &str = "1";
const PERFORMANCE_METRICS_REPORT_INTERVAL: Duration = Duration::from_secs(5);

/// Enables cadence-limited frame-time reports for an explicitly requested run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerformanceMetricsReporting {
    Disabled,
    Enabled,
}

/// Shared configuration for optional performance instrumentation.
#[derive(Resource, Debug, Clone, Copy)]
pub struct PerformanceMetricsConfig {
    reporting: PerformanceMetricsReporting,
}

impl PerformanceMetricsConfig {
    pub fn from_environment() -> Self {
        let reporting = match env::var(PERFORMANCE_METRICS_ENV).as_deref() {
            Ok(PERFORMANCE_METRICS_ENABLED_VALUE) => PerformanceMetricsReporting::Enabled,
            _ => PerformanceMetricsReporting::Disabled,
        };
        Self { reporting }
    }

    fn report_interval(self) -> Option<Duration> {
        match self.reporting {
            PerformanceMetricsReporting::Disabled => None,
            PerformanceMetricsReporting::Enabled => Some(PERFORMANCE_METRICS_REPORT_INTERVAL),
        }
    }
}

/// A typed summary of the bounded frame-time history.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FrameTimeSummary {
    pub sample_count: usize,
    pub p50_ms: f32,
    pub p95_ms: f32,
    pub p99_ms: f32,
}

#[derive(Default)]
struct FrameTimeHistory {
    samples_ms: VecDeque<f32>,
}

impl FrameTimeHistory {
    fn record(&mut self, frame_time_ms: f32) {
        if !frame_time_ms.is_finite() || frame_time_ms <= 0.0 {
            return;
        }

        if self.samples_ms.len() == FRAME_TIME_SAMPLE_CAPACITY {
            self.samples_ms.pop_front();
        }
        self.samples_ms.push_back(frame_time_ms);
    }

    fn summary(&self) -> Option<FrameTimeSummary> {
        if self.samples_ms.is_empty() {
            return None;
        }

        let mut sorted = self.samples_ms.iter().copied().collect::<Vec<_>>();
        sorted.sort_by(f32::total_cmp);
        let percentile = |fraction: f32| {
            let index = ((sorted.len() as f32 * fraction).ceil() as usize).saturating_sub(1);
            sorted[index]
        };
        Some(FrameTimeSummary {
            sample_count: sorted.len(),
            p50_ms: percentile(0.50),
            p95_ms: percentile(0.95),
            p99_ms: percentile(0.99),
        })
    }
}

/// Owns the reporting cadence independently from the frame-time samples.
#[derive(Resource, Default)]
pub(crate) struct PerformanceMetricsReporter {
    last_report_at: Option<Instant>,
}

impl PerformanceMetricsReporter {
    pub(crate) fn report_due(&mut self, config: PerformanceMetricsConfig, now: Instant) -> bool {
        let Some(report_interval) = config.report_interval() else {
            return false;
        };
        if self
            .last_report_at
            .is_some_and(|previous| now.duration_since(previous) < report_interval)
        {
            return false;
        }

        self.last_report_at = Some(now);
        true
    }
}

// Frame metrics used by the shared FPS UI.
#[derive(Resource)]
pub struct PerformanceStats {
    pub frame_time_ms: f32,
    pub fps_raw: f32,
    pub fps_smoothed: f32,
    pub fps_display: f32,
    last_frame_time: Option<Instant>,
    frame_time_history: FrameTimeHistory,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            frame_time_ms: 16.67,
            fps_raw: 60.0,
            fps_smoothed: 60.0,
            fps_display: 60.0,
            last_frame_time: None,
            frame_time_history: FrameTimeHistory {
                samples_ms: VecDeque::with_capacity(FRAME_TIME_SAMPLE_CAPACITY),
            },
        }
    }
}

impl PerformanceStats {
    pub(crate) fn update_at(&mut self, now: Instant) {
        let frame_time_seconds = self
            .last_frame_time
            .map(|last_frame_time| now.duration_since(last_frame_time).as_secs_f64())
            .unwrap_or(1.0 / 60.0);
        self.last_frame_time = Some(now);
        self.frame_time_ms = (frame_time_seconds * 1000.0) as f32;
        self.frame_time_history.record(self.frame_time_ms);
        if self.frame_time_ms <= 0.0 {
            self.fps_raw = 0.0;
        } else {
            self.fps_raw = 1000.0 / self.frame_time_ms;
        }
        const SMOOTHING_FACTOR: f32 = 0.1;
        self.fps_smoothed =
            self.fps_smoothed * (1.0 - SMOOTHING_FACTOR) + self.fps_raw * SMOOTHING_FACTOR;
        self.fps_display = self.fps_smoothed;
    }

    pub(crate) fn frame_time_summary(&self) -> Option<FrameTimeSummary> {
        self.frame_time_history.summary()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reporter_respects_the_typed_reporting_mode_and_cadence() {
        let now = Instant::now();
        let mut reporter = PerformanceMetricsReporter::default();
        let disabled = PerformanceMetricsConfig {
            reporting: PerformanceMetricsReporting::Disabled,
        };
        let enabled = PerformanceMetricsConfig {
            reporting: PerformanceMetricsReporting::Enabled,
        };

        assert!(!reporter.report_due(disabled, now));
        assert!(reporter.report_due(enabled, now));
        assert!(!reporter.report_due(enabled, now + Duration::from_secs(4)));
        assert!(reporter.report_due(enabled, now + PERFORMANCE_METRICS_REPORT_INTERVAL));
    }

    #[test]
    fn frame_time_history_is_bounded_and_reports_percentiles() {
        let mut history = FrameTimeHistory::default();
        for frame_time_ms in 1..=601 {
            history.record(frame_time_ms as f32);
        }

        assert_eq!(history.samples_ms.len(), 600);
        assert_eq!(
            history.summary(),
            Some(FrameTimeSummary {
                sample_count: 600,
                p50_ms: 301.0,
                p95_ms: 571.0,
                p99_ms: 595.0,
            })
        );
    }
}
