// Rocket HUD UI - encapsulated, type-driven design.

use super::components::*;
use super::telemetry::RocketEventFeed;
use crate::domain::services::simulation_time::SimulationTime;
use bevy::camera::CameraOutputMode;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;

/// HUD panel types for different display regions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudPanel {
    Left,  // Full telemetry panel
    Right, // Compact speed tape
}

/// HUD field identifier for type-safe updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HudField {
    // Altitude group
    AltitudeAgl,
    AltitudeMsl,
    RadarAltitude,
    // Velocity group
    VelocityTotal,
    VelocityVertical,
    VelocityHorizontal,
    MachNumber,
    // Aero group
    DynamicPressure,
    GLoad,
    AngleOfAttack,
    BankAngle,
    // Orbital group
    Apoapsis,
    Periapsis,
    TwRatio,
    DeltaV,
    // Vehicle group
    Stage,
    MissionPhase,
    Mass,
    Thrust,
    PropellantFraction,
    // Attitude group
    AngularRates,
    // Control group
    Throttle,
    Gimbal,
    // Thermal group
    HeatFlux,
    Ablation,
    PlasmaBlackout,
    // Recovery group
    Parachute,
    SurfaceType,
    TouchdownScorecard,
    // Meta
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
            bright: Color::srgb(0.8, 0.9, 1.0),
            dim: Color::srgb(0.5, 0.6, 0.7),
            warning: Color::srgb(1.0, 0.8, 0.2),
            caution: Color::srgb(1.0, 0.5, 0.2),
            success: Color::srgb(0.3, 1.0, 0.3),
            danger: Color::srgb(1.0, 0.2, 0.2),
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

    fn section_header(&self, text: &str) -> (Text, TextFont, TextColor) {
        self.txt(text, TextStyle::new(10.0, self.colors.dim))
    }

    fn title(&self, text: &str) -> (Text, TextFont, TextColor) {
        self.txt(text, TextStyle::new(12.0, self.colors.bright))
    }
}

/// Marker component for HUD entities.
#[derive(Component, Debug)]
pub struct RocketHudMarker {
    pub panel: HudPanel,
    pub field: HudField,
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

    spawn_left_panel(&mut commands, &builder);
    spawn_right_panel(&mut commands, &builder);
}

fn spawn_left_panel(commands: &mut Commands, builder: &HudBuilder) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                padding: UiRect::all(Val::Px(8.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.7)),
            BorderColor::all(Color::srgba(0.2, 0.3, 0.5, 0.4)),
            BorderRadius::all(Val::Px(6.0)),
            RocketHudMarker {
                panel: HudPanel::Left,
                field: HudField::Warnings,
            },
        ))
        .with_children(|p| {
            p.spawn(builder.title("=== ROCKET FLIGHT ==="));

            // Altitude group
            p.spawn(builder.section_header("--- ALTITUDE ---"));
            spawn_field(p, builder, HudField::AltitudeAgl, "AGL: --- m");
            spawn_field(p, builder, HudField::AltitudeMsl, "MSL: --- m");
            spawn_field(p, builder, HudField::RadarAltitude, "Radar: --- m");

            // Velocity group
            p.spawn(builder.section_header("--- VELOCITY ---"));
            spawn_field(p, builder, HudField::VelocityTotal, "Total: --- m/s");
            spawn_field(p, builder, HudField::VelocityVertical, "Vertical: --- m/s");
            spawn_field(
                p,
                builder,
                HudField::VelocityHorizontal,
                "Horizontal: --- m/s",
            );
            spawn_field(p, builder, HudField::MachNumber, "Mach: ---");

            // Aero group
            p.spawn(builder.section_header("--- AERO ---"));
            spawn_field(p, builder, HudField::DynamicPressure, "Q: --- Pa");
            spawn_field(p, builder, HudField::GLoad, "G-Load: ---");
            spawn_field(p, builder, HudField::AngleOfAttack, "AoA: --- deg");
            spawn_field(p, builder, HudField::BankAngle, "Bank: --- deg");

            // Orbital group
            p.spawn(builder.section_header("--- ORBIT ---"));
            spawn_field(p, builder, HudField::Apoapsis, "Apoapsis: --- km");
            spawn_field(p, builder, HudField::Periapsis, "Periapsis: --- km");
            spawn_field(p, builder, HudField::TwRatio, "T/W: ---");
            spawn_field(p, builder, HudField::DeltaV, "dV: --- m/s");

            // Vehicle group
            p.spawn(builder.section_header("--- VEHICLE ---"));
            spawn_field(p, builder, HudField::Stage, "Stage: ---");
            spawn_field(p, builder, HudField::MissionPhase, "Phase: ---");
            spawn_field(p, builder, HudField::Mass, "Mass: --- kg");
            spawn_field(p, builder, HudField::Thrust, "Thrust: --- kN");
            spawn_field(p, builder, HudField::PropellantFraction, "Fuel: ---%");

            // Attitude group
            p.spawn(builder.section_header("--- ATTITUDE ---"));
            spawn_field(
                p,
                builder,
                HudField::AngularRates,
                "Rates: R:--- P:--- Y:--- deg/s",
            );

            // Control group
            p.spawn(builder.section_header("--- CONTROL ---"));
            spawn_field(p, builder, HudField::Throttle, "Throttle: ---%");
            spawn_field(p, builder, HudField::Gimbal, "Gimbal: P:--- Y:--- deg");

            // Thermal group
            p.spawn(builder.section_header("--- THERMAL ---"));
            spawn_field(p, builder, HudField::HeatFlux, "Heat: --- MW/m2");
            spawn_field(p, builder, HudField::Ablation, "Nose R: --- m  TPS: --- m");
            spawn_field(p, builder, HudField::PlasmaBlackout, "Blackout: NO");

            // Recovery group
            p.spawn(builder.section_header("--- RECOVERY ---"));
            spawn_field(p, builder, HudField::Parachute, "Drogue: NO  Main: NO");
            spawn_field(p, builder, HudField::SurfaceType, "Surface: ---");
            spawn_field(p, builder, HudField::TouchdownScorecard, "TD: ---");

            // Time & camera
            p.spawn(builder.section_header("--- ---"));
            spawn_field(p, builder, HudField::TimeAndCamera, "T+: --- s  CAM: ---");

            // Event feed (latest staging/fairing/splashdown/blackout event)
            spawn_field(p, builder, HudField::EventLog, "");

            // Warnings
            spawn_field(p, builder, HudField::Warnings, "");
        });
}

fn spawn_right_panel(commands: &mut Commands, builder: &HudBuilder) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                right: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(4.0),
                padding: UiRect::all(Val::Px(10.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.05, 0.7)),
            BorderColor::all(Color::srgba(0.2, 0.3, 0.5, 0.4)),
            BorderRadius::all(Val::Px(6.0)),
            RocketHudMarker {
                panel: HudPanel::Right,
                field: HudField::Warnings,
            },
        ))
        .with_children(|p| {
            p.spawn(builder.txt("ALTITUDE", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::AltitudeAgl,
                "--- m",
                TextStyle::new(20.0, builder.colors.bright),
            );

            p.spawn(builder.txt("VERT SPEED", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::VelocityVertical,
                "--- m/s",
                TextStyle::new(16.0, builder.colors.bright),
            );

            p.spawn(builder.txt("SPEED", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::VelocityTotal,
                "--- m/s",
                TextStyle::new(16.0, builder.colors.bright),
            );

            p.spawn(builder.txt("MACH", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::MachNumber,
                "---",
                TextStyle::new(16.0, builder.colors.bright),
            );

            p.spawn(builder.txt("Q (kPa)", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::DynamicPressure,
                "---",
                TextStyle::new(16.0, builder.colors.bright),
            );

            p.spawn(builder.txt("G-LOAD", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::GLoad,
                "---",
                TextStyle::new(16.0, builder.colors.bright),
            );

            p.spawn(builder.txt("THROTTLE", TextStyle::new(9.0, builder.colors.dim)));
            spawn_field_with_style(
                p,
                builder,
                HudField::Throttle,
                "---%",
                TextStyle::new(16.0, builder.colors.bright),
            );
        });
}

fn spawn_field(
    parent: &mut ChildSpawnerCommands,
    builder: &HudBuilder,
    field: HudField,
    initial: impl Into<String>,
) {
    spawn_field_with_style(
        parent,
        builder,
        field,
        initial,
        TextStyle::new(10.0, builder.colors.bright),
    )
}

fn spawn_field_with_style(
    parent: &mut ChildSpawnerCommands,
    builder: &HudBuilder,
    field: HudField,
    initial: impl Into<String>,
    style: TextStyle,
) {
    parent
        .spawn(builder.txt(initial, style))
        .insert(RocketHudMarker {
            panel: HudPanel::Left,
            field,
        });
}

/// Formatter implementations for each field.
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
        match field {
            HudField::AltitudeAgl => (
                format!("AGL: {:.0} m", telemetry.altitude_agl_m),
                Color::WHITE,
            ),
            HudField::AltitudeMsl => (
                format!("MSL: {:.0} m", telemetry.altitude_msl_m),
                Color::WHITE,
            ),
            HudField::RadarAltitude => (
                format!("Radar: {:.1} m", telemetry.radar_altitude_m),
                Color::WHITE,
            ),
            HudField::VelocityTotal => (
                format!("Total: {:.0} m/s", telemetry.velocity_total_mps),
                Color::WHITE,
            ),
            HudField::VelocityVertical => {
                let color = if telemetry.velocity_vertical_mps > 0.0 {
                    HudColors::default().success
                } else {
                    HudColors::default().danger
                };
                (
                    format!("Vertical: {:.1} m/s", telemetry.velocity_vertical_mps),
                    color,
                )
            }
            HudField::VelocityHorizontal => (
                format!("Horizontal: {:.0} m/s", telemetry.velocity_horizontal_mps),
                Color::WHITE,
            ),
            HudField::MachNumber => (format!("Mach: {:.2}", telemetry.mach_number), Color::WHITE),
            HudField::DynamicPressure => (
                format!(
                    "Q: {:.0} Pa ({:.1} kPa)",
                    telemetry.dynamic_pressure_pa,
                    telemetry.dynamic_pressure_pa / 1000.0
                ),
                Color::WHITE,
            ),
            HudField::GLoad => {
                let color = if telemetry.g_load > 6.0 {
                    HudColors::default().danger
                } else if telemetry.g_load > 3.0 {
                    HudColors::default().warning
                } else {
                    Color::WHITE
                };
                (format!("G-Load: {:.2}", telemetry.g_load), color)
            }
            HudField::Apoapsis => (
                if telemetry.apoapsis_altitude_m.is_finite() {
                    format!("Apoapsis: {:.0} km", telemetry.apoapsis_altitude_m / 1000.0)
                } else {
                    "Apoapsis: N/A".to_string()
                },
                Color::WHITE,
            ),
            HudField::Periapsis => {
                let color = if telemetry.periapsis_altitude_m.is_finite()
                    && telemetry.periapsis_altitude_m < 100_000.0
                    && telemetry.mission_phase != RocketMissionState::Orbit
                {
                    HudColors::default().danger
                } else {
                    Color::WHITE
                };
                (
                    if telemetry.periapsis_altitude_m.is_finite() {
                        format!(
                            "Periapsis: {:.0} km",
                            telemetry.periapsis_altitude_m / 1000.0
                        )
                    } else {
                        "Periapsis: N/A".to_string()
                    },
                    color,
                )
            }
            HudField::TwRatio => (format!("T/W: {:.2}", telemetry.tw_ratio), Color::WHITE),
            HudField::DeltaV => (
                format!("dV: {:.0} m/s", telemetry.delta_v_remaining_mps),
                Color::WHITE,
            ),
            HudField::PropellantFraction => {
                let color = if telemetry.propellant_fraction < 0.1 {
                    HudColors::default().danger
                } else if telemetry.propellant_fraction < 0.3 {
                    HudColors::default().warning
                } else {
                    Color::WHITE
                };
                (
                    format!("Fuel: {:.0}%", telemetry.propellant_fraction * 100.0),
                    color,
                )
            }
            HudField::Stage => (
                format!("Stage: {}", telemetry.active_stage + 1),
                Color::WHITE,
            ),
            HudField::MissionPhase => {
                let (text, color) = match telemetry.mission_phase {
                    RocketMissionState::Crashed => {
                        (String::from("Crashed"), HudColors::default().danger)
                    }
                    RocketMissionState::Landed => {
                        (String::from("Landed"), HudColors::default().success)
                    }
                    RocketMissionState::ReentryCorridor => {
                        (String::from("Reentry"), HudColors::default().caution)
                    }
                    RocketMissionState::PoweredDescent | RocketMissionState::Landing => {
                        (String::from("Descent"), HudColors::default().success)
                    }
                    _ => (format!("{:?}", telemetry.mission_phase), Color::WHITE),
                };
                (format!("Phase: {}", text), color)
            }
            HudField::Mass => (format!("Mass: {:.0} kg", telemetry.mass_kg), Color::WHITE),
            HudField::Thrust => (
                format!("Thrust: {:.1} kN", telemetry.total_thrust_n / 1000.0),
                Color::WHITE,
            ),
            HudField::AngularRates => (
                format!(
                    "Rates: R:{:.1} P:{:.1} Y:{:.1} deg/s",
                    telemetry.roll_rate_dps, telemetry.pitch_rate_dps, telemetry.yaw_rate_dps
                ),
                Color::WHITE,
            ),
            HudField::Throttle => (
                format!("Throttle: {:.0}%", telemetry.throttle * 100.0),
                Color::WHITE,
            ),
            HudField::Gimbal => (
                format!(
                    "Gimbal: P:{:.1} Y:{:.1} deg",
                    telemetry.gimbal_pitch_deg, telemetry.gimbal_yaw_deg
                ),
                Color::WHITE,
            ),
            HudField::HeatFlux => {
                let total_mw = telemetry.total_heat_flux_w_m2 / 1_000_000.0;
                let color = if total_mw > 10.0 {
                    HudColors::default().danger
                } else if total_mw > 1.0 {
                    HudColors::default().warning
                } else {
                    Color::WHITE
                };
                (format!("Heat: {:.2} MW/m2", total_mw), color)
            }
            HudField::Ablation => (
                format!(
                    "Nose R: {:.3} m  TPS: {:.3} m",
                    telemetry.nose_radius_m, telemetry.tps_thickness_remaining_m
                ),
                Color::WHITE,
            ),
            HudField::PlasmaBlackout => {
                if telemetry.plasma_blackout {
                    // Flash between alarm red and dim while the link is down
                    // (driven by the event-backed CommsState, presentation
                    // only).
                    let color = if flash_on {
                        HudColors::default().danger
                    } else {
                        HudColors::default().dim
                    };
                    ("Blackout: YES".to_string(), color)
                } else {
                    ("Blackout: NO".to_string(), Color::WHITE)
                }
            }
            HudField::Parachute => {
                let drogue = if telemetry.drogue_deployed {
                    "YES"
                } else {
                    "NO"
                };
                let main = if telemetry.main_deployed { "YES" } else { "NO" };
                (format!("Drogue: {}  Main: {}", drogue, main), Color::WHITE)
            }
            HudField::SurfaceType => {
                // Water is inferred from terrain at mean sea level on Earth.
                if telemetry.over_water {
                    ("Surface: WATER".to_string(), HudColors::default().caution)
                } else {
                    ("Surface: LAND".to_string(), Color::WHITE)
                }
            }
            HudField::TouchdownScorecard => {
                if !telemetry.touchdown_recorded {
                    ("TD: ---".to_string(), Color::WHITE)
                } else {
                    let color = if telemetry.toppling {
                        HudColors::default().danger
                    } else if telemetry.touchdown_tilt_deg > 10.0 {
                        HudColors::default().warning
                    } else {
                        HudColors::default().success
                    };
                    (
                        format!(
                            "TD: v{:.1} lat{:.1} tilt{:.0}° slope{:.0}° tgt{:.0}m strut{:.1}m",
                            telemetry.touchdown_vertical_speed_mps,
                            telemetry.touchdown_lateral_speed_mps,
                            telemetry.touchdown_tilt_deg,
                            telemetry.touchdown_slope_deg,
                            telemetry.touchdown_distance_to_target_m,
                            telemetry.leg_compression_peak_m,
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
                (
                    format!(
                        "T+: {:.1} s  CAM: {}  WARP ×{}  QUEUE {:.1}s",
                        telemetry.time_since_liftoff_s,
                        cam_name,
                        time_acceleration,
                        pending_simulation_s,
                    ),
                    Color::WHITE,
                )
            }
            HudField::Warnings => {
                let warnings = Self::compute_warnings(telemetry);
                if warnings.is_empty() {
                    (String::new(), Color::WHITE)
                } else {
                    (
                        format!("⚠ {}", warnings.join("  ⚠ ")),
                        HudColors::default().danger,
                    )
                }
            }
            HudField::EventLog => {
                // Event-driven display fed by domain messages; empty while no
                // event is recent.
                if event_feed.latest.is_empty() {
                    (String::new(), Color::WHITE)
                } else {
                    (
                        format!("» {}", event_feed.latest),
                        HudColors::default().warning,
                    )
                }
            }
            HudField::AngleOfAttack => (
                format!("AoA: {:.1} deg", telemetry.angle_of_attack_deg),
                Color::WHITE,
            ),
            HudField::BankAngle => (
                format!("Bank: {:.1} deg", telemetry.bank_angle_deg),
                Color::WHITE,
            ),
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
        if telemetry.propellant_fraction < 0.05
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
}
