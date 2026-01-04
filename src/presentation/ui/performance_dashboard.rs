use crate::infrastructure::bevy_adapters::components::{PerformanceStats, QualityController, QualityLevel};
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

pub fn performance_dashboard_ui(
    mut contexts: EguiContexts,
    perf_stats: Res<PerformanceStats>,
    quality_controller: Res<QualityController>,
) {
    let ctx = contexts.ctx_mut();

    egui::Window::new("Performance Monitor")
        .default_pos([10.0, 10.0])
        .show(ctx, |ui| {
            // FPS indicator with color coding (using rolling average for stability)
            let display_fps = perf_stats.average_fps;
            let fps_color = if display_fps >= 60.0 {
                egui::Color32::GREEN
            } else if display_fps >= 45.0 {
                egui::Color32::YELLOW
            } else {
                egui::Color32::RED
            };

            ui.colored_label(fps_color, format!("FPS: {:.1}", display_fps));

            // Quality level indicator
            ui.label(format!("Quality: {:?}", quality_controller.current_level));

            // Frame time
            ui.label(format!("Frame Time: {:.2}ms", perf_stats.frame_time * 1000.0));

            // Performance history (simplified)
            ui.label(format!("Target FPS: {:.0}", quality_controller.min_fps));

            // Adaptive quality status
            let adaptive_status = if perf_stats.adaptive_enabled {
                "Enabled"
            } else {
                "Disabled"
            };
            ui.label(format!("Auto Quality: {}", adaptive_status));

            // CPU feature detection status
            ui.collapsing("System Info", |ui| {
                let cpu_features = crate::infrastructure::bevy_adapters::simd_kepler::detect_cpu_features();
                ui.label(format!("CPU Features: {:?}", cpu_features));

                // Placeholder for GPU compute status
                ui.label("WebGPU: Detecting...");
                ui.label("Web Workers: Detecting...");
            });
        });
}