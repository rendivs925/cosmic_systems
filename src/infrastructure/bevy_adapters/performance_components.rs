use bevy::prelude::*;

// Quality levels for automatic adjustment
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QualityLevel {
    Ultra,   // Highest quality, no performance optimizations
    High,    // High quality with minimal optimizations
    Medium,  // Balanced quality and performance
    Low,     // Lower quality for better performance
    Minimal, // Minimum quality for maximum performance
}

// Performance monitoring and quality adjustment
#[derive(Resource)]
pub struct PerformanceStats {
    // PRODUCTION-GRADE FPS MEASUREMENT (per industry standards)
    // Frame time is the truth - FPS is derived
    pub frame_time_ms: f32, // Current frame time in milliseconds (PRIMARY METRIC)
    pub fps_raw: f32,       // Raw FPS = 1/frame_time (jumps violently, not for display)
    pub fps_smoothed: f32,  // Exponential moving average FPS (stable, responsive)
    pub fps_display: f32,   // FPS to show users (smoothed, human-friendly)

    // Frame time statistics (most important for performance analysis)
    pub frame_time_smoothed: f32, // EMA frame time in ms (stable metric)
    pub frame_time_99th: f32,     // 99th percentile frame time (stutter detection)
    pub frame_time_min: f32,      // Minimum frame time this session
    pub frame_time_max: f32,      // Maximum frame time this session

    // GPU timing. `NaN` means no timestamp-query instrumentation is available.
    pub gpu_frame_time_ms: f32,
    // Wall-clock frame time when GPU timing is unavailable; otherwise the max.
    pub cpu_gpu_frame_time: f32,

    // High-precision timing infrastructure
    pub last_frame_time: std::time::Instant, // Monotonic high-res clock timestamp
    pub frame_time_history: Vec<f32>,        // Raw frame times for percentile calculation
    pub history_capacity: usize, // Frame history size (default 1000 for 99th percentile)

    // Session and configuration
    pub frame_count: u64,            // Total frames rendered
    pub quality_level: QualityLevel, // Current quality setting
    pub target_fps: f32,             // Target FPS for quality adjustment
    pub adaptive_enabled: bool,      // Whether automatic quality adjustment is enabled
    pub adaptation_rate: f32,

    // Detailed optimization timing (for benchmarking)
    pub kepler_solve_time: f32, // Time spent solving Kepler equations (ms)
    pub physics_update_time: f32, // Total physics update time (ms)
    pub rendering_time: f32,    // Rendering time (ms)
    pub material_update_time: f32, // Material property updates (ms)
    pub orbit_visual_time: f32, // Orbit visualization updates (ms)
    pub ui_update_time: f32,    // UI update time (ms)

    // Optimization-specific metrics
    pub adaptive_kepler_calls: u64, // Number of adaptive Kepler calls
    pub full_precision_kepler: u64, // Full precision (8 iterations)
    pub half_precision_kepler: u64, // Half precision (4 iterations)
    pub quarter_precision_kepler: u64, // Quarter precision (2 iterations)
    pub minimal_precision_kepler: u64, // Minimal precision (1 iteration)
    pub vulkan_kepler_calls: u64,   // Number of Vulkan Kepler calls

    // SIMD and parallel processing metrics
    pub simd_enabled: bool,     // Whether SIMD is active
    pub parallel_enabled: bool, // Whether parallel processing is active
    pub cpu_cores_used: usize,  // Number of CPU cores utilized
    pub vector_width: usize,    // SIMD vector width (128, 256, 512 bits)

    // Process memory metrics. `NaN` means this platform has no collector.
    pub memory_usage_mb: f32,
    pub peak_memory_mb: f32,

    // Benchmark timing accumulators
    pub benchmark_start_time: Option<std::time::Instant>,
    pub benchmark_frame_count: u64,
    pub benchmark_total_time: f32,

    // Advanced Quality Adaptation System
    pub gpu_utilization: f32,        // GPU utilization percentage (0.0-1.0)
    pub cpu_utilization: f32,        // CPU utilization percentage (0.0-1.0)
    pub memory_pressure: f32,        // Memory pressure (0.0-1.0)
    pub frame_time_variance: f32,    // Frame time variance (ms²)
    pub adaptive_confidence: f32,    // Confidence in adaptation decisions (0.0-1.0)
    pub quality_trend: QualityTrend, // Current quality adjustment trend
    pub last_quality_adjustment: std::time::Instant, // Time of last quality change
    pub adaptation_cooldown_ms: u64, // Minimum time between adjustments (ms)
}

#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub enum QualityTrend {
    Stable,    // Quality is stable, no changes needed
    Improving, // Performance improving, can increase quality
    Degrading, // Performance degrading, should reduce quality
    Critical,  // Critical performance issues, immediate quality reduction
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            // Production-grade FPS measurement (primary metrics)
            frame_time_ms: 16.67, // Assume 60 FPS initially
            fps_raw: 60.0,        // Raw FPS
            fps_smoothed: 60.0,   // EMA smoothed FPS
            fps_display: 60.0,    // Display FPS (smoothed)

            // Frame time statistics
            frame_time_smoothed: 16.67, // EMA frame time
            frame_time_99th: 16.67,     // 99th percentile (initially same)
            frame_time_min: 16.67,      // Min frame time
            frame_time_max: 16.67,      // Max frame time

            // No GPU timestamp-query implementation is registered yet.
            gpu_frame_time_ms: f32::NAN,
            cpu_gpu_frame_time: 16.67,

            // High-precision timing
            last_frame_time: std::time::Instant::now(),
            frame_time_history: Vec::with_capacity(1000),
            history_capacity: 1000, // Keep 1000 samples for 99th percentile

            // Session configuration
            frame_count: 0,
            quality_level: QualityLevel::High,
            target_fps: 60.0,
            // Rendering quality is an explicit user setting. Do not silently
            // degrade terrain or presentation during a demanding scene.
            adaptive_enabled: false,
            adaptation_rate: 0.1,

            // Detailed optimization timing
            kepler_solve_time: 0.0,
            physics_update_time: 0.0,
            rendering_time: 0.0,
            material_update_time: 0.0,
            orbit_visual_time: 0.0,
            ui_update_time: 0.0,

            // Optimization-specific metrics
            adaptive_kepler_calls: 0,
            full_precision_kepler: 0,
            half_precision_kepler: 0,
            quarter_precision_kepler: 0,
            minimal_precision_kepler: 0,
            vulkan_kepler_calls: 0,

            // SIMD and parallel processing metrics
            simd_enabled: false,
            parallel_enabled: false,
            cpu_cores_used: 1,
            vector_width: 128,

            // No native process-memory collector is registered yet.
            memory_usage_mb: f32::NAN,
            peak_memory_mb: f32::NAN,

            // Benchmark timing
            benchmark_start_time: None,
            benchmark_frame_count: 0,
            benchmark_total_time: 0.0,

            // Advanced Quality Adaptation System
            gpu_utilization: f32::NAN,
            cpu_utilization: 0.0, // CPU utilization (0.0-1.0)
            memory_pressure: f32::NAN,
            frame_time_variance: 0.0, // Frame time variance (ms²)
            adaptive_confidence: 0.5, // Initial confidence
            quality_trend: QualityTrend::Stable,
            last_quality_adjustment: std::time::Instant::now(),
            adaptation_cooldown_ms: 1000, // 1 second minimum between adjustments
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_adaptation_is_disabled_by_default() {
        assert!(!PerformanceStats::default().adaptive_enabled);
    }
}
