use bevy::prelude::*;

// Frame metrics used by the shared FPS UI.
#[derive(Resource)]
pub struct PerformanceStats {
    pub frame_time_ms: f32,
    pub fps_raw: f32,
    pub fps_smoothed: f32,
    pub fps_display: f32,
    pub(crate) last_frame_time: Option<std::time::Instant>,
}

impl Default for PerformanceStats {
    fn default() -> Self {
        Self {
            frame_time_ms: 16.67,
            fps_raw: 60.0,
            fps_smoothed: 60.0,
            fps_display: 60.0,
            last_frame_time: None,
        }
    }
}
