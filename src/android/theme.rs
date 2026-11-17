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

/// Proportional interface typeface (see assets/fonts/LICENSE.md).
pub const FONT_PATH: &str = "fonts/Roboto-Regular.ttf";

// Type scale.
pub const FONT_DISPLAY: f32 = 32.0;
pub const FONT_TITLE: f32 = 22.0;
pub const FONT_BODY: f32 = 16.0;
pub const FONT_CAPTION: f32 = 13.0;

// Touch metrics.
pub const TOUCH_MIN: f32 = 48.0;
pub const TOUCH_HEIGHT: f32 = 52.0;
pub const TOUCH_WIDTH: f32 = 64.0;

// Safe area allowances for the status bar / gesture bar. The activity uses a
// fullscreen theme, so these are conservative paddings rather than insets.
pub const SAFE_TOP: f32 = 24.0;
pub const SAFE_BOTTOM: f32 = 20.0;

// Colours.
pub const SURFACE: Color = Color::srgba(0.055, 0.065, 0.09, 0.96);
pub const SURFACE_RAISED: Color = Color::srgba(0.13, 0.15, 0.20, 0.96);
pub const SURFACE_PRESSED: Color = Color::srgba(0.24, 0.45, 0.78, 0.98);
pub const TRACK: Color = Color::srgba(1.0, 1.0, 1.0, 0.14);
pub const ACCENT: Color = Color::srgb(0.36, 0.62, 0.98);
pub const TEXT_PRIMARY: Color = Color::srgb(0.97, 0.98, 1.0);
pub const TEXT_SECONDARY: Color = Color::srgba(0.78, 0.82, 0.90, 0.86);
