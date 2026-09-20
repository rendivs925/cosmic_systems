// Rocket HUD UI - encapsulated, type-driven design.

use super::components::*;
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
    EventLog,
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
const PRIMARY_PANEL_WIDTH_PX: f32 = 168.0;
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

    pub fn with_colors(mut self, colors: HudColors) -> Self {
        self.colors = colors;
        self
    }

    fn txt(&self, text: impl Into<String>, style: TextStyle) -> (Text, TextFont, TextColor) {
        (
            Text::new(text),
            TextFont {
                font_size: style.font_size,
                ..default()
            },
            TextColor(style.color),
        )
    }

    fn label(&self, text: &str) -> (Text, TextFont, TextColor) {
        self.txt(text, TextStyle::new(LABEL_FONT_PX, self.colors.dim))
    }

    fn section_header(&self, text: &str) -> (Text, TextFont, TextColor) {
        self.txt(text, TextStyle::new(9.0, self.colors.dim))
    }

    fn title(&self, text: &str) -> (Text, TextFont, TextColor) {
        self.txt(text, TextStyle::new(13.0, self.colors.bright))
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

/// Presentation-only HUD options (never part of simulation state).
#[derive(Resource, Debug, Clone, Copy)]
pub struct HudOptions {
    /// Show diagnostic rows (orbital elements, attitude, thermal, recovery).
    pub detail: bool,
}

impl Default for HudOptions {
    fn default() -> Self {
        Self { detail: false }
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

            status_row(p, builder, HudPanel::Left, HudField::EventLog, false);
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
    fn format_field(
        field: HudField,
        telemetry: &RocketTelemetry,
        camera_mode: &RocketCameraMode,
        flash_on: bool,
        event_feed: &RocketEventFeed,
        time_acceleration: f64,
        pending_simulation_s: f64,
    ) -> (String, Color) {
        let colors = HudColors::default();
        let white = Color::WHITE;
        match field {
            HudField::AltitudeAgl => (format!("{:.0} m", telemetry.altitude_agl_m), white),
            HudField::AltitudeMsl => (format!("{:.0} m", telemetry.altitude_msl_m), white),
            HudField::RadarAltitude => (format!("{:.1} m", telemetry.radar_altitude_m), white),
            HudField::VelocityTotal => (format!("{:.0} m/s", telemetry.velocity_total_mps), white),
            HudField::VelocityVertical => {
                let color = if telemetry.velocity_vertical_mps >= 0.0 {
                    colors.success
                } else {
                    colors.danger
                };
                (
                    format!("{:+.1} m/s", telemetry.velocity_vertical_mps),
                    color,
                )
            }
            HudField::VelocityHorizontal => (
                format!("{:.0} m/s", telemetry.velocity_horizontal_mps),
                white,
            ),
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
                    format!("{:.0} km", telemetry.apoapsis_altitude_m / 1000.0)
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
                        format!("{:.0} km", telemetry.periapsis_altitude_m / 1000.0)
                    } else {
                        "N/A".to_string()
                    },
                    color,
                )
            }
            HudField::SemiMajorAxis => (
                if telemetry.orbital_semi_major_axis_m.is_finite() {
                    format!("{:.0} km", telemetry.orbital_semi_major_axis_m / 1000.0)
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
            HudField::DeltaV => (format!("{:.0} m/s", telemetry.delta_v_remaining_mps), white),
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
                (
                    format!(
                        "{:.1}%  S{}  {:.1} t",
                        fraction * 100.0,
                        telemetry.active_stage + 1,
                        telemetry.total_propellant_kg / 1000.0,
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
            HudField::Mass => (format!("{:.0} kg", telemetry.mass_kg), white),
            HudField::Thrust => (
                format!("{:.1} kN", telemetry.total_thrust_n / 1000.0),
                white,
            ),
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
            HudField::EventLog => {
                if event_feed.latest.is_empty() {
                    (String::new(), white)
                } else {
                    (format!(">> {}", event_feed.latest), colors.warning)
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
    for (marker, mut text, mut text_color) in hud_query.iter_mut() {
        let (formatted, color) = FieldFormatters::format_field(
            marker.field,
            &telemetry,
            &camera_mode,
            flash_on,
            &event_feed,
            sim_time.time_acceleration,
            sim_time.pending_simulation_s(),
        );
        if text.0 != formatted {
            text.0 = formatted;
        }
        if text_color.0 != color {
            text_color.0 = color;
        }
    }
}

/// H toggles the diagnostic detail view; Z hides the whole HUD (Zen mode).
pub fn toggle_hud_options_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut options: ResMut<HudOptions>,
    mut zen_mode: ResMut<ZenMode>,
) {
    if keyboard.just_pressed(KeyCode::KeyH) {
        options.detail = !options.detail;
    }
    if keyboard.just_pressed(KeyCode::KeyZ) {
        zen_mode.enabled = !zen_mode.enabled;
    }
}

/// Apply presentation visibility: Zen mode hides everything; detail option
/// shows or hides diagnostic rows.
pub fn apply_hud_visibility_system(
    options: Res<HudOptions>,
    zen_mode: Res<ZenMode>,
    mut roots: Query<(&HudRoot, &mut Node), Without<HudDetailRow>>,
    mut detail_rows: Query<&mut Node, With<HudDetailRow>>,
) {
    let visible = UiDisplay::Flex;
    let hidden = UiDisplay::None;

    for (_root, mut node) in &mut roots {
        node.display = if zen_mode.enabled { hidden } else { visible };
    }
    let show_detail = options.detail && !zen_mode.enabled;
    for mut node in &mut detail_rows {
        node.display = if show_detail { visible } else { hidden };
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
        );
        assert!(warnings.contains("HIGH G-LOAD"));
        assert_eq!(color, HudColors::default().danger);
    }
}
