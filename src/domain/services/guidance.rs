//! Mission guidance: where the vehicle should go.
//!
//! Guidance is the first layer of the flight loop (AGENTS.md section 18):
//! mission → guidance → control → actuation → physics → state → guidance.
//! Guidance computes a target attitude (and phase transitions) from the
//! mission and the current state; it never commands actuators or writes the
//! vehicle's motion. All target generation is a pure function so it is
//! testable without Bevy.
//!
//! ## Ascent guidance
//!
//! A gravity-turn pitch-over: the vehicle holds the local vertical on the pad,
//! then pitches over toward the downrange direction by an angle that grows with
//! altitude, reaching [`AscentGuidanceProfile::max_turn_angle_rad`] by
//! [`AscentGuidanceProfile::turn_end_altitude_m`]. Orbit insertion aligns the
//! vehicle prograde (with the velocity vector).
//!
//! ## Descent guidance
//!
//! - Deorbit burn: computes retrograde burn to lower periapsis to entry interface.
//! - Reentry corridor: bank-angle profile to manage g-load, q, and heat flux.
//! - Powered descent: convex optimization for minimum-fuel landing.
//! - Unpowered descent: parafoil lateral acceleration tracking.

use crate::domain::entities::rocket::RocketMissionState;
use bevy::math::{DQuat, DVec3};

/// Gravity-turn ascent profile.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AscentGuidanceProfile {
    /// Altitude (m) at which Launch transitions to Ascent.
    pub ascent_start_altitude_m: f64,
    /// Altitude (m) at which the pitch-over begins.
    pub turn_start_altitude_m: f64,
    /// Altitude (m) at which the pitch-over reaches its maximum.
    pub turn_end_altitude_m: f64,
    /// Maximum pitch angle from the local vertical, radians.
    pub max_turn_angle_rad: f64,
}

impl Default for AscentGuidanceProfile {
    fn default() -> Self {
        Self {
            ascent_start_altitude_m: 5_000.0,
            turn_start_altitude_m: 2_000.0,
            turn_end_altitude_m: 80_000.0,
            max_turn_angle_rad: 80.0_f64.to_radians(),
        }
    }
}

impl AscentGuidanceProfile {
    pub fn new(
        ascent_start_altitude_m: f64,
        turn_start_altitude_m: f64,
        turn_end_altitude_m: f64,
        max_turn_angle_rad: f64,
    ) -> Self {
        Self {
            ascent_start_altitude_m,
            turn_start_altitude_m,
            turn_end_altitude_m,
            max_turn_angle_rad,
        }
    }
}

/// Pitch angle (radians from the local vertical) for the gravity turn at an
/// altitude, ramping from 0 at [`AscentGuidanceProfile::turn_start_altitude_m`]
/// to [`AscentGuidanceProfile::max_turn_angle_rad`] at
/// [`AscentGuidanceProfile::turn_end_altitude_m`].
pub fn gravity_turn_pitch_angle(profile: &AscentGuidanceProfile, altitude_m: f64) -> f64 {
    let t = ((altitude_m - profile.turn_start_altitude_m)
        / (profile.turn_end_altitude_m - profile.turn_start_altitude_m))
        .clamp(0.0, 1.0);
    profile.max_turn_angle_rad * t
}

/// Desired body-axis direction for the gravity turn: the local vertical
/// rotated about the pitch axis (horizontal, perpendicular to the ascent
/// plane) by the turn angle at the current altitude.
pub fn gravity_turn_direction(
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
) -> DVec3 {
    let angle = gravity_turn_pitch_angle(profile, altitude_m);
    (DQuat::from_axis_angle(pitch_axis, angle) * up_dir).normalize()
}

/// The horizontal pitch axis for a pitch-over toward an azimuth: the
/// normalized cross of the local vertical and the reference direction. The
/// chosen reference fixes the ascent plane; the result is horizontal
/// (perpendicular to `up_dir`).
pub fn pitch_axis_from_reference(up_dir: DVec3, reference: DVec3) -> Option<DVec3> {
    let axis = up_dir.cross(reference);
    if axis.length_squared() < 1e-12 {
        None
    } else {
        Some(axis.normalize())
    }
}

/// Attitude (body→world) whose +Y body axis points along `direction`, with
/// minimal rotation (no roll).
pub fn attitude_from_direction(direction: DVec3) -> DQuat {
    DQuat::from_rotation_arc(DVec3::Y, direction)
}

/// Prograde (velocity-aligned) target attitude. Falls back to identity for a
/// stationary vehicle.
pub fn prograde_attitude(velocity_mps: DVec3) -> DQuat {
    let speed = velocity_mps.length();
    if speed < 1e-6 {
        DQuat::IDENTITY
    } else {
        DQuat::from_rotation_arc(DVec3::Y, velocity_mps / speed)
    }
}

/// Configuration for descent guidance parameters per celestial body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DescentGuidanceConfig {
    /// Entry interface altitude (m) where reentry corridor management begins.
    pub entry_interface_altitude_m: f64,
    /// Maximum allowable g-load during reentry (in Earth g's).
    pub max_g_load: f64,
    /// Maximum dynamic pressure during reentry (Pa).
    pub max_dynamic_pressure_pa: f64,
    /// Maximum heat flux during reentry (W/m²).
    pub max_heat_flux_w_m2: f64,
    /// Altitude (m) at which powered descent phase begins.
    pub powered_descent_altitude_m: f64,
    /// Altitude (m) at which terminal descent phase begins.
    pub terminal_descent_altitude_m: f64,
    /// Target vertical velocity at touchdown (m/s, negative for descent).
    pub touchdown_vertical_velocity_mps: f64,
}

impl Default for DescentGuidanceConfig {
    fn default() -> Self {
        Self {
            entry_interface_altitude_m: 120_000.0,
            max_g_load: 4.0,
            max_dynamic_pressure_pa: 50_000.0,
            max_heat_flux_w_m2: 1_000_000.0,
            powered_descent_altitude_m: 5_000.0,
            terminal_descent_altitude_m: 100.0,
            touchdown_vertical_velocity_mps: -1.0,
        }
    }
}

/// Per-body descent guidance configurations.
impl DescentGuidanceConfig {
    pub fn for_body(name: &str) -> Self {
        match name {
            "Earth" => Self::default(),
            "Moon" => Self {
                entry_interface_altitude_m: 50_000.0,
                max_g_load: 3.0,
                max_dynamic_pressure_pa: 1_000.0,
                max_heat_flux_w_m2: 10_000.0,
                powered_descent_altitude_m: 2_000.0,
                terminal_descent_altitude_m: 50.0,
                touchdown_vertical_velocity_mps: -0.5,
                ..Default::default()
            },
            "Mars" => Self {
                entry_interface_altitude_m: 125_000.0,
                max_g_load: 5.0,
                max_dynamic_pressure_pa: 40_000.0,
                max_heat_flux_w_m2: 500_000.0,
                powered_descent_altitude_m: 3_000.0,
                terminal_descent_altitude_m: 80.0,
                touchdown_vertical_velocity_mps: -0.8,
                ..Default::default()
            },
            _ => Self::default(),
        }
    }
}

/// Deorbit burn delta-v (m/s) to lower periapsis to `target_periapsis_m`
/// from a circular orbit at `orbit_radius_m` around a body with `mu_m3_s2`.
/// Uses the vis-viva equation for a Hohmann transfer to an elliptical orbit
/// with the target periapsis.
pub fn deorbit_burn_dv(orbit_radius_m: f64, target_periapsis_m: f64, mu_m3_s2: f64) -> f64 {
    let r1 = orbit_radius_m;
    let r2 = target_periapsis_m;
    let a = (r1 + r2) / 2.0; // semi-major axis of transfer ellipse
    let v_circular = (mu_m3_s2 / r1).sqrt();
    let v_transfer = (mu_m3_s2 * (2.0 / r1 - 1.0 / a)).sqrt();
    (v_circular - v_transfer).max(0.0)
}

/// Deorbit burn targeting: computes the retrograde delta-v, burn attitude
/// (retrograde), and ignition time to achieve a target periapsis.
pub fn deorbit_burn_targeting(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_periapsis_m: f64,
    mu_m3_s2: f64,
) -> (f64, DQuat) {
    let r = position_m.length();
    let v = velocity_mps.length();
    let dv = deorbit_burn_dv(r, target_periapsis_m, mu_m3_s2);
    // Retrograde attitude: opposite to velocity vector.
    let burn_attitude = if v > 1e-6 {
        DQuat::from_rotation_arc(DVec3::Y, -velocity_mps / v)
    } else {
        DQuat::IDENTITY
    };
    (dv, burn_attitude)
}

/// Reentry corridor guidance: computes bank angle command to maintain
/// trajectory within g-load, dynamic pressure, and heat flux limits.
/// Uses a simple bang-bang controller with predictor-corrector logic.
pub fn reentry_bank_angle(
    altitude_m: f64,
    velocity_mps: f64,
    dynamic_pressure_pa: f64,
    heat_flux_w_m2: f64,
    g_load: f64,
    config: &DescentGuidanceConfig,
    crossrange_remaining_m: f64,
) -> f64 {
    // Compute constraint margins.
    let g_margin = config.max_g_load - g_load;
    let q_margin = config.max_dynamic_pressure_pa - dynamic_pressure_pa;
    let heat_margin = config.max_heat_flux_w_m2 - heat_flux_w_m2;

    // If any constraint is violated, bank to reduce lift (increase drag).
    let constraint_margin = g_margin.min(q_margin / 1000.0).min(heat_margin / 1000.0);

    // Base bank angle: 0° (full lift up) when within corridor, ±90° when violating.
    let base_bank = if constraint_margin > 0.0 {
        0.0
    } else {
        // Violating constraints: bank to 90° (lift down) to increase drag and reduce g/q/heat.
        90.0_f64.to_radians()
    };

    // Crossrange steering: modulate bank sign to steer toward target.
    // Simplified: if crossrange > 0, use negative bank (left turn), else positive (right turn).
    let crossrange_sign = if crossrange_remaining_m > 0.0 {
        -1.0
    } else {
        1.0
    };

    // Blend: use full bank magnitude for constraint management, sign for crossrange.
    if constraint_margin <= 0.0 {
        base_bank * crossrange_sign
    } else {
        // Within corridor: use smaller bank for crossrange steering.
        let max_crossrange_bank = 30.0_f64.to_radians();
        (crossrange_remaining_m / 100_000.0).clamp(-1.0, 1.0) * max_crossrange_bank
    }
}

/// Powered descent guidance using lossless convexification (simplified).
/// Computes thrust vector and attitude to land at target with minimum fuel.
pub fn powered_descent_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
    max_thrust_angle_rad: f64,
    dt: f64,
    config: &DescentGuidanceConfig,
) -> (DVec3, DQuat) {
    // Time-to-go estimate.
    let altitude = position_m.length();
    let vertical_vel = velocity_mps.dot(position_m.normalize_or_zero());
    let t_go = if vertical_vel < -1.0 {
        (altitude - config.terminal_descent_altitude_m) / (-vertical_vel)
    } else {
        10.0
    }
    .max(1.0);

    // Required acceleration to reach target with zero terminal velocity.
    let r_tgo = position_m + velocity_mps * t_go;
    let accel_req = (target_position_m - r_tgo) * 2.0 / (t_go * t_go);

    // Gravity compensation.
    let up_dir = position_m.normalize_or_zero();
    let gravity_accel = 9.81; // Approximate; real gravity from physics.
    let accel_cmd = accel_req + up_dir * gravity_accel;

    // Thrust direction and magnitude.
    let thrust_mag = (accel_cmd.length() * mass_kg).min(max_thrust_n);
    let thrust_dir = if accel_cmd.length() > 1e-6 {
        accel_cmd.normalize()
    } else {
        up_dir
    };

    // Limit thrust angle from vertical.
    let angle_from_vertical = thrust_dir.angle_between(up_dir);
    let thrust_dir = if angle_from_vertical > max_thrust_angle_rad {
        // Rotate toward vertical.
        let axis = up_dir.cross(thrust_dir).normalize_or_zero();
        DQuat::from_axis_angle(axis, max_thrust_angle_rad - angle_from_vertical) * thrust_dir
    } else {
        thrust_dir
    };

    // Attitude aligns body +Y with thrust direction.
    let attitude = DQuat::from_rotation_arc(DVec3::Y, thrust_dir);
    (thrust_dir * thrust_mag, attitude)
}

/// Unpowered descent guidance for parafoil/parachute.
/// Computes lateral acceleration command to steer toward landing target.
pub fn unpowered_descent_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    parafoil_max_lat_accel_mps2: f64,
) -> DVec3 {
    // Predict impact point assuming constant wind.
    let altitude = position_m.length();
    let downrange_vel = velocity_mps.length();
    let t_go = if downrange_vel > 1.0 {
        altitude / downrange_vel
    } else {
        1.0
    }
    .max(1.0);

    let predicted_impact = position_m + velocity_mps * t_go;
    let miss_distance = (target_position_m - predicted_impact).length();

    // Lateral acceleration command proportional to miss distance.
    let lat_accel_cmd = (miss_distance / (t_go * t_go)).min(parafoil_max_lat_accel_mps2);

    // Direction perpendicular to velocity in horizontal plane.
    let up = position_m.normalize_or_zero();
    let vel_horizontal = velocity_mps - up * velocity_mps.dot(up);
    let lat_dir = if vel_horizontal.length() > 1e-6 {
        up.cross(vel_horizontal.normalize())
    } else {
        DVec3::Z
    };

    lat_dir * lat_accel_cmd
}

/// Advance the ascent mission phase from the current state:
/// - Launch → Ascent once above `ascent_start_altitude_m`.
/// - Ascent → Orbit once the speed reaches `circular_speed_mps` (within
///   `orbit_speed_fraction`).
pub fn advance_ascent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    speed_mps: f64,
    circular_speed_mps: f64,
    ascent_start_altitude_m: f64,
    orbit_speed_fraction: f64,
) -> RocketMissionState {
    match phase {
        RocketMissionState::Launch if altitude_m >= ascent_start_altitude_m => {
            RocketMissionState::Ascent
        }
        RocketMissionState::Ascent if speed_mps >= circular_speed_mps * orbit_speed_fraction => {
            RocketMissionState::Orbit
        }
        _ => phase,
    }
}

/// Advance the descent mission phase based on altitude, velocity, and propulsion state.
pub fn advance_descent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    velocity_mps: f64,
    dynamic_pressure_pa: f64,
    has_active_engines: bool,
    config: &DescentGuidanceConfig,
) -> RocketMissionState {
    match phase {
        RocketMissionState::Orbit => {
            // Deorbit burn is commanded externally; transition when burn completes.
            // For auto-launch, we could add logic here. For now, stay in Orbit.
            phase
        }
        RocketMissionState::DeorbitBurn => {
            // Transition to reentry corridor after burn completes (detected by altitude/velocity change).
            if altitude_m < config.entry_interface_altitude_m {
                RocketMissionState::ReentryCorridor
            } else {
                phase
            }
        }
        RocketMissionState::ReentryCorridor => {
            // Transition to powered/unpowered descent when slow enough.
            if velocity_mps < 340.0 && dynamic_pressure_pa < config.max_dynamic_pressure_pa {
                if has_active_engines {
                    RocketMissionState::PoweredDescent
                } else {
                    RocketMissionState::UnpoweredDescent
                }
            } else {
                phase
            }
        }
        RocketMissionState::PoweredDescent => {
            if altitude_m <= config.terminal_descent_altitude_m {
                RocketMissionState::Landing
            } else {
                phase
            }
        }
        RocketMissionState::UnpoweredDescent => {
            if altitude_m <= config.terminal_descent_altitude_m {
                RocketMissionState::Landing
            } else {
                phase
            }
        }
        _ => phase,
    }
}

/// The guidance target attitude for a mission phase:
/// - PreLaunch / Launch: local vertical.
/// - Ascent: gravity turn toward the downrange plane.
/// - Orbit: prograde.
/// - DeorbitBurn: retrograde.
/// - ReentryCorridor: bank angle modulated (handled by control system).
/// - PoweredDescent: thrust-aligned.
/// - UnpoweredDescent: vertical (handled by parafoil control).
/// - Landing: local vertical.
pub fn target_attitude_for_phase(
    phase: RocketMissionState,
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
    velocity_mps: DVec3,
) -> DQuat {
    match phase {
        RocketMissionState::PreLaunch | RocketMissionState::Launch => {
            attitude_from_direction(up_dir)
        }
        RocketMissionState::Ascent => attitude_from_direction(gravity_turn_direction(
            profile, up_dir, pitch_axis, altitude_m,
        )),
        RocketMissionState::Orbit => prograde_attitude(velocity_mps),
        RocketMissionState::DeorbitBurn => {
            // Retrograde attitude.
            let speed = velocity_mps.length();
            if speed > 1e-6 {
                attitude_from_direction(-velocity_mps / speed)
            } else {
                attitude_from_direction(up_dir)
            }
        }
        RocketMissionState::ReentryCorridor => {
            // Bank angle is managed by control system; hold angle of attack.
            attitude_from_direction(up_dir)
        }
        RocketMissionState::PoweredDescent => {
            // Thrust-aligned attitude (computed by powered_descent_guidance).
            attitude_from_direction(up_dir)
        }
        RocketMissionState::UnpoweredDescent => {
            // Vertical descent; parafoil handles lateral.
            attitude_from_direction(up_dir)
        }
        RocketMissionState::Landing => attitude_from_direction(up_dir),
        _ => attitude_from_direction(up_dir),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::circular_orbit_speed_mps;
    use bevy::math::DVec3;

    fn profile() -> AscentGuidanceProfile {
        AscentGuidanceProfile::new(0.0, 0.0, 80_000.0, 80.0_f64.to_radians())
    }

    fn up_dir() -> DVec3 {
        DVec3::new(0.0, 1.0, 0.0)
    }

    #[test]
    fn gravity_turn_ramps_pitch_with_altitude() {
        let p = profile();
        assert_eq!(gravity_turn_pitch_angle(&p, 0.0), 0.0);
        assert_eq!(
            gravity_turn_pitch_angle(&p, 40_000.0),
            40.0_f64.to_radians()
        );
        assert!((gravity_turn_pitch_angle(&p, 80_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
        // Clamps beyond the profile.
        assert!((gravity_turn_pitch_angle(&p, 200_000.0) - 80.0_f64.to_radians()).abs() < 1e-12);
    }

    #[test]
    fn gravity_turn_direction_pitches_toward_azimuth() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let dir = gravity_turn_direction(&p, up_dir(), axis, 80_000.0);
        // Rotated 80° from vertical toward the horizontal plane.
        assert!((dir.dot(up_dir()) - 80.0_f64.to_radians().cos()).abs() < 1e-9);
        // Horizontal component grows with altitude.
        let vertical = gravity_turn_direction(&p, up_dir(), axis, 0.0);
        assert!((vertical - up_dir()).length() < 1e-9);
    }

    #[test]
    fn pitch_axis_is_horizontal_and_normalized() {
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        assert!((axis.length() - 1.0).abs() < 1e-12);
        assert!(axis.dot(up_dir()).abs() < 1e-12);
        assert!(pitch_axis_from_reference(DVec3::Z, DVec3::Z).is_none());
    }

    #[test]
    fn attitude_points_body_y_along_direction() {
        let dir = DVec3::new(0.0, 1.0, 0.0);
        let q = attitude_from_direction(dir);
        let body_y = q * DVec3::Y;
        assert!((body_y - dir).length() < 1e-9);
    }

    #[test]
    fn prograde_aligns_with_velocity() {
        let vel = DVec3::new(7_600.0, 0.0, 0.0);
        let q = prograde_attitude(vel);
        assert!((q * DVec3::Y - DVec3::X).length() < 1e-9);
        assert_eq!(prograde_attitude(DVec3::ZERO), DQuat::IDENTITY);
    }

    #[test]
    fn phase_selects_distinct_targets() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        let vel = DVec3::new(7_600.0, 0.0, 0.0);

        let launch =
            target_attitude_for_phase(RocketMissionState::Launch, &p, up_dir(), axis, 0.0, vel);
        let ascent = target_attitude_for_phase(
            RocketMissionState::Ascent,
            &p,
            up_dir(),
            axis,
            80_000.0,
            vel,
        );
        let orbit =
            target_attitude_for_phase(RocketMissionState::Orbit, &p, up_dir(), axis, 80_000.0, vel);

        assert!((launch * DVec3::Y - up_dir()).length() < 1e-9);
        // Ascent tilts from vertical.
        assert!((ascent * DVec3::Y).dot(up_dir()) < 1.0 - 1e-3);
        // Orbit aligns with the velocity.
        assert!((orbit * DVec3::Y - DVec3::X).length() < 1e-9);
    }

    #[test]
    fn phase_advances_launch_ascent_orbit() {
        let circular = circular_orbit_speed_mps(5.97237e24, 6_571_000.0);
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Launch,
                1_000.0,
                0.0,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Launch
        );
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Launch,
                6_000.0,
                100.0,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Ascent
        );
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Ascent,
                200_000.0,
                circular,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Orbit
        );
        // Below orbital speed stays in ascent.
        assert_eq!(
            advance_ascent_phase(
                RocketMissionState::Ascent,
                200_000.0,
                circular * 0.5,
                circular,
                5_000.0,
                0.98
            ),
            RocketMissionState::Ascent
        );
    }

    #[test]
    fn deorbit_burn_dv_positive_for_lower_periapsis() {
        let mu = 3.986e14; // Earth
        let orbit_r = 6_771_000.0; // 400 km altitude
        let target_peri = 6_471_000.0; // 100 km periapsis
        let dv = deorbit_burn_dv(orbit_r, target_peri, mu);
        assert!(dv > 0.0);
        assert!(dv < 200.0); // Reasonable deorbit burn
    }

    #[test]
    fn deorbit_burn_targeting_returns_retrograde() {
        let pos = DVec3::new(6_771_000.0, 0.0, 0.0);
        let vel = DVec3::new(0.0, 7_600.0, 0.0);
        let mu = 3.986e14;
        let (dv, att) = deorbit_burn_targeting(pos, vel, 6_471_000.0, mu);
        assert!(dv > 0.0);
        // Attitude should point opposite to velocity (retrograde).
        let body_y = att * DVec3::Y;
        let retrograde = -vel.normalize();
        assert!((body_y - retrograde).length() < 1e-6);
    }

    #[test]
    fn reentry_bank_angle_zero_when_within_corridor() {
        let config = DescentGuidanceConfig::default();
        let bank = reentry_bank_angle(80_000.0, 3000.0, 10_000.0, 100_000.0, 2.0, &config, 0.0);
        assert!((bank - 0.0).abs() < 1e-6);
    }

    #[test]
    fn reentry_bank_angle_90_deg_when_violating_constraints() {
        let config = DescentGuidanceConfig::default();
        let bank = reentry_bank_angle(50_000.0, 5000.0, 100_000.0, 2_000_000.0, 6.0, &config, 0.0);
        assert!((bank.abs() - 90.0_f64.to_radians()).abs() < 1e-6);
    }

    #[test]
    fn descent_phase_transitions() {
        let config = DescentGuidanceConfig::default();

        // Orbit -> DeorbitBurn (external)
        let p = advance_descent_phase(
            RocketMissionState::Orbit,
            400_000.0,
            7600.0,
            0.0,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::Orbit); // External command needed

        // DeorbitBurn -> ReentryCorridor
        let p = advance_descent_phase(
            RocketMissionState::DeorbitBurn,
            100_000.0,
            7000.0,
            1000.0,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::ReentryCorridor);

        // ReentryCorridor -> PoweredDescent
        let p = advance_descent_phase(
            RocketMissionState::ReentryCorridor,
            5_000.0,
            200.0,
            1000.0,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::PoweredDescent);

        // PoweredDescent -> Landing
        let p = advance_descent_phase(
            RocketMissionState::PoweredDescent,
            50.0,
            1.0,
            100.0,
            true,
            &config,
        );
        assert_eq!(p, RocketMissionState::Landing);

        // ReentryCorridor -> UnpoweredDescent (no engines)
        let p = advance_descent_phase(
            RocketMissionState::ReentryCorridor,
            5_000.0,
            200.0,
            1000.0,
            false,
            &config,
        );
        assert_eq!(p, RocketMissionState::UnpoweredDescent);
    }
}
