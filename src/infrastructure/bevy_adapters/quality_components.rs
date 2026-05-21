use std::collections::VecDeque;
use bevy::prelude::*;
use super::performance_components::{PerformanceStats, QualityLevel, QualityTrend};

// System monitoring for adaptive quality control
#[derive(Clone, Debug)]
pub struct SystemMetrics {
    pub gpu_utilization: f32,
    pub cpu_utilization: f32,
    pub memory_pressure: f32,
    pub frame_time_variance: f32,
    pub timestamp: std::time::Instant,
}

// Adaptive quality controller
pub struct AdaptiveQualityController {
    target_fps: f32,
    quality_levels: Vec<QualityLevel>,
    current_quality_index: usize,
    adaptation_history: Vec<f32>, // Recent frame times
    history_size: usize,
    last_adaptation: std::time::Instant,
    cooldown_ms: u64,
}

impl AdaptiveQualityController {
    pub fn new(target_fps: f32) -> Self {
        Self {
            target_fps,
            quality_levels: vec![
                QualityLevel::Minimal,
                QualityLevel::Low,
                QualityLevel::Medium,
                QualityLevel::High,
                QualityLevel::Ultra,
            ],
            current_quality_index: 3, // Start with High
            adaptation_history: Vec::with_capacity(60),
            history_size: 60,
            last_adaptation: std::time::Instant::now(),
            cooldown_ms: 2000, // 2 second cooldown
        }
    }

    /// Analyze system metrics and determine optimal quality level
    pub fn adapt_quality(&mut self, current_fps: f32, system_metrics: &SystemMetrics) -> Option<QualityLevel> {
        let now = std::time::Instant::now();
        if now.duration_since(self.last_adaptation).as_millis() < self.cooldown_ms as u128 {
            return None; // Cooldown active
        }

        // Add current FPS to history
        self.adaptation_history.push(current_fps);
        if self.adaptation_history.len() > self.history_size {
            self.adaptation_history.remove(0);
        }

        // Need minimum history for stable decisions
        if self.adaptation_history.len() < 10 {
            return None;
        }

        // Calculate metrics
        let avg_fps = self.adaptation_history.iter().sum::<f32>() / self.adaptation_history.len() as f32;
        let fps_variance = self.adaptation_history.iter()
            .map(|fps| (fps - avg_fps).powi(2))
            .sum::<f32>() / self.adaptation_history.len() as f32;

        // Target frame time in ms
        // Quality adaptation logic
        let mut new_quality_index = self.current_quality_index;

        // Critical performance degradation
        if avg_fps < self.target_fps * 0.7 || system_metrics.memory_pressure > 0.9 {
            new_quality_index = new_quality_index.saturating_sub(2); // Drop 2 levels
        }
        // Significant performance degradation
        else if avg_fps < self.target_fps * 0.85 || system_metrics.gpu_utilization > 0.95 {
            new_quality_index = new_quality_index.saturating_sub(1);
        }
        // Performance improving
        else if avg_fps > self.target_fps * 1.1 && system_metrics.gpu_utilization < 0.7 &&
                system_metrics.memory_pressure < 0.5 && fps_variance < 2.0 {
            new_quality_index = (new_quality_index + 1).min(self.quality_levels.len() - 1);
        }

        // Only change if there's a difference
        if new_quality_index != self.current_quality_index {
            self.current_quality_index = new_quality_index;
            self.last_adaptation = now;
            println!("Adaptive Quality: {} FPS -> {:?} (GPU: {:.1}%, Mem: {:.1}%)",
                    avg_fps as i32, self.quality_levels[new_quality_index],
                    system_metrics.gpu_utilization * 100.0,
                    system_metrics.memory_pressure * 100.0);
            return Some(self.quality_levels[new_quality_index]);
        }

        None
    }

    pub fn get_current_quality(&self) -> QualityLevel {
        self.quality_levels[self.current_quality_index]
    }
}

/// Advanced Quality Adaptation System for intelligent performance scaling
pub struct QualityAdaptationSystem {
    controller: AdaptiveQualityController,
    last_metrics_update: std::time::Instant,
    metrics_update_interval_ms: u64,
}

impl QualityAdaptationSystem {
    pub fn new(target_fps: f32) -> Self {
        Self {
            controller: AdaptiveQualityController::new(target_fps),
            last_metrics_update: std::time::Instant::now(),
            metrics_update_interval_ms: 100, // Update metrics every 100ms
        }
    }

    /// Update system metrics and adapt quality if needed
    pub fn update_and_adapt(&mut self, perf_stats: &mut PerformanceStats) -> Option<QualityLevel> {
        let now = std::time::Instant::now();

        // Update metrics periodically
        if now.duration_since(self.last_metrics_update).as_millis() >= self.metrics_update_interval_ms as u128 {
            self.update_system_metrics(perf_stats);
            self.last_metrics_update = now;
        }

        // Get current FPS for adaptation
        let current_fps = perf_stats.fps_display;

        // Create system metrics for adaptation
        let system_metrics = SystemMetrics {
            gpu_utilization: perf_stats.gpu_utilization,
            cpu_utilization: perf_stats.cpu_utilization,
            memory_pressure: perf_stats.memory_pressure,
            frame_time_variance: perf_stats.frame_time_variance,
            timestamp: now,
        };

        // Adapt quality based on current performance and system state
        self.controller.adapt_quality(current_fps, &system_metrics)
    }

    /// Update system monitoring metrics
    fn update_system_metrics(&self, perf_stats: &mut PerformanceStats) {
        // Calculate frame time variance from recent history
        if perf_stats.frame_time_history.len() >= 10 {
            let mean = perf_stats.frame_time_history.iter().sum::<f32>() / perf_stats.frame_time_history.len() as f32;
            let variance = perf_stats.frame_time_history.iter()
                .map(|ft| (ft - mean).powi(2))
                .sum::<f32>() / perf_stats.frame_time_history.len() as f32;
            perf_stats.frame_time_variance = variance;
        }

        // Estimate GPU utilization based on frame times and Vulkan activity
        if perf_stats.vulkan_enabled && perf_stats.vulkan_kepler_calls > 0 {
            // GPU is active, estimate utilization
            let gpu_frame_time_ratio = perf_stats.gpu_frame_time_ms / perf_stats.frame_time_ms;
            perf_stats.gpu_utilization = (gpu_frame_time_ratio * 0.8).min(1.0); // Conservative estimate
        } else {
            perf_stats.gpu_utilization = 0.0;
        }

        // Estimate CPU utilization (simplified)
        let physics_ratio = perf_stats.physics_update_time / perf_stats.frame_time_ms;
        perf_stats.cpu_utilization = (physics_ratio * 1.5).min(1.0); // Physics is CPU intensive

        // Estimate memory pressure (simplified - could be enhanced with actual system memory monitoring)
        perf_stats.memory_pressure = (perf_stats.memory_usage_mb / 16000.0).min(1.0); // Assume 16GB system

        // Update adaptation confidence based on stability
        if perf_stats.frame_time_variance < 1.0 && perf_stats.gpu_utilization < 0.9 {
            perf_stats.adaptive_confidence = (perf_stats.adaptive_confidence + 0.05).min(1.0);
        } else {
            perf_stats.adaptive_confidence = (perf_stats.adaptive_confidence - 0.1).max(0.0);
        }

        // Update quality trend
        let fps_ratio = perf_stats.fps_display / perf_stats.target_fps;
        perf_stats.quality_trend = if fps_ratio < 0.8 {
            QualityTrend::Critical
        } else if fps_ratio < 0.9 {
            QualityTrend::Degrading
        } else if fps_ratio > 1.1 {
            QualityTrend::Improving
        } else {
            QualityTrend::Stable
        };
    }

    /// Get predictive quality recommendation
    pub fn get_predictive_quality(&self, perf_stats: &PerformanceStats) -> QualityLevel {
        match perf_stats.quality_trend {
            QualityTrend::Critical => QualityLevel::Low,
            QualityTrend::Degrading => match perf_stats.quality_level {
                QualityLevel::Ultra => QualityLevel::High,
                QualityLevel::High => QualityLevel::Medium,
                QualityLevel::Medium => QualityLevel::Low,
                _ => QualityLevel::Low,
            },
            QualityTrend::Improving => match perf_stats.quality_level {
                QualityLevel::Low => QualityLevel::Medium,
                QualityLevel::Medium => QualityLevel::High,
                QualityLevel::High => QualityLevel::Ultra,
                _ => QualityLevel::Ultra,
            },
            QualityTrend::Stable => perf_stats.quality_level,
        }
    }
}

/// Advanced quality adaptation system
#[derive(Resource)]
pub struct QualityAdaptationResource {
    pub system: QualityAdaptationSystem,
    pub enabled: bool,
}

impl Default for QualityAdaptationResource {
    fn default() -> Self {
        Self {
            system: QualityAdaptationSystem::new(60.0),
            enabled: true,
        }
    }
}

// Quality controller for gradual performance adaptation
#[derive(Resource)]
pub struct QualityController {
    pub current_level: QualityLevel,
    pub min_fps: f32,
    pub frame_times: VecDeque<f32>,
    pub adaptation_rate: f32, // Gradual 10% changes
}

impl Default for QualityController {
    fn default() -> Self {
        Self {
            current_level: QualityLevel::High,
            min_fps: 60.0,
            frame_times: VecDeque::with_capacity(60), // 1 second at 60 FPS
            adaptation_rate: 0.1,
        }
    }
}

impl QualityController {
    pub fn adapt_quality(&mut self, current_fps: f32) {
        if current_fps < self.min_fps {
            self.decrease_quality();
        } else if current_fps > self.min_fps * 1.2 {
            self.increase_quality();
        }
    }

    fn decrease_quality(&mut self) {
        // Gradual parameter reduction
        match self.current_level {
            QualityLevel::Ultra => self.current_level = QualityLevel::High,
            QualityLevel::High => self.current_level = QualityLevel::Medium,
            QualityLevel::Medium => self.current_level = QualityLevel::Low,
            QualityLevel::Low => self.current_level = QualityLevel::Minimal,
            QualityLevel::Minimal => {} // Can't go lower
        }
    }

    fn increase_quality(&mut self) {
        // Gradual parameter increase
        match self.current_level {
            QualityLevel::Ultra => {} // Already at maximum
            QualityLevel::High => self.current_level = QualityLevel::Ultra,
            QualityLevel::Medium => self.current_level = QualityLevel::High,
            QualityLevel::Low => self.current_level = QualityLevel::Medium,
            QualityLevel::Minimal => self.current_level = QualityLevel::Low,
        }
    }
}
