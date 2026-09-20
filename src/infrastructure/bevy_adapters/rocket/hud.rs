// Rocket HUD UI - encapsulated, type-driven design.

use super::components::*;
use super::hud_units::HudUnits;
use super::telemetry::RocketEventFeed;
use crate::domain::services::simulation_time::SimulationTime;
use crate::infrastructure::bevy_adapters::ui_components::ZenMode;
use bevy::camera::CameraOutputMode;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy::ui::Display as UiDisplay;

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

/// Builder for HUD UI elements.
#[derive(Default)]
pub struct HudBuilder {
    colors: HudColors,
}

impl HudBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn txt(
        &self,
        text: impl Into<String>,
        style: TextStyle,
    ) -> (Text, TextFont, TextColor, HudBaseFontSize) {
        (
            Text::new(text),
            TextFont {
                font_size: style.font_size,
                ..default()
            },
            TextColor(style.color),
            HudBaseFontSize(style.font_size),
        )
    }

    fn label(&self, text: &str) -> (Text, TextFont, TextColor, HudBaseFontSize, HudStaticText) {
        let (text, font, color, base) =
            self.txt(text, TextStyle::new(LABEL_FONT_PX, self.colors.dim));
        (text, font, color, base, HudStaticText(HudStaticRole::Label))
    }

    fn section_header(
        &self,
        text: &str,
    ) -> (Text, TextFont, TextColor, HudBaseFontSize, HudStaticText) {
        let (text, font, color, base) = self.txt(text, TextStyle::new(9.0, self.colors.dim));
        (
            text,
            font,
            color,
            base,
            HudStaticText(HudStaticRole::Header),
        )
    }

    fn title(&self, text: &str) -> (Text, TextFont, TextColor, HudBaseFontSize, HudStaticText) {
        let (text, font, color, base) = self.txt(text, TextStyle::new(13.0, self.colors.bright));
        (text, font, color, base, HudStaticText(HudStaticRole::Title))
    }
}

/// Marker component for dynamic HUD value entities.
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
pub fn spawn_rocket_hud(mut commands: Commands) {
    let builder = HudBuilder::new();

    // Main 2D camera for HUD.
    //
    // `output_mode` + clear-color workaround for Bevy 0.17 multi-camera + MSAA:
    // the 3D flight camera uses Msaa::Sample4, and a later 2D camera with
    // `ClearColorConfig::None` alone discards the previous camera's output
    // (bevyengine/bevy#18901, #18903, #23844) -> the whole scene renders black.
    // Writing with ALPHA_BLENDING over a transparent clear preserves the 3D
    // pass underneath.
    commands.spawn((
        Camera2d,
        Camera {
            order: 11,
            clear_color: ClearColorConfig::Custom(Color::NONE),
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            ..default()
        },
    ));

    // Full-screen flex row pins the flight card to the left edge and the
    // primary gauge strip to the right edge. Anchoring through a real layout
    // container is robust across window sizes; a bare absolute `right` node
    // depends on the implicit UI root's resolved width.
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::FlexStart,
            padding: UiRect::all(Val::Px(PANEL_MARGIN_PX)),
            column_gap: Val::Px(12.0),
            ..default()
        })
        .with_children(|root| {
            spawn_flight_panel(root, &builder);
            spawn_primary_panel(root, &builder);
        });
}

fn panel_node(width_px: f32) -> Node {
    Node {
        width: Val::Px(width_px),
        flex_direction: FlexDirection::Column,
        row_gap: Val::Px(3.0),
        padding: UiRect::all(Val::Px(10.0)),
        ..default()
    }
}

fn panel_chrome() -> (BackgroundColor, BorderColor, BorderRadius) {
    (
        BackgroundColor(PANEL_BG),
        BorderColor::all(PANEL_BORDER),
        BorderRadius::all(Val::Px(10.0)),
    )
}

fn row_node() -> Node {
    Node {
        width: Val::Percent(100.0),
        flex_direction: FlexDirection::Row,
        justify_content: JustifyContent::SpaceBetween,
        align_items: AlignItems::Center,
        column_gap: Val::Px(8.0),
        ..default()
    }
}

/// Label + right-aligned value row. Values key off `HudField`; labels are
/// static so alignment never depends on the font's metrics.
fn stat_row(
    parent: &mut ChildSpawnerCommands,
    builder: &HudBuilder,
    panel: HudPanel,
    label: &str,
    field: HudField,
    value_style: TextStyle,
    detail: bool,
) {
    let mut row = parent.spawn(row_node());
    if detail {
        row.insert(HudDetailRow);
    }
    row.with_children(|r| {
        r.spawn(builder.label(label));
        r.spawn((
            builder.txt("---", value_style),
            RocketHudMarker { panel, field },
        ));
    });
}

/// Full-width status value row (event feed / warnings) with no static label.
fn status_row(
    parent: &mut ChildSpawnerCommands,
    builder: &HudBuilder,
    panel: HudPanel,
    field: HudField,
    detail: bool,
) {
    let mut row = parent.spawn(Node {
        width: Val::Percent(100.0),
        ..default()
    });
    if detail {
        row.insert(HudDetailRow);
    }
    row.with_children(|r| {
        r.spawn((
            builder.txt("", TextStyle::new(11.0, builder.colors.bright)),
            RocketHudMarker { panel, field },
        ));
    });
}

/// Multi-line event timeline value (newest first), toggled with `L`.
fn timeline_row(
    parent: &mut ChildSpawnerCommands,
    builder: &HudBuilder,
    panel: HudPanel,
    field: HudField,
) {
    parent
        .spawn(Node {
            width: Val::Percent(100.0),
            ..default()
        })
        .insert(HudTimelineRow)
        .with_children(|r| {
            r.spawn((
                builder.txt("", TextStyle::new(10.0, builder.colors.dim)),
                RocketHudMarker { panel, field },
            ));
        });
}

/// Section header for a section whose rows are all diagnostic.
fn detail_header(parent: &mut ChildSpawnerCommands, builder: &HudBuilder, text: &str) {
    parent.spawn((builder.section_header(text), HudDetailRow));
}

fn normal_style(builder: &HudBuilder) -> TextStyle {
    TextStyle::new(VALUE_FONT_PX, builder.colors.bright)
}

fn primary_style(builder: &HudBuilder) -> TextStyle {
    TextStyle::new(PRIMARY_VALUE_FONT_PX, builder.colors.bright)
}

fn spawn_flight_panel(parent: &mut ChildSpawnerCommands, builder: &HudBuilder) {
    let node = panel_node(FLIGHT_PANEL_WIDTH_PX);
    let (bg, border, radius) = panel_chrome();
    parent
        .spawn((
            node,
            bg,
            border,
            radius,
            HudRoot {
                panel: HudPanel::Left,
            },
        ))
        .with_children(|p| {
            p.spawn(builder.title("FLIGHT"));

            p.spawn(builder.section_header("VEHICLE"));
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "STAGE",
                HudField::Stage,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "PHASE",
                HudField::MissionPhase,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "MASS",
                HudField::Mass,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "FUEL",
                HudField::PropellantFraction,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "DELTA-V",
                HudField::DeltaV,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "T/W",
                HudField::TwRatio,
                normal_style(builder),
                false,
            );

            detail_header(p, builder, "FLIGHT PATH");
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "MSL",
                HudField::AltitudeMsl,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "RADAR",
                HudField::RadarAltitude,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "H-SPD",
                HudField::VelocityHorizontal,
                normal_style(builder),
                true,
            );

            p.spawn(builder.section_header("ORBIT"));
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "APOAPSIS",
                HudField::Apoapsis,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "PERIAPSIS",
                HudField::Periapsis,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "SEMI-MAJOR",
                HudField::SemiMajorAxis,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "ECCENTRICITY",
                HudField::Eccentricity,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "INCLINATION",
                HudField::Inclination,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "RAAN",
                HudField::RaanDeg,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "ARG PERIAPSIS",
                HudField::ArgPeriapsis,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "TRUE ANOMALY",
                HudField::TrueAnomaly,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "PERIOD",
                HudField::OrbitalPeriod,
                normal_style(builder),
                true,
            );

            p.spawn(builder.section_header("ATTITUDE"));
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "AoA",
                HudField::AngleOfAttack,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "BANK",
                HudField::BankAngle,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "RATES",
                HudField::AngularRates,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "GIMBAL",
                HudField::Gimbal,
                normal_style(builder),
                true,
            );

            p.spawn(builder.section_header("THERMAL"));
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "HEAT",
                HudField::HeatFlux,
                normal_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "ABLATION",
                HudField::Ablation,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "BLACKOUT",
                HudField::PlasmaBlackout,
                normal_style(builder),
                false,
            );

            detail_header(p, builder, "RECOVERY");
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "CHUTES",
                HudField::Parachute,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "SURFACE",
                HudField::SurfaceType,
                normal_style(builder),
                true,
            );
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "TOUCHDOWN",
                HudField::TouchdownScorecard,
                normal_style(builder),
                true,
            );

            p.spawn(builder.section_header("MISSION"));
            stat_row(
                p,
                builder,
                HudPanel::Left,
                "TIME",
                HudField::TimeAndCamera,
                normal_style(builder),
                false,
            );

            p.spawn((builder.section_header("EVENTS"), HudTimelineRow));
            timeline_row(p, builder, HudPanel::Left, HudField::EventTimeline);
            status_row(p, builder, HudPanel::Left, HudField::Warnings, false);
        });
}

fn spawn_primary_panel(parent: &mut ChildSpawnerCommands, builder: &HudBuilder) {
    let node = panel_node(PRIMARY_PANEL_WIDTH_PX);
    let (bg, border, radius) = panel_chrome();
    parent
        .spawn((
            node,
            bg,
            border,
            radius,
            HudRoot {
                panel: HudPanel::Right,
            },
        ))
        .with_children(|p| {
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "ALT",
                HudField::AltitudeAgl,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "V/S",
                HudField::VelocityVertical,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "SPD",
                HudField::VelocityTotal,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "MACH",
                HudField::MachNumber,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "Q",
                HudField::DynamicPressure,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "G",
                HudField::GLoad,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "THR",
                HudField::Throttle,
                primary_style(builder),
                false,
            );
            stat_row(
                p,
                builder,
                HudPanel::Right,
                "THRUST",
                HudField::Thrust,
                normal_style(builder),
                true,
            );
        });
}

/// Formatter implementations for each field. Returns the value only; the
/// static label is owned by the layout so alignment is independent of text
/// metrics.
struct FieldFormatters;

impl FieldFormatters {
    #[allow(clippy::too_many_arguments)]
    fn format_field(
        field: HudField,
        telemetry: &RocketTelemetry,
        camera_mode: &RocketCameraMode,
        flash_on: bool,
        event_feed: &RocketEventFeed,
        time_acceleration: f64,
        pending_simulation_s: f64,
        colors: &HudColors,
        units: HudUnits,
    ) -> (String, Color) {
        let colors = *colors;
        let white = Color::WHITE;
        match field {
            HudField::AltitudeAgl => {
                let (value, unit) = units.altitude(telemetry.altitude_agl_m);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::AltitudeMsl => {
                let (value, unit) = units.altitude(telemetry.altitude_msl_m);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::RadarAltitude => {
                let (value, unit) = units.altitude(telemetry.radar_altitude_m);
                (format!("{value:.1} {unit}"), white)
            }
            HudField::VelocityTotal => {
                let (value, unit) = units.speed(telemetry.velocity_total_mps);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::VelocityVertical => {
                let color = if telemetry.velocity_vertical_mps >= 0.0 {
                    colors.success
                } else {
                    colors.danger
                };
                let (value, unit) = units.speed(telemetry.velocity_vertical_mps);
                (format!("{value:+.1} {unit}"), color)
            }
            HudField::VelocityHorizontal => {
                let (value, unit) = units.speed(telemetry.velocity_horizontal_mps);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::MachNumber => (format!("{:.2}", telemetry.mach_number), white),
            HudField::DynamicPressure => {
                let color = if telemetry.dynamic_pressure_pa > 50_000.0 {
                    colors.danger
                } else {
                    white
                };
                (
                    format!("{:.1} kPa", telemetry.dynamic_pressure_pa / 1000.0),
                    color,
                )
            }
            HudField::GLoad => {
                let color = if telemetry.g_load > 6.0 {
                    colors.danger
                } else if telemetry.g_load > 3.0 {
                    colors.warning
                } else {
                    white
                };
                (format!("{:.2} g", telemetry.g_load), color)
            }
            HudField::AngleOfAttack => {
                (format!("{:+.1} deg", telemetry.angle_of_attack_deg), white)
            }
            HudField::BankAngle => (format!("{:+.1} deg", telemetry.bank_angle_deg), white),
            HudField::Apoapsis => (
                if telemetry.apoapsis_altitude_m.is_finite() {
                    let (value, unit) = units.distance_km(telemetry.apoapsis_altitude_m);
                    format!("{value:.0} {unit}")
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::Periapsis => {
                let color = if telemetry.periapsis_altitude_m.is_finite()
                    && telemetry.periapsis_altitude_m < 100_000.0
                    && telemetry.mission_phase != RocketMissionState::Orbit
                {
                    colors.danger
                } else {
                    white
                };
                (
                    if telemetry.periapsis_altitude_m.is_finite() {
                        let (value, unit) = units.distance_km(telemetry.periapsis_altitude_m);
                        format!("{value:.0} {unit}")
                    } else {
                        "N/A".to_string()
                    },
                    color,
                )
            }
            HudField::SemiMajorAxis => (
                if telemetry.orbital_semi_major_axis_m.is_finite() {
                    let (value, unit) = units.distance_km(telemetry.orbital_semi_major_axis_m);
                    format!("{value:.0} {unit}")
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::Eccentricity => (
                if telemetry.orbital_eccentricity.is_finite() {
                    format!("{:.4}", telemetry.orbital_eccentricity)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::Inclination => (
                if telemetry.orbital_inclination_deg.is_finite() {
                    format!("{:.2} deg", telemetry.orbital_inclination_deg)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::RaanDeg => (
                if telemetry.orbital_raan_deg.is_finite() {
                    format!("{:.1} deg", telemetry.orbital_raan_deg)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::ArgPeriapsis => (
                if telemetry.orbital_arg_periapsis_deg.is_finite() {
                    format!("{:.1} deg", telemetry.orbital_arg_periapsis_deg)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::TrueAnomaly => (
                if telemetry.orbital_true_anomaly_deg.is_finite() {
                    format!("{:.1} deg", telemetry.orbital_true_anomaly_deg)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::OrbitalPeriod => (
                if telemetry.orbital_period_s.is_finite() {
                    format!("{:.1} min", telemetry.orbital_period_s / 60.0)
                } else {
                    "N/A".to_string()
                },
                white,
            ),
            HudField::TwRatio => (format!("{:.2}", telemetry.tw_ratio), white),
            HudField::DeltaV => {
                let (value, unit) = units.speed(telemetry.delta_v_remaining_mps);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::PropellantFraction => {
                // The active stage is the tank draining right now, so its
                // fraction is the precise gauge reading; the whole-vehicle
                // fraction barely moves while only one stage burns.
                let fraction = telemetry.active_stage_propellant_fraction;
                let color = if fraction < 0.1 {
                    colors.danger
                } else if fraction < 0.3 {
                    colors.warning
                } else {
                    white
                };
                let (mass_value, mass_unit) = match units {
                    HudUnits::Metric => (telemetry.total_propellant_kg / 1000.0, "t"),
                    HudUnits::Imperial => units.mass(telemetry.total_propellant_kg),
                };
                (
                    format!(
                        "{:.1}%  S{}  {:.1} {}",
                        fraction * 100.0,
                        telemetry.active_stage + 1,
                        mass_value,
                        mass_unit,
                    ),
                    color,
                )
            }
            HudField::Stage => (format!("{}", telemetry.active_stage + 1), white),
            HudField::MissionPhase => match telemetry.mission_phase {
                RocketMissionState::Crashed => ("CRASHED".to_string(), colors.danger),
                RocketMissionState::Landed => ("LANDED".to_string(), colors.success),
                RocketMissionState::ReentryCorridor => ("REENTRY".to_string(), colors.caution),
                RocketMissionState::PoweredDescent | RocketMissionState::Landing => {
                    ("DESCENT".to_string(), colors.success)
                }
                RocketMissionState::PreLaunch => ("PRELAUNCH".to_string(), colors.dim),
                // `RocketMissionState` wraps the domain enum; print the inner
                // value rather than the wrapper's derived Debug.
                other => (format!("{:?}", other.0).to_uppercase(), white),
            },
            HudField::Mass => {
                let (value, unit) = units.mass(telemetry.mass_kg);
                (format!("{value:.0} {unit}"), white)
            }
            HudField::Thrust => {
                let (value, unit) = units.thrust_kn(telemetry.total_thrust_n / 1000.0);
                (format!("{value:.1} {unit}"), white)
            }
            HudField::AngularRates => (
                format!(
                    "{:.1} / {:.1} / {:.1} deg/s",
                    telemetry.roll_rate_dps, telemetry.pitch_rate_dps, telemetry.yaw_rate_dps
                ),
                white,
            ),
            HudField::Throttle => (format!("{:.0}%", telemetry.throttle * 100.0), white),
            HudField::Gimbal => (
                format!(
                    "{:+.2} / {:+.2} deg",
                    telemetry.gimbal_pitch_deg, telemetry.gimbal_yaw_deg
                ),
                white,
            ),
            HudField::HeatFlux => {
                let total_mw = telemetry.total_heat_flux_w_m2 / 1_000_000.0;
                let color = if total_mw > 10.0 {
                    colors.danger
                } else if total_mw > 1.0 {
                    colors.warning
                } else {
                    white
                };
                (format!("{:.2} MW/m2", total_mw), color)
            }
            HudField::Ablation => (
                format!(
                    "{:.3} / {:.3} m",
                    telemetry.nose_radius_m, telemetry.tps_thickness_remaining_m
                ),
                white,
            ),
            HudField::PlasmaBlackout => {
                if telemetry.plasma_blackout {
                    // Flash between alarm red and dim while the link is down.
                    let color = if flash_on { colors.danger } else { colors.dim };
                    ("YES".to_string(), color)
                } else {
                    ("NO".to_string(), white)
                }
            }
            HudField::Parachute => {
                let drogue = if telemetry.drogue_deployed { "Y" } else { "-" };
                let main = if telemetry.main_deployed { "Y" } else { "-" };
                (format!("D {drogue}  M {main}"), white)
            }
            HudField::SurfaceType => {
                if telemetry.over_water {
                    ("WATER".to_string(), colors.caution)
                } else {
                    ("LAND".to_string(), white)
                }
            }
            HudField::TouchdownScorecard => {
                if !telemetry.touchdown_recorded {
                    ("---".to_string(), white)
                } else {
                    let color = if telemetry.toppling {
                        colors.danger
                    } else if telemetry.touchdown_tilt_deg > 10.0 {
                        colors.warning
                    } else {
                        colors.success
                    };
                    (
                        format!(
                            "v{:.1}  t{:.0} deg  d{:.0}m",
                            telemetry.touchdown_vertical_speed_mps,
                            telemetry.touchdown_tilt_deg,
                            telemetry.touchdown_distance_to_target_m,
                        ),
                        color,
                    )
                }
            }
            HudField::TimeAndCamera => {
                let cam_name = match *camera_mode {
                    RocketCameraMode::Chase => "CHASE",
                    RocketCameraMode::Cockpit => "COCKPIT",
                    RocketCameraMode::Orbital => "ORBITAL",
                    RocketCameraMode::Surface => "SURFACE",
                    RocketCameraMode::Free => "FREE",
                };
                let queue = if pending_simulation_s > 0.5 {
                    format!("  Q{pending_simulation_s:.0}s")
                } else {
                    String::new()
                };
                (
                    format!(
                        "{:.1} s  {}  x{}{}",
                        telemetry.time_since_liftoff_s, cam_name, time_acceleration, queue
                    ),
                    white,
                )
            }
            HudField::Warnings => {
                let warnings = Self::compute_warnings(telemetry);
                if warnings.is_empty() {
                    (String::new(), white)
                } else {
                    (format!("! {}", warnings.join("  |  ")), colors.danger)
                }
            }
            HudField::EventTimeline => {
                if event_feed.history.is_empty() {
                    ("(no events yet)".to_string(), colors.dim)
                } else {
                    let text = event_feed
                        .history
                        .iter()
                        .map(|line| format!("t+{:.1}  {}", line.sim_time_s, line.label))
                        .collect::<Vec<_>>()
                        .join("\n");
                    (text, colors.bright)
                }
            }
        }
    }

    fn compute_warnings(telemetry: &RocketTelemetry) -> Vec<&'static str> {
        let mut warnings = Vec::new();
        if telemetry.plasma_blackout {
            warnings.push("COMMS BLACKOUT");
        }
        if telemetry.mission_phase == RocketMissionState::Landed && telemetry.over_water {
            warnings.push("SPLASHDOWN");
        }
        if telemetry.g_load > 6.0 {
            warnings.push("HIGH G-LOAD");
        }
        if telemetry.dynamic_pressure_pa > 50_000.0 {
            warnings.push("HIGH Q");
        }
        if telemetry.total_heat_flux_w_m2 > 10_000_000.0 {
            warnings.push("EXTREME HEATING");
        }
        if telemetry.periapsis_altitude_m < 100_000.0
            && telemetry.mission_phase != RocketMissionState::Orbit
        {
            warnings.push("LOW PERIAPSIS");
        }
        if telemetry.active_stage_propellant_fraction < 0.05
            && telemetry.mission_phase != RocketMissionState::Orbit
            && telemetry.mission_phase != RocketMissionState::Landed
        {
            warnings.push("LOW FUEL");
        }
        if telemetry.radar_altitude_m < 100.0 && telemetry.velocity_vertical_mps < -10.0 {
            warnings.push("TERRAIN PROXIMITY");
        }
        warnings
    }
}

/// Flash rate of the blackout banner while the link is down (Hz).
const BLACKOUT_FLASH_HZ: f32 = 2.0;
const HUD_UPDATE_INTERVAL_S: f32 = 1.0 / 30.0;

#[derive(Default)]
pub(crate) struct HudUpdateState {
    initialized: bool,
    last_update_real_time_s: f32,
}

/// System to update all HUD fields from telemetry.
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
