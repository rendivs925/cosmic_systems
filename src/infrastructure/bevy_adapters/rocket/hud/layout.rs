//! HUD layout: the fluent [`HudBuilder`] and the panel/row spawn helpers.

use super::{
    HudBaseFontSize, HudColors, HudDetailRow, HudField, HudPanel, HudRoot, HudStaticRole,
    HudStaticText, HudTimelineRow, RocketHudMarker, TextStyle, FLIGHT_PANEL_WIDTH_PX,
    LABEL_FONT_PX, PANEL_BG, PANEL_BORDER, PANEL_MARGIN_PX, PRIMARY_PANEL_WIDTH_PX,
    PRIMARY_VALUE_FONT_PX, VALUE_FONT_PX,
};
use bevy::camera::CameraOutputMode;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

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
