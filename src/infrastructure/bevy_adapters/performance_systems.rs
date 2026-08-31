use super::components::*;
use crate::infrastructure::bevy_adapters::ui_components::VideoRecordingState;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot};

/// Request a Bevy framebuffer capture in every simulation mode. This belongs
/// to shared presentation infrastructure rather than solar-system input so
/// rocket validation captures the actual rendered terrain, not the desktop.
pub fn request_screenshot_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut screenshot_state: ResMut<ScreenshotState>,
    mut notifications: ResMut<NotificationQueue>,
) {
    if keyboard.just_pressed(KeyCode::F12) || keyboard.just_pressed(KeyCode::KeyP) {
        notifications.hide_for_screenshot = true;
        screenshot_state.pending = true;
    }
}

// Capture a screenshot on the frame after notifications are hidden.
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

/// Correctly measures frame time first, then derives FPS from it.
/// Uses exponential moving average for stability and responsiveness.
pub fn update_performance_stats(mut performance_stats: ResMut<PerformanceStats>) {
    let now = std::time::Instant::now();
    let frame_time_seconds = performance_stats
        .last_frame_time
        .map(|last_frame_time| now.duration_since(last_frame_time).as_secs_f64())
        .unwrap_or(1.0 / 60.0);
    performance_stats.last_frame_time = Some(now);
    let frame_time_ms = (frame_time_seconds * 1000.0) as f32;
    performance_stats.frame_time_ms = frame_time_ms;
    performance_stats.fps_raw = if frame_time_ms > 0.0 {
        1000.0 / frame_time_ms
    } else {
        0.0
    };
    const SMOOTHING_FACTOR: f32 = 0.1;
    performance_stats.fps_smoothed = performance_stats.fps_smoothed * (1.0 - SMOOTHING_FACTOR)
        + performance_stats.fps_raw * SMOOTHING_FACTOR;
    performance_stats.fps_display = performance_stats.fps_smoothed;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn performance_stats_initializes_fps_ema() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(PerformanceStats::default());
        app.add_systems(Update, update_performance_stats);
        app.update();

        let stats = app.world().resource::<PerformanceStats>();
        assert!(stats.fps_display.is_finite());
        assert!(stats.frame_time_ms.is_finite());
    }
}
