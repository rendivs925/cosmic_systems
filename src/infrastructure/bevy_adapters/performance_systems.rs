use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use bevy::prelude::*;

// Performance monitoring and quality adaptation system
pub fn update_performance_monitor(
    mut perf_stats: ResMut<PerformanceStats>,
    mut quality_controller: ResMut<QualityController>,
    time: Res<Time>,
) {
    // Update frame time history
    perf_stats.frame_time = time.delta_seconds();
    quality_controller
        .frame_times
        .push_back(perf_stats.frame_time);

    if quality_controller.frame_times.len() > 60 {
        quality_controller.frame_times.pop_front();
    }

    // Calculate average FPS
    let avg_frame_time = quality_controller.frame_times.iter().sum::<f32>()
        / quality_controller.frame_times.len() as f32;
    perf_stats.fps = 1.0 / avg_frame_time;

    // Update quality level in PerformanceStats to match QualityController
    perf_stats.quality_level = quality_controller.current_level;

    // Gradual quality adaptation
    quality_controller.adapt_quality(perf_stats.fps);
}

// System to capture screenshot on next frame after notifications are hidden
pub fn take_pending_screenshot(
    mut screenshot_state: ResMut<ScreenshotState>,
    mut screenshot_manager: ResMut<bevy::render::view::screenshot::ScreenshotManager>,
    main_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    if !screenshot_state.pending {
        return;
    }

    screenshot_state.pending = false;

    let window_entity = main_window.single();

    // Create screenshots directory in home folder
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let screenshots_dir = format!("{}/cosmic_systems_images", home_dir);

    if let Err(e) = std::fs::create_dir_all(&screenshots_dir) {
        notifications.notifications.push(Notification {
            message: format!("Failed to create screenshots directory: {}", e),
            notification_type: NotificationType::Error,
            created_at: time.elapsed_seconds(),
            duration: 5.0,
        });
        return;
    }

    // Generate filename with timestamp
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let filename = format!("{}/cosmic_systems_{}.png", screenshots_dir, timestamp);

    // Take screenshot using Bevy's screenshot API
    match screenshot_manager.save_screenshot_to_disk(window_entity, filename.clone()) {
        Ok(_) => {
            notifications.notifications.push(Notification {
                message: format!("Screenshot saved to: {}", filename),
                notification_type: NotificationType::Success,
                created_at: time.elapsed_seconds(),
                duration: 4.0,
            });
        }
        Err(e) => {
            notifications.notifications.push(Notification {
                message: format!("Failed to save screenshot: {}", e),
                notification_type: NotificationType::Error,
                created_at: time.elapsed_seconds(),
                duration: 5.0,
            });
        }
    }
}

// System for adaptive quality control
pub fn adaptive_quality_system(
    mut perf_stats: ResMut<PerformanceStats>,
    mut quality_adapter: ResMut<QualityAdaptationResource>,
) {
    if !quality_adapter.enabled {
        return;
    }

    // Update frame time history for variance calculation
    let frame_time_ms = perf_stats.frame_time_ms;
    perf_stats.frame_time_history.push(frame_time_ms);
    if perf_stats.frame_time_history.len() > perf_stats.history_capacity {
        perf_stats.frame_time_history.remove(0);
    }

    // Run quality adaptation
    if let Some(new_quality) = quality_adapter.system.update_and_adapt(&mut perf_stats) {
        perf_stats.quality_level = new_quality;
        println!("🎚️ Quality adapted to: {:?}", new_quality);
    }

        // Log adaptation status less frequently - only when quality changes
        if perf_stats.frame_count % 600 == 0 { // Every 10 seconds
            println!("🎯 Quality: {:?} | FPS: {:.1} | GPU: {:.1}%",
                perf_stats.quality_level,
                perf_stats.fps_display,
                perf_stats.gpu_utilization * 100.0
            );
        }
}

// PRODUCTION-GRADE FPS MEASUREMENT (Industry Standard Implementation)
/// Correctly measures frame time first, then derives FPS from it.
/// Uses exponential moving average for stability and responsiveness.
pub fn update_performance_stats(
    _time: Res<Time>,
    mut performance_stats: ResMut<PerformanceStats>,
    mut solar_params: ResMut<SolarSystemParameters>,
    chrome: Option<Res<ChromeOptimizations>>,
) {
    // PRODUCTION-GRADE FRAME TIME MEASUREMENT
    // Use high-resolution monotonic clock for accurate timing
    let now = std::time::Instant::now();

    // Calculate frame time as difference from last frame
    let frame_time_seconds = if performance_stats.frame_count > 0 {
        now.duration_since(performance_stats.last_frame_time)
            .as_secs_f64()
    } else {
        // First frame - use target frame time as estimate
        1.0 / performance_stats.target_fps as f64
    };

    performance_stats.last_frame_time = now;

    // Convert to milliseconds (industry standard unit)
    let frame_time_ms = (frame_time_seconds * 1000.0) as f32;

    // PRIMARY METRIC: Frame time (this is the truth)
    performance_stats.frame_time_ms = frame_time_ms;

    // DERIVED METRIC: Raw FPS = 1/frame_time (jumps violently, not for display)
    performance_stats.fps_raw = if frame_time_ms > 0.0 {
        1000.0 / frame_time_ms
    } else {
        0.0 // Prevent division by zero
    };

    // EXPONENTIAL MOVING AVERAGE (Industry Standard Smoothing)
    // fps_smoothed = fps_smoothed * 0.9 + fps_raw * 0.1
    // - Stable: Doesn't jump around
    // - Responsive: Reacts to changes quickly
    // - Cheap: Single multiplication per frame
    const SMOOTHING_FACTOR: f32 = 0.1; // 0.1 = 10% new data, 90% history
    performance_stats.fps_smoothed = performance_stats.fps_smoothed * (1.0 - SMOOTHING_FACTOR)
        + performance_stats.fps_raw * SMOOTHING_FACTOR;

    // Frame time EMA (more stable than FPS for performance analysis)
    performance_stats.frame_time_smoothed = performance_stats.frame_time_smoothed
        * (1.0 - SMOOTHING_FACTOR)
        + performance_stats.frame_time_ms * SMOOTHING_FACTOR;

    // DISPLAY FPS (what users see - smoothed for human consumption)
    performance_stats.fps_display = performance_stats.fps_smoothed;

    // FRAME TIME STATISTICS (most important for performance analysis)
    // Update min/max frame times
    performance_stats.frame_time_min = performance_stats.frame_time_min.min(frame_time_ms);
    performance_stats.frame_time_max = performance_stats.frame_time_max.max(frame_time_ms);

    // Maintain frame time history for percentile calculations
    performance_stats.frame_time_history.push(frame_time_ms);
    if performance_stats.frame_time_history.len() > performance_stats.history_capacity {
        performance_stats.frame_time_history.remove(0); // Remove oldest
    }

    // Calculate 99th percentile frame time (stutter detection)
    if !performance_stats.frame_time_history.is_empty() {
        let mut sorted_times = performance_stats.frame_time_history.clone();
        sorted_times.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let percentile_index = ((sorted_times.len() - 1) as f32 * 0.99) as usize;
        performance_stats.frame_time_99th =
            sorted_times[percentile_index.min(sorted_times.len() - 1)];
    }

    // GPU TIMING (when available - Vulkan/WebGPU)
    // For now, assume GPU time ≈ CPU time (simplified)
    // TODO: Add actual GPU timestamp queries for Vulkan/WebGPU
    performance_stats.gpu_frame_time_ms = frame_time_ms; // Placeholder
    performance_stats.cpu_gpu_frame_time = frame_time_ms.max(performance_stats.gpu_frame_time_ms);

    // LEGACY COMPATIBILITY (deprecated fields)
    performance_stats.frame_time = performance_stats.frame_time_ms;
    performance_stats.fps = performance_stats.fps_display;
    performance_stats.average_frame_time = performance_stats.frame_time_smoothed;
    performance_stats.average_fps = performance_stats.fps_smoothed;

    // Update frame count
    performance_stats.frame_count += 1;

    // Chrome detection for adaptive rate adjustment
    if let Some(chrome) = chrome {
        performance_stats.adaptation_rate = if chrome.is_chrome { 0.05 } else { 0.1 };
    }

    // LEGACY: Maintain old rolling average for compatibility
    let fps_raw_copy = performance_stats.fps_raw; // Copy before mutable borrow ends
    performance_stats.frame_history.push_back(fps_raw_copy);
    if performance_stats.frame_history.len() > performance_stats.history_len {
        performance_stats.frame_history.pop_front();
    }

    // AUTOMATIC QUALITY ADJUSTMENT (based on frame time, not FPS)
    if performance_stats.adaptive_enabled {
        adjust_quality_based_on_performance(&mut performance_stats, &mut solar_params);
    }
}

// Automatic quality adjustment based on performance metrics
fn adjust_quality_based_on_performance(
    performance_stats: &mut PerformanceStats,
    solar_params: &mut SolarSystemParameters,
) {
    let target_fps = performance_stats.target_fps.max(1.0);
    let avg_fps = performance_stats.average_fps;
    let rate = performance_stats.adaptation_rate;

    let mut new_quality_level = performance_stats.quality_level;
    if avg_fps < target_fps * (1.0 - rate) {
        new_quality_level = match performance_stats.quality_level {
            QualityLevel::Ultra => QualityLevel::High,
            QualityLevel::High => QualityLevel::Medium,
            QualityLevel::Medium => QualityLevel::Low,
            QualityLevel::Low => QualityLevel::Minimal,
            QualityLevel::Minimal => QualityLevel::Minimal,
        };
    } else if avg_fps > target_fps * (1.0 + rate) {
        new_quality_level = match performance_stats.quality_level {
            QualityLevel::Ultra => QualityLevel::Ultra,
            QualityLevel::High => QualityLevel::Ultra,
            QualityLevel::Medium => QualityLevel::High,
            QualityLevel::Low => QualityLevel::Medium,
            QualityLevel::Minimal => QualityLevel::Low,
        };
    }

    if new_quality_level != performance_stats.quality_level {
        performance_stats.quality_level = new_quality_level;
        apply_quality_settings(new_quality_level, solar_params, avg_fps);
    }
}

// Apply quality settings based on the quality level
fn apply_quality_settings(quality_level: QualityLevel, solar_params: &mut SolarSystemParameters, current_fps: f32) {
    // Quality adaptation now preserves user-set time scale
    // Only adjust other quality parameters, not time scale
    match quality_level {
        QualityLevel::Ultra => {
            // Maximum quality - no performance optimizations
            println!("🎯 Performance excellent ({:.0} FPS) - Quality Ultra", current_fps);
        }
        QualityLevel::High => {
            // High quality with minimal optimizations
            println!("✅ Performance good ({:.0} FPS) - Quality High", current_fps);
        }
        QualityLevel::Medium => {
            // Balanced quality and performance
            println!("⚖️ Performance moderate ({:.0} FPS) - Quality Medium", current_fps);
        }
        QualityLevel::Low => {
            // Lower quality for better performance
            println!("⚡ Performance low ({:.0} FPS) - Quality Low", current_fps);
        }
        QualityLevel::Minimal => {
            // Minimum quality for maximum performance
            println!("🚀 Performance critical ({:.0} FPS) - Quality Minimal", current_fps);
        }
    }
}

/// PRODUCTION-GRADE PERFORMANCE LOGGING (Industry Standards)
/// Displays frame time (truth) and FPS (derived) with 99th percentile stutter detection
pub fn log_performance_stats(perf_stats: Res<PerformanceStats>, _time: Res<Time>) {
    // Log performance stats every 300 frames (5 seconds) - reduced noise
    if perf_stats.frame_count % 300 == 0 {
        // PRIMARY DISPLAY: Essential performance info only
        println!("🎯 PERF_STATS: FPS: {:.1} | Frame: {:.1}ms | Quality: {:?}",
            perf_stats.fps_display,
            perf_stats.frame_time_ms,
            perf_stats.quality_level
        );

        // Only show detailed GPU timing if Vulkan is active
        if perf_stats.vulkan_enabled && perf_stats.gpu_frame_time_ms > 0.0 {
            println!("🎮 GPU: {:.1}ms | Vulkan calls: {}",
                perf_stats.gpu_frame_time_ms,
                perf_stats.vulkan_kepler_calls
            );
        }

        // Show physics performance only if significant
        if perf_stats.physics_update_time > 0.1 {
            println!("⚛️ PHYSICS: {:.2}ms", perf_stats.physics_update_time);
        }
    }
}