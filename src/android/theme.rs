//! Mobile design tokens.
//!
//! One scale for spacing, type and colour so the loading screen, HUD overlays
//! and on-screen controls read as one system. Values are logical pixels (dp).
//! Touch targets follow the 48 dp minimum with a 56 dp default.

use bevy::prelude::*;

// Spacing scale (4 dp base).
pub const SPACE_1: f32 = 4.0;
pub const SPACE_2: f32 = 8.0;
pub const SPACE_3: f32 = 12.0;
pub const SPACE_4: f32 = 16.0;
pub const SPACE_6: f32 = 24.0;
pub const SPACE_8: f32 = 32.0;

// Corner radii.
pub const RADIUS_SM: f32 = 8.0;
pub const RADIUS_MD: f32 = 12.0;
pub const RADIUS_LG: f32 = 20.0;
pub const RADIUS_FULL: f32 = 999.0;

/// Proportional interface typeface (see assets/fonts/LICENSE.md).
pub const FONT_PATH: &str = "fonts/Roboto-Regular.ttf";

// Type scale.
pub const FONT_DISPLAY: f32 = 36.0;
pub const FONT_TITLE: f32 = 22.0;
pub const FONT_BODY: f32 = 16.0;
pub const FONT_CAPTION: f32 = 13.0;
pub const FONT_MICRO: f32 = 11.0;

// Touch metrics.
pub const TOUCH_MIN: f32 = 48.0;
pub const TOUCH_HEIGHT: f32 = 52.0;
pub const TOUCH_WIDTH: f32 = 64.0;
pub const TOUCH_PRIMARY_HEIGHT: f32 = 64.0;
pub const TOUCH_PRIMARY_WIDTH: f32 = 120.0;

// Safe area allowances for the status bar / gesture bar. The activity uses a
// fullscreen theme, so these are conservative paddings rather than insets.
pub const SAFE_TOP: f32 = 24.0;
pub const SAFE_BOTTOM: f32 = 20.0;

// Colours — space-themed palette with layered surfaces.
pub const SURFACE: Color = Color::srgba(0.04, 0.05, 0.08, 0.94);
pub const SURFACE_PANEL: Color = Color::srgba(0.04, 0.05, 0.08, 0.72);
pub const SURFACE_RAISED: Color = Color::srgba(0.14, 0.17, 0.24, 0.92);
pub const SURFACE_PRESSED: Color = Color::srgba(0.24, 0.45, 0.78, 0.98);
pub const SURFACE_PRIMARY: Color = Color::srgba(0.20, 0.44, 0.96, 0.96);
pub const TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.08);
pub const ACCENT: Color = Color::srgb(0.40, 0.68, 1.0);
pub const ACCENT_WARM: Color = Color::srgb(1.0, 0.72, 0.30);
pub const TEXT_PRIMARY: Color = Color::srgb(0.97, 0.98, 1.0);
pub const TEXT_SECONDARY: Color = Color::srgba(0.72, 0.78, 0.88, 0.78);
pub const TEXT_DIM: Color = Color::srgba(0.55, 0.60, 0.70, 0.60);
pub const DANGER: Color = Color::srgb(0.96, 0.32, 0.32);

// Fade timing: how long the loading screen takes to dissolve into the scene.
pub const FADE_OUT_SECONDS: f32 = 0.45;
