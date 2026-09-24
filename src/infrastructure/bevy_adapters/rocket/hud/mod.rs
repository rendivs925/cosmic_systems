// Rocket HUD UI - encapsulated, type-driven design.

use super::components::*;
use super::hud_units::HudUnits;
use super::telemetry::RocketEventFeed;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::ui_components::ZenMode;
use bevy::prelude::*;
use bevy::ui::Display as UiDisplay;

mod format;
mod layout;

use format::FieldFormatters;
pub use layout::{spawn_rocket_hud, HudBuilder};

/// HUD panel types for different display regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudPanel {
    /// Secondary flight data card (left).
    Left,
    /// Primary glanceable gauge strip (right).
    Right,
}

/// HUD field identifier for type-safe updates. The value entities carry this
/// key; static labels are plain text and never depend on telemetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudField {
    // Primary gauge strip
    AltitudeAgl,
    VelocityVertical,
    VelocityTotal,
    MachNumber,
    DynamicPressure,
    GLoad,
    Throttle,
    // Vehicle
    Stage,
    MissionPhase,
    Mass,
    Thrust,
    PropellantFraction,
    DeltaV,
    TwRatio,
    // Navigation / orbit
    AltitudeMsl,
    RadarAltitude,
    VelocityHorizontal,
    Apoapsis,
    Periapsis,
    SemiMajorAxis,
    Eccentricity,
    Inclination,
    RaanDeg,
    ArgPeriapsis,
    TrueAnomaly,
    OrbitalPeriod,
    // Attitude / control
    AngleOfAttack,
    BankAngle,
    AngularRates,
    Gimbal,
    // Thermal
    HeatFlux,
    Ablation,
    PlasmaBlackout,
    // Recovery
    Parachute,
    SurfaceType,
    TouchdownScorecard,
    // Meta / status
    TimeAndCamera,
    EventTimeline,
    Warnings,
}

/// Color scheme for HUD elements.
#[derive(Debug, Clone, Copy)]
pub struct HudColors {
    pub bright: Color,
    pub dim: Color,
    pub warning: Color,
    pub caution: Color,
    pub success: Color,
    pub danger: Color,
}

impl Default for HudColors {
    fn default() -> Self {
        Self {
            bright: Color::srgb(0.82, 0.90, 1.0),
            dim: Color::srgb(0.52, 0.61, 0.72),
            warning: Color::srgb(1.0, 0.80, 0.20),
            caution: Color::srgb(1.0, 0.55, 0.20),
            success: Color::srgb(0.35, 0.95, 0.45),
            danger: Color::srgb(1.0, 0.30, 0.28),
        }
    }
}

/// HUD color accessibility palette. The high-contrast variant replaces the
/// red/green pairing with magenta/blue so the good/bad distinction survives
/// red-green color vision deficiency (AGENTS.md section 29 presentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HudPalette {
    #[default]
    Standard,
    HighContrast,
}

impl HudColors {
    pub fn for_palette(palette: HudPalette) -> Self {
        match palette {
            HudPalette::Standard => Self::default(),
            HudPalette::HighContrast => Self {
                bright: Color::WHITE,
                dim: Color::srgb(0.78, 0.80, 0.84),
                warning: Color::srgb(1.0, 0.86, 0.10),
                caution: Color::srgb(1.0, 0.80, 0.20),
                success: Color::srgb(0.30, 0.72, 1.0),
                danger: Color::srgb(1.0, 0.36, 0.78),
            },
        }
    }
}

/// Presentation-only HUD display settings: unit system, accessibility palette,
/// and text scale. Never part of simulation state.
#[derive(Resource, Debug, Clone, Copy)]
pub struct HudDisplaySettings {
    pub units: HudUnits,
    pub palette: HudPalette,
    pub text_scale: f32,
}

impl Default for HudDisplaySettings {
    fn default() -> Self {
        Self {
            units: HudUnits::Metric,
            palette: HudPalette::Standard,
            text_scale: 1.0,
        }
    }
}

/// Bounds the operator can select with the HUD text-scale keys.
pub const HUD_TEXT_SCALE_MIN: f32 = 0.8;
pub const HUD_TEXT_SCALE_MAX: f32 = 1.6;
pub const HUD_TEXT_SCALE_STEP: f32 = 0.1;

/// Base (unscaled) font size of one HUD text entity, so text scale can be
/// applied and reversed without storing per-row state elsewhere.
#[derive(Component, Debug, Clone, Copy)]
pub struct HudBaseFontSize(pub f32);

/// Role of a static HUD text entity so the accessibility palette can recolor
/// labels/headers/titles without touching dynamic value colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudStaticRole {
    Label,
    Header,
    Title,
}

#[derive(Component, Debug, Clone, Copy)]
pub struct HudStaticText(pub HudStaticRole);

/// Text style configuration.
#[derive(Debug, Clone, Copy)]
pub struct TextStyle {
    pub font_size: f32,
    pub color: Color,
}

impl TextStyle {
    pub fn new(font_size: f32, color: Color) -> Self {
        Self { font_size, color }
    }
}

const PANEL_BG: Color = Color::srgba(0.02, 0.03, 0.06, 0.72);
const PANEL_BORDER: Color = Color::srgba(0.20, 0.30, 0.50, 0.35);
const FLIGHT_PANEL_WIDTH_PX: f32 = 248.0;
const PRIMARY_PANEL_WIDTH_PX: f32 = 200.0;
const PANEL_MARGIN_PX: f32 = 12.0;
const LABEL_FONT_PX: f32 = 10.0;
const VALUE_FONT_PX: f32 = 12.0;
const PRIMARY_VALUE_FONT_PX: f32 = 19.0;

#[derive(Component, Debug)]
pub struct RocketHudMarker {
    pub panel: HudPanel,
    pub field: HudField,
}

/// Root node of a HUD panel. Toggling its display hides the whole panel.
#[derive(Component, Debug, Clone, Copy)]
pub struct HudRoot {
    pub panel: HudPanel,
}

/// A diagnostic row hidden unless the operator enables the detail view.
#[derive(Component, Debug, Clone, Copy)]
pub struct HudDetailRow;

/// The event timeline rows, toggled independently of the diagnostic detail.
#[derive(Component, Debug, Clone, Copy)]
pub struct HudTimelineRow;

/// Presentation-only HUD options (never part of simulation state).
#[derive(Resource, Debug, Clone, Copy)]
pub struct HudOptions {
    /// Show diagnostic rows (orbital elements, attitude, thermal, recovery).
    pub detail: bool,
    /// Show the flight event timeline.
    pub timeline: bool,
}

impl Default for HudOptions {
    fn default() -> Self {
        Self {
            detail: false,
            timeline: true,
        }
    }
}

/// Spawn the complete rocket HUD.
/// Flash rate of the blackout banner while the link is down (Hz).
const BLACKOUT_FLASH_HZ: f32 = 2.0;
const HUD_UPDATE_INTERVAL_S: f32 = 1.0 / 30.0;

#[derive(Default)]
pub(crate) struct HudUpdateState {
    initialized: bool,
    last_update_real_time_s: f32,
}

/// System to update all HUD fields from telemetry.
#[expect(
    clippy::too_many_arguments,
    reason = "The cadence-limited HUD writer reads telemetry, camera mode, display settings, clock, and event feed before writing bounded text nodes."
)]
pub(crate) fn update_rocket_hud_system(
    telemetry: Res<RocketTelemetry>,
    camera_mode: Res<RocketCameraMode>,
    display: Res<HudDisplaySettings>,
    time: Res<Time>,
    sim_time: Res<SimulationTime>,
    event_feed: Res<RocketEventFeed>,
    mut hud_query: Query<(&RocketHudMarker, &mut Text, &mut TextColor)>,
    mut update_state: Local<HudUpdateState>,
) {
    if !hud_update_due(&update_state, time.elapsed_secs()) {
        return;
    }
    update_state.initialized = true;
    update_state.last_update_real_time_s = time.elapsed_secs();

    // Presentation-only flash phase for the blackout banner.
    let flash_on = ((time.elapsed_secs() * BLACKOUT_FLASH_HZ) as usize).is_multiple_of(2);
    let colors = HudColors::for_palette(display.palette);
    for (marker, mut text, mut text_color) in hud_query.iter_mut() {
        let (formatted, color) = FieldFormatters::format_field(
            marker.field,
            &telemetry,
            &camera_mode,
            flash_on,
            &event_feed,
            sim_time.time_acceleration,
            sim_time.pending_simulation_s(),
            &colors,
            display.units,
        );
        if text.0 != formatted {
            text.0 = formatted;
        }
        if text_color.0 != color {
            text_color.0 = color;
        }
    }
}

/// H toggles diagnostics, L the event timeline, Z Zen mode, U metric/imperial
/// units, N the accessibility palette, and `[`/`]` the HUD text scale.
pub fn toggle_hud_options_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut options: ResMut<HudOptions>,
    mut display: ResMut<HudDisplaySettings>,
    mut zen_mode: ResMut<ZenMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyH) {
        options.detail = !options.detail;
    }
    if keyboard.just_pressed(KeyCode::KeyL) {
        options.timeline = !options.timeline;
    }
    if keyboard.just_pressed(KeyCode::KeyZ) {
        zen_mode.enabled = !zen_mode.enabled;
    }
    if keyboard.just_pressed(KeyCode::KeyU) {
        display.units = match display.units {
            HudUnits::Metric => HudUnits::Imperial,
            HudUnits::Imperial => HudUnits::Metric,
        };
    }
    if keyboard.just_pressed(KeyCode::KeyN) {
        display.palette = match display.palette {
            HudPalette::Standard => HudPalette::HighContrast,
            HudPalette::HighContrast => HudPalette::Standard,
        };
    }
    if keyboard.just_pressed(KeyCode::BracketLeft) {
        display.text_scale = (display.text_scale - HUD_TEXT_SCALE_STEP)
            .clamp(HUD_TEXT_SCALE_MIN, HUD_TEXT_SCALE_MAX);
    }
    if keyboard.just_pressed(KeyCode::BracketRight) {
        display.text_scale = (display.text_scale + HUD_TEXT_SCALE_STEP)
            .clamp(HUD_TEXT_SCALE_MIN, HUD_TEXT_SCALE_MAX);
    }
}

/// Apply the presentation text scale to every HUD text entity from its stored
/// base size. Runs only when the display settings change.
pub fn apply_hud_text_scale_system(
    display: Res<HudDisplaySettings>,
    mut text_query: Query<(&HudBaseFontSize, &mut TextFont)>,
) {
    if !display.is_changed() {
        return;
    }
    for (base, mut font) in &mut text_query {
        let scaled = base.0 * display.text_scale;
        if (font.font_size - scaled).abs() > f32::EPSILON {
            font.font_size = scaled;
        }
    }
}

/// Recolor static labels/headers/titles when the accessibility palette
/// changes. Dynamic value colors are produced by the HUD update system, which
/// already reads the active palette.
pub fn apply_hud_palette_system(
    display: Res<HudDisplaySettings>,
    mut query: Query<(&HudStaticText, &mut TextColor)>,
) {
    if !display.is_changed() {
        return;
    }
    let colors = HudColors::for_palette(display.palette);
    for (static_text, mut color) in &mut query {
        let target = match static_text.0 {
            HudStaticRole::Label | HudStaticRole::Header => colors.dim,
            HudStaticRole::Title => colors.bright,
        };
        if color.0 != target {
            color.0 = target;
        }
    }
}

/// Apply presentation visibility: Zen mode hides everything; detail option
/// shows or hides diagnostic rows.
#[expect(
    clippy::type_complexity,
    reason = "The disjoint root/detail/timeline node queries make Bevy access explicit and prevent Node aliasing."
)]
pub fn apply_hud_visibility_system(
    options: Res<HudOptions>,
    zen_mode: Res<ZenMode>,
    mut roots: Query<(&HudRoot, &mut Node), (Without<HudDetailRow>, Without<HudTimelineRow>)>,
    mut detail_rows: Query<&mut Node, (With<HudDetailRow>, Without<HudTimelineRow>)>,
    mut timeline_rows: Query<&mut Node, (With<HudTimelineRow>, Without<HudDetailRow>)>,
) {
    let visible = UiDisplay::Flex;
    let hidden = UiDisplay::None;

    let hud_visible = !zen_mode.enabled;
    for (_root, mut node) in &mut roots {
        node.display = if hud_visible { visible } else { hidden };
    }
    let show_detail = options.detail && hud_visible;
    for mut node in &mut detail_rows {
        node.display = if show_detail { visible } else { hidden };
    }
    let show_timeline = options.timeline && hud_visible;
    for mut node in &mut timeline_rows {
        node.display = if show_timeline { visible } else { hidden };
    }
}

fn hud_update_due(state: &HudUpdateState, now_s: f32) -> bool {
    !state.initialized || now_s >= state.last_update_real_time_s + HUD_UPDATE_INTERVAL_S
}

/// System to spawn HUD on startup.
pub fn spawn_rocket_hud_system(commands: Commands) {
    spawn_rocket_hud(commands);
}

#[cfg(test)]
mod tests {
    use super::format::FieldFormatters;
    use super::*;

    #[test]
    fn hud_updates_are_cadence_limited() {
        let state = HudUpdateState {
            initialized: true,
            last_update_real_time_s: 2.0,
        };
        assert!(!hud_update_due(&state, 2.01));
        assert!(hud_update_due(&state, 2.0 + HUD_UPDATE_INTERVAL_S));
    }

    #[test]
    fn fuel_field_reports_active_stage_and_total() {
        let telemetry = RocketTelemetry {
            active_stage: 0,
            active_stage_propellant_fraction: 0.875,
            total_propellant_kg: 105_000.0,
            ..default()
        };
        let (text, _) = FieldFormatters::format_field(
            HudField::PropellantFraction,
            &telemetry,
            &RocketCameraMode::default(),
            false,
            &RocketEventFeed::default(),
            1.0,
            0.0,
            &HudColors::default(),
            HudUnits::Metric,
        );
        assert_eq!(text, "87.5%  S1  105.0 t");
    }

    #[test]
    fn orbital_fields_render_unavailable_before_flight() {
        let telemetry = RocketTelemetry {
            orbital_eccentricity: f64::NAN,
            orbital_inclination_deg: f64::NAN,
            orbital_period_s: f64::NAN,
            ..default()
        };
        for field in [
            HudField::Eccentricity,
            HudField::Inclination,
            HudField::OrbitalPeriod,
        ] {
            let (text, _) = FieldFormatters::format_field(
                field,
                &telemetry,
                &RocketCameraMode::default(),
                false,
                &RocketEventFeed::default(),
                1.0,
                0.0,
                &HudColors::default(),
                HudUnits::Metric,
            );
            assert_eq!(text, "N/A", "field {field:?} must read N/A prelaunch");
        }
    }

    #[test]
    fn warning_status_line_joins_compact_labels() {
        let telemetry = RocketTelemetry {
            g_load: 7.5,
            ..default()
        };
        let (warnings, color) = FieldFormatters::format_field(
            HudField::Warnings,
            &telemetry,
            &RocketCameraMode::default(),
            false,
            &RocketEventFeed::default(),
            1.0,
            0.0,
            &HudColors::default(),
            HudUnits::Metric,
        );
        assert!(warnings.contains("HIGH G-LOAD"));
        assert_eq!(color, HudColors::default().danger);
    }

    #[test]
    fn imperial_units_change_display_suffixes() {
        let telemetry = RocketTelemetry {
            altitude_agl_m: 1_000.0,
            velocity_total_mps: 100.0,
            mass_kg: 10_000.0,
            ..default()
        };
        let format = |field, units| {
            FieldFormatters::format_field(
                field,
                &telemetry,
                &RocketCameraMode::default(),
                false,
                &RocketEventFeed::default(),
                1.0,
                0.0,
                &HudColors::default(),
                units,
            )
            .0
        };
        assert!(format(HudField::AltitudeAgl, HudUnits::Metric).ends_with(" m"));
        assert!(format(HudField::AltitudeAgl, HudUnits::Imperial).ends_with(" ft"));
        assert!(format(HudField::VelocityTotal, HudUnits::Metric).ends_with(" m/s"));
        assert!(format(HudField::VelocityTotal, HudUnits::Imperial).ends_with(" mph"));
        assert!(format(HudField::Mass, HudUnits::Metric).ends_with(" kg"));
        assert!(format(HudField::Mass, HudUnits::Imperial).ends_with(" lb"));
    }

    #[test]
    fn high_contrast_palette_replaces_red_green_pairing() {
        let standard = HudColors::for_palette(HudPalette::Standard);
        let contrast = HudColors::for_palette(HudPalette::HighContrast);
        assert_ne!(standard.success, contrast.success);
        assert_ne!(standard.danger, contrast.danger);
        assert_eq!(contrast.bright, Color::WHITE);
    }
}
