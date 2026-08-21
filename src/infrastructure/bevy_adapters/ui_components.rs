use bevy::prelude::*;

// Resource to track currently selected planet
#[derive(Resource)]
pub struct SelectedPlanet {
    pub entity: Option<Entity>,
    pub name: Option<String>,
}

// Resource to track hovered planet for information display
#[derive(Resource)]
pub struct HoveredPlanet {
    pub name: Option<String>,
    pub info: Option<String>,
}

// Resource to manage notification queue
#[derive(Resource)]
pub struct NotificationQueue {
    pub notifications: Vec<Notification>,
    pub hide_for_screenshot: bool, // Temporarily hide notifications during screenshot
}

// Resource to track pending screenshot capture
#[derive(Resource)]
pub struct ScreenshotState {
    pub pending: bool, // Screenshot requested, will capture next frame
}

// Resource to track video recording state
#[derive(Resource)]
pub struct VideoRecordingState {
    pub is_recording: bool,
    pub frame_count: u32,
    pub start_time: f64,
    pub output_dir: String,
}

impl Default for VideoRecordingState {
    fn default() -> Self {
        Self {
            is_recording: false,
            frame_count: 0,
            start_time: 0.0,
            output_dir: String::new(),
        }
    }
}

#[derive(Resource, Default, Clone, Copy)]
pub struct ZenMode {
    pub enabled: bool,
}

// Resource to track if UI is currently under the cursor
#[derive(Resource, Default)]
pub struct UiPointerState {
    pub is_over_ui: bool,
}

// Notification types for user feedback
#[derive(Clone, Debug)]
pub enum NotificationType {
    Success,
    Error,
    Info,
    Warning,
}

// Individual notification message
#[derive(Clone, Debug)]
pub struct Notification {
    pub message: String,
    pub notification_type: NotificationType,
    pub created_at: f32, // Time in seconds
    pub duration: f32,   // How long to display (seconds)
}

// Resource to track camera input state
#[derive(Resource)]
pub struct CameraInputState {
    pub last_input_time: f32,
    pub suppress_auto_inspect_for: Option<Entity>,
    pub last_selected_entity: Option<Entity>,
    pub earth_terrain_active: bool, // Track if Earth terrain view is currently active
}

impl Default for CameraInputState {
    fn default() -> Self {
        Self {
            last_input_time: -1000.0,
            suppress_auto_inspect_for: None,
            last_selected_entity: None,
            earth_terrain_active: false,
        }
    }
}

// Resource to track dynamic resolution scaling
#[derive(Resource)]
pub struct DynamicResolutionState {
    pub scale: f32,
    pub min_scale: f32,
    pub max_scale: f32,
    pub cooldown: f32,
}

impl Default for DynamicResolutionState {
    fn default() -> Self {
        Self {
            scale: 1.0,
            min_scale: 0.6,
            max_scale: 1.0,
            cooldown: 0.0,
        }
    }
}
