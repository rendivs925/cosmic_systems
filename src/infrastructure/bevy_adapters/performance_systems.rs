use super::components::*;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

// Performance monitoring and quality adaptation system
pub fn update_performance_monitor(
    mut perf_stats: ResMut<PerformanceStats>,
    mut quality_controller: ResMut<QualityController>,
    time: Res<Time>,
) {
    // Update frame time history
    perf_stats.frame_time = time.delta_secs();
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
// TODO: Re-implement with Bevy 0.17 screenshot API
pub fn take_pending_screenshot(
    mut commands: Commands,
    mut screenshot_state: ResMut<ScreenshotState>,
    video_state: Res<VideoRecordingState>,
    main_window: Query<Entity, With<bevy::window::PrimaryWindow>>,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    if !screenshot_state.pending {
        return;
    }

    screenshot_state.pending = false;

    let _window_entity = main_window.single();

    // Determine output directory based on recording state
    let (output_dir, filename_prefix) =
        if video_state.is_recording && !video_state.output_dir.is_empty() {
            (
                video_state.output_dir.clone(),
                format!("frame_{:06}", video_state.frame_count),
            )
        } else {
            let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            (
                format!("{}/cosmic_systems_images", home_dir),
                "screenshot".to_string(),
            )
        };

    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        notifications.notifications.push(Notification {
            message: format!("Failed to create output directory: {}", e),
            notification_type: NotificationType::Error,
            created_at: time.elapsed_secs(),
            duration: 5.0,
        });
        return;
    }

    // Generate filename with timestamp
    let filename = if video_state.is_recording {
        format!("{}/{}.png", output_dir, filename_prefix)
    } else {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        format!("{}/cosmic_systems_{}.png", output_dir, timestamp)
    };

    // Take screenshot using Bevy's new screenshot API
    commands
        .spawn(Screenshot::primary_window())
        .observe(save_to_disk(filename.clone()));

    // Show notification for screenshot taken
    if !video_state.is_recording {
        notifications.notifications.push(Notification {
            message: format!("Screenshot saved to: {}", filename),
            notification_type: NotificationType::Success,
            created_at: time.elapsed_secs(),
            duration: 4.0,
        });
    }
}

// System to handle video recording frame capture
pub fn handle_video_recording(
    mut video_state: ResMut<VideoRecordingState>,
    mut screenshot_state: ResMut<ScreenshotState>,
    time: Res<Time>,
) {
    if !video_state.is_recording {
        return;
    }

    // Capture frame every frame for video recording
    // This will create a sequence of images that can be converted to video
    if video_state.frame_count == 0 {
        video_state.start_time = time.elapsed_secs_f64();
    }

    video_state.frame_count += 1;
    screenshot_state.pending = true; // Trigger screenshot capture
}

// System to start/stop video recording
pub fn toggle_video_recording(
    keyboard_input: Res<ButtonInput<KeyCode>>,
    mut video_state: ResMut<VideoRecordingState>,
    mut notifications: ResMut<NotificationQueue>,
    time: Res<Time>,
) {
    if keyboard_input.just_pressed(KeyCode::KeyV) && keyboard_input.pressed(KeyCode::ControlLeft) {
        if video_state.is_recording {
            // Stop recording and start conversion
            video_state.is_recording = false;
            let duration = time.elapsed_secs_f64() - video_state.start_time;
            let frame_count = video_state.frame_count;
            let output_dir = video_state.output_dir.clone();

            notifications.notifications.push(Notification {
                message: format!(
                    "Video recording stopped. Converting {} frames to MP4...",
                    frame_count
                ),
                notification_type: NotificationType::Info,
                created_at: time.elapsed_secs(),
                duration: 3.0,
            });

            // Spawn a task to convert frames to MP4
            let _notifications_clone = notifications.notifications.clone();
            std::thread::spawn(move || {
                convert_frames_to_mp4(&output_dir, frame_count, duration);
            });

            // Reset state
            video_state.frame_count = 0;
            video_state.start_time = 0.0;
            video_state.output_dir.clear();
        } else {
            // Start recording
            video_state.is_recording = true;
            video_state.frame_count = 0;

            // Create video directory
            let home_dir = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            video_state.output_dir =
                format!("{}/cosmic_systems_videos/recording_{}", home_dir, timestamp);

            if let Err(e) = std::fs::create_dir_all(&video_state.output_dir) {
                notifications.notifications.push(Notification {
                    message: format!("Failed to create video directory: {}", e),
                    notification_type: NotificationType::Error,
                    created_at: time.elapsed_secs(),
                    duration: 5.0,
                });
                video_state.is_recording = false;
                return;
            }

            notifications.notifications.push(Notification {
                message: "Video recording started. Press Ctrl+V to stop.".to_string(),
                notification_type: NotificationType::Info,
                created_at: time.elapsed_secs(),
                duration: 3.0,
            });
        }
    }
}

// Function to convert frame sequence to MP4 using ffmpeg
fn convert_frames_to_mp4(output_dir: &str, frame_count: u32, duration: f64) {
    // Check if ffmpeg is available
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_err()
    {
        eprintln!("ffmpeg not found. Please install ffmpeg to enable automatic MP4 conversion.");
        eprintln!("Manual conversion: ffmpeg -framerate 60 -i {}/frame_%06d.png -c:v libx264 {}/output.mp4", output_dir, output_dir);
        return;
    }

    let input_pattern = format!("{}/frame_%06d.png", output_dir);
    let output_file = format!("{}/cosmic_recording.mp4", output_dir);

    // Calculate appropriate framerate based on duration and frame count
    let target_framerate = if duration > 0.0 {
        (frame_count as f64 / duration).clamp(24.0, 60.0)
    } else {
        60.0
    };

    println!(
        "Converting {} frames to MP4 at {:.1} FPS...",
        frame_count, target_framerate
    );

    let result = std::process::Command::new("ffmpeg")
        .args([
            "-framerate",
            &format!("{}", target_framerate as u32),
            "-i",
            &input_pattern,
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
            "22", // Good quality/size balance
            "-pix_fmt",
            "yuv420p", // Compatible with most players
            "-y",      // Overwrite output file
            &output_file,
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            println!("MP4 conversion successful! Video saved to: {}", output_file);

            // Clean up PNG frames to save disk space
            if let Ok(entries) = std::fs::read_dir(output_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(extension) = path.extension() {
                        if extension == "png"
                            && path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.starts_with("frame_"))
                        {
                            let _ = std::fs::remove_file(&path); // Ignore errors
                        }
                    }
                }
            }

            println!(
                "Cleaned up {} PNG frames. Final video: {}",
                frame_count, output_file
            );
        }
        Ok(output) => {
            eprintln!(
                "ffmpeg conversion failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            eprintln!(
                "Manual conversion: ffmpeg -framerate {} -i {} -c:v libx264 {} -y",
                target_framerate as u32, input_pattern, output_file
            );
        }
        Err(e) => {
            eprintln!("Failed to run ffmpeg: {}", e);
            eprintln!(
                "Manual conversion: ffmpeg -framerate {} -i {} -c:v libx264 {} -y",
                target_framerate as u32, input_pattern, output_file
            );
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
        println!("Quality adapted to: {:?}", new_quality);
    }

    // Minimal quality logging - only when quality changes significantly
    // Removed frequent quality status logging to reduce noise
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
pub fn apply_quality_settings(
    quality_level: QualityLevel,
    _solar_params: &mut SolarSystemParameters,
    _current_fps: f32,
) {
    // Quality adaptation now preserves user-set time scale
    // Only adjust other quality parameters, not time scale
    // Quality change notifications removed to reduce console noise
    let _ = quality_level;
}

/// PRODUCTION-GRADE PERFORMANCE LOGGING (Industry Standards)
/// Displays frame time (truth) and FPS (derived) with 99th percentile stutter detection
pub fn log_performance_stats(_perf_stats: Res<PerformanceStats>, _time: Res<Time>) {
    // Ultra-minimal logging - silent operation
    // Performance monitoring completely silent for elegant experience
}

pub fn cap_fixed_overstep(mut fixed_time: ResMut<Time<Fixed>>) {
    let max_overstep =
        std::time::Duration::from_secs_f32(fixed_time.timestep().as_secs_f32() * 2.0);
    let overstep = fixed_time.overstep();
    if overstep > max_overstep {
        fixed_time.discard_overstep(overstep - max_overstep);
    }
}
