//! Telemetry-to-text formatting for the HUD fields, plus derived warnings.
//!
//! Pure formatting: it reads a telemetry snapshot and produces display strings
//! and colors. Layout owns the static labels and node structure.

use super::{HudColors, HudField};
use crate::infrastructure::bevy_adapters::rocket::components::{
    RocketCameraMode, RocketMissionState, RocketTelemetry,
};
use crate::infrastructure::bevy_adapters::rocket::hud_units::HudUnits;
use crate::infrastructure::bevy_adapters::rocket::telemetry::RocketEventFeed;
use bevy::prelude::Color;

/// Formatter implementations for each field. Returns the value only; the
/// static label is owned by the layout so alignment is independent of text
/// metrics.
pub(crate) struct FieldFormatters;

impl FieldFormatters {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn format_field(
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

    pub(crate) fn compute_warnings(telemetry: &RocketTelemetry) -> Vec<&'static str> {
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
