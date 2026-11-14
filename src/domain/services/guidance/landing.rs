//! Powered and terminal landing guidance.

use super::attitude::attitude_from_direction;
use super::entry::DescentGuidanceConfig;
use crate::domain::math::{DQuat, DVec3};

/// Powered descent guidance using lossless convexification (simplified).
/// Computes thrust vector and attitude to land at target with minimum fuel.
#[expect(
    clippy::too_many_arguments,
    reason = "The guidance API exposes physically distinct state and configuration inputs."
)]
pub fn powered_descent_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
    max_thrust_angle_rad: f64,
    _dt: f64,
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

/// Default landing point directly below a vehicle on a spherical body's mean
/// surface. `position_m * altitude / radius` incorrectly targets a point near
/// the center instead of the surface.
pub fn default_surface_landing_target(position_m: DVec3, surface_radius_m: f64) -> DVec3 {
    position_m.normalize_or_zero() * surface_radius_m.max(0.0)
}

/// Suicide burn / hover-slam terminal guidance.
/// Computes the thrust vector and ignition time to land with zero terminal velocity.
/// Uses the "constant deceleration" approximation: a = v² / (2h) for vertical, plus gravity.
pub fn suicide_burn_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
    gravity_accel_mps2: f64,
) -> (DVec3, DQuat, f64, bool) {
    let up_dir = position_m.normalize_or_zero();
    let altitude = (position_m - target_position_m).length();
    let vertical_vel = velocity_mps.dot(up_dir);
    let horizontal_vel_vec = velocity_mps - up_dir * vertical_vel;
    let horizontal_speed = horizontal_vel_vec.length();

    // Time to cancel vertical velocity at max thrust (with gravity).
    let max_accel = max_thrust_n / mass_kg;
    let net_decel = max_accel - gravity_accel_mps2;

    // Suicide burn altitude: h = v² / (2a) for constant deceleration.
    let suicide_altitude = if net_decel > 0.0 && vertical_vel < 0.0 {
        vertical_vel * vertical_vel / (2.0 * net_decel)
    } else {
        0.0
    };

    // Horizontal stopping distance (assume we can thrust horizontally at max_accel).
    let horizontal_stop_dist = if horizontal_speed > 0.0 {
        horizontal_speed * horizontal_speed / (2.0 * max_accel)
    } else {
        0.0
    };

    // Total altitude needed for suicide burn.
    let total_suicide_altitude = suicide_altitude + horizontal_stop_dist + 10.0; // 10m margin

    // Should we start the burn now?
    let should_burn = altitude <= total_suicide_altitude && vertical_vel < -0.5;

    // Compute required acceleration to reach target with zero velocity.
    let t_go = if vertical_vel < -0.1 {
        altitude / (-vertical_vel).max(0.1)
    } else {
        5.0
    }
    .max(1.0);

    let r_tgo = position_m + velocity_mps * t_go;
    let accel_req = (target_position_m - r_tgo) * 2.0 / (t_go * t_go);

    // Gravity compensation.
    let accel_cmd = accel_req + up_dir * gravity_accel_mps2;

    // Thrust direction and magnitude.
    let thrust_mag = (accel_cmd.length() * mass_kg).min(max_thrust_n);
    let thrust_dir = if accel_cmd.length() > 1e-6 {
        accel_cmd.normalize()
    } else {
        up_dir
    };

    // Attitude aligns body +Y with thrust direction.
    let attitude = DQuat::from_rotation_arc(DVec3::Y, thrust_dir);

    (
        thrust_dir * thrust_mag,
        attitude,
        total_suicide_altitude,
        should_burn,
    )
}

/// Hover-slam guidance: maintain a constant descent rate while nulling horizontal velocity.
/// Used for final approach when suicide burn has arrested most velocity.
pub fn hover_slam_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
    gravity_accel_mps2: f64,
    target_descent_rate_mps: f64,
) -> (DVec3, DQuat) {
    let up_dir = position_m.normalize_or_zero();
    let _altitude = (position_m - target_position_m).length();
    let vertical_vel = velocity_mps.dot(up_dir);
    let horizontal_vel_vec = velocity_mps - up_dir * vertical_vel;

    // Vertical control: maintain target descent rate.
    let vertical_error = vertical_vel - target_descent_rate_mps;
    let vertical_accel_cmd = -vertical_error * 2.0; // PD control

    // Horizontal control: null horizontal velocity.
    let horizontal_accel_cmd = -horizontal_vel_vec * 1.0; // Proportional control

    // Combined acceleration command + gravity compensation.
    let accel_cmd = up_dir * (vertical_accel_cmd + gravity_accel_mps2) + horizontal_accel_cmd;

    let thrust_mag = (accel_cmd.length() * mass_kg).min(max_thrust_n);
    let thrust_dir = if accel_cmd.length() > 1e-6 {
        accel_cmd.normalize()
    } else {
        up_dir
    };

    let attitude = DQuat::from_rotation_arc(DVec3::Y, thrust_dir);
    (thrust_dir * thrust_mag, attitude)
}

/// Speed below which the retrograde divert gives way to the vertical terminal
/// law, m/s.
pub const TERMINAL_DIVERT_SPEED_MPS: f64 = 25.0;

/// Ignition margin on the high-speed stopping distance. The retro-burn law
/// assumes instantaneous full thrust; the modelled throttle slew and engine
/// start are not instantaneous, so the vehicle must commit before the
/// idealized stopping point. This is a control margin, not extra thrust.
pub const TERMINAL_IGNITION_MARGIN_RATIO: f64 = 0.25;

/// Terminal descent-rate gain: `rate = -(altitude * gain)`, 1/s. A booster does
/// not hover from 100 m; it descends at a controlled rate and flares in the
/// final metres.
pub const TERMINAL_DESCENT_RATE_GAIN_PER_S: f64 = 0.08;

/// Minimum controlled descent rate before the final flare, m/s.
pub const TERMINAL_MIN_DESCENT_RATE_MPS: f64 = 10.0;

/// Position gain on the horizontal terminal error, 1/s².
pub const TERMINAL_HORIZONTAL_POSITION_GAIN_PER_S2: f64 = 0.12;

/// Velocity damping gain on the horizontal terminal rate, 1/s.
pub const TERMINAL_HORIZONTAL_VELOCITY_GAIN_PER_S: f64 = 0.9;

/// One-way landing sequence, retained with the vehicle's guidance state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LandingBurnSequence {
    #[default]
    Coast,
    Retrograde,
    Terminal,
}

/// Terrain-relative terminal guidance for a reusable vertical landing. The
/// controller combines a stopping-distance brake with velocity damping and
/// limits lateral acceleration to a strict thrust-vector tilt envelope. It
/// never commands a downward-pointing main engine near the ground.
///
/// Above [`TERMINAL_DIVERT_SPEED_MPS`] the law is a three-dimensional retrograde
/// suicide burn: it coasts while the target is farther than the distance a
/// maximum-thrust retro burn can null, then points the engine opposite the
/// velocity vector. That is the only phase that can remove a large horizontal
/// component without tilting the main engine far from the deck normal; it
/// requires a target on the vehicle's natural ground track (a downrange deck
/// placed at the ballistic impact point), not a pad the vehicle has already
/// overflown. Below the divert speed the existing tilt-limited terminal law
/// takes over for the final settle.
#[expect(
    clippy::too_many_arguments,
    reason = "Physical inputs and persistent landing sequence."
)]
pub fn terminal_landing_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    radar_altitude_m: f64,
    mass_kg: f64,
    max_thrust_n: f64,
    gravity_accel_mps2: f64,
    sequence: &mut LandingBurnSequence,
) -> (DVec3, DQuat) {
    const MAX_TILT_RAD: f64 = 12.0_f64.to_radians();
    const LANDING_MARGIN_M: f64 = 15.0;
    let up_dir = position_m.normalize_or_zero();
    let mass_kg = mass_kg.max(1.0);
    let max_accel_mps2 = (max_thrust_n / mass_kg).max(0.0);
    if up_dir.length_squared() <= 1e-12 || max_accel_mps2 <= gravity_accel_mps2 {
        return (DVec3::ZERO, attitude_from_direction(up_dir));
    }

    let altitude_m = radar_altitude_m.max(0.0);
    let speed_mps = velocity_mps.length();

    // High-speed divert. The available deceleration along the velocity is the
    // thrust minus the gravity component that accelerates the vehicle along
    // that same direction. Coast until a maximum-thrust retro burn can just
    // null the velocity before reaching the target, then burn retrograde.
    if speed_mps <= TERMINAL_DIVERT_SPEED_MPS {
        *sequence = LandingBurnSequence::Terminal;
    }
    if *sequence != LandingBurnSequence::Terminal {
        let velocity_unit = velocity_mps / speed_mps;
        let gravity_along_velocity_mps2 = (-up_dir * gravity_accel_mps2).dot(velocity_unit);
        let net_decel_mps2 = max_accel_mps2 - gravity_along_velocity_mps2;
        let stopping_distance_m = if net_decel_mps2 > 0.0 {
            speed_mps.powi(2) / (2.0 * net_decel_mps2)
        } else {
            f64::INFINITY
        };
        let distance_to_target_m = (target_position_m - position_m).length();
        let thrust_direction = -velocity_unit;
        let ignition_distance_m =
            stopping_distance_m * (1.0 + TERMINAL_IGNITION_MARGIN_RATIO) + LANDING_MARGIN_M;
        if distance_to_target_m <= ignition_distance_m {
            *sequence = LandingBurnSequence::Retrograde;
        }
        if *sequence == LandingBurnSequence::Retrograde {
            return (
                thrust_direction * max_thrust_n,
                attitude_from_direction(thrust_direction),
            );
        }
        return (DVec3::ZERO, attitude_from_direction(thrust_direction));
    }

    let vertical_speed_mps = velocity_mps.dot(up_dir);
    let horizontal_velocity_mps = velocity_mps - up_dir * vertical_speed_mps;
    let target_offset_m = target_position_m - position_m;
    let horizontal_error_m = target_offset_m - up_dir * target_offset_m.dot(up_dir);
    let max_net_upward_accel_mps2 = max_accel_mps2 - gravity_accel_mps2;
    let stopping_distance_m = if vertical_speed_mps < 0.0 {
        vertical_speed_mps.powi(2) / (2.0 * max_net_upward_accel_mps2)
    } else {
        0.0
    };
    let braking = altitude_m <= stopping_distance_m + LANDING_MARGIN_M;
    let target_descent_rate_mps = if braking {
        -1.5
    } else {
        -(altitude_m * TERMINAL_DESCENT_RATE_GAIN_PER_S).clamp(TERMINAL_MIN_DESCENT_RATE_MPS, 45.0)
    };
    let vertical_error_mps = target_descent_rate_mps - vertical_speed_mps;
    let requested_net_upward_accel_mps2 = if braking {
        (vertical_error_mps * 1.5)
            .max(stopping_distance_m / altitude_m.max(1.0) * 0.5)
            .clamp(0.0, max_net_upward_accel_mps2)
    } else {
        (vertical_error_mps * 0.8).clamp(-gravity_accel_mps2 * 0.75, max_net_upward_accel_mps2)
    };
    // Keep a positive ground-normal thrust component. Reducing throttle is the
    // safe response to excess upward velocity, never turning the engine down.
    let vertical_thrust_accel_mps2 = (gravity_accel_mps2 + requested_net_upward_accel_mps2)
        .clamp(gravity_accel_mps2 * 0.25, max_accel_mps2);
    let requested_horizontal_accel_mps2 = horizontal_error_m
        * TERMINAL_HORIZONTAL_POSITION_GAIN_PER_S2
        - horizontal_velocity_mps * TERMINAL_HORIZONTAL_VELOCITY_GAIN_PER_S;
    let max_horizontal_accel_mps2 = vertical_thrust_accel_mps2 * MAX_TILT_RAD.tan();
    let horizontal_accel_mps2 =
        requested_horizontal_accel_mps2.clamp_length_max(max_horizontal_accel_mps2);
    let thrust_accel_mps2 = up_dir * vertical_thrust_accel_mps2 + horizontal_accel_mps2;
    let thrust_magnitude_n = (thrust_accel_mps2.length() * mass_kg).min(max_thrust_n);
    let thrust_direction = thrust_accel_mps2.normalize_or_zero();

    (
        thrust_direction * thrust_magnitude_n,
        attitude_from_direction(thrust_direction),
    )
}

/// Enhanced powered descent guidance using lossless convexification.
/// Solves minimum-fuel landing problem with thrust and pointing constraints.
#[expect(
    clippy::too_many_arguments,
    reason = "The guidance API exposes physically distinct state and configuration inputs."
)]
pub fn powered_descent_guidance_convex(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
    min_thrust_n: f64,
    max_thrust_angle_rad: f64,
    gravity_accel_mps2: f64,
    time_to_go_s: f64,
) -> (DVec3, DQuat) {
    let up_dir = position_m.normalize_or_zero();

    // State relative to target.
    let r_rel = position_m - target_position_m;
    let v_rel = velocity_mps;

    // Time-to-go estimate.
    let t_go = time_to_go_s.max(1.0);

    // Lossless convexification: solve for minimum-fuel thrust profile.
    // The optimal thrust is bang-bang or singular. For landing, we use a simplified
    // analytical solution: constant acceleration to reach target with zero velocity.

    // Required acceleration (constant) to reach target with zero velocity in t_go.
    // r_tgo = r + v*t + 0.5*a*t² = 0  =>  a = -2(r + v*t) / t²
    let accel_req = -(r_rel + v_rel * t_go) * 2.0 / (t_go * t_go);

    // Add gravity compensation.
    let accel_cmd = accel_req + up_dir * gravity_accel_mps2;

    // Thrust magnitude (clamped to engine limits).
    let thrust_mag = (accel_cmd.length() * mass_kg).clamp(min_thrust_n, max_thrust_n);

    // Thrust direction with pointing constraint.
    let thrust_dir = if accel_cmd.length() > 1e-6 {
        accel_cmd.normalize()
    } else {
        up_dir
    };

    // Enforce maximum thrust angle from vertical.
    let angle_from_vertical = thrust_dir.angle_between(up_dir);
    let thrust_dir = if angle_from_vertical > max_thrust_angle_rad {
        let axis = up_dir.cross(thrust_dir).normalize_or_zero();
        DQuat::from_axis_angle(axis, max_thrust_angle_rad - angle_from_vertical) * thrust_dir
    } else {
        thrust_dir
    };

    let attitude = DQuat::from_rotation_arc(DVec3::Y, thrust_dir);
    (thrust_dir * thrust_mag, attitude)
}

#[cfg(test)]
mod sequence_tests {
    use super::*;

    #[test]
    fn landing_burn_latch_holds_a_commit_and_hands_off_when_slow() {
        let target = DVec3::X * 6_371_000.0;
        let mut sequence = LandingBurnSequence::default();
        for (altitude, speed, expected) in [
            (5_000.0, 200.0, LandingBurnSequence::Coast),
            (200.0, 200.0, LandingBurnSequence::Retrograde),
            (200.0, 100.0, LandingBurnSequence::Retrograde),
            (100.0, 20.0, LandingBurnSequence::Terminal),
            (65.0, 26.0, LandingBurnSequence::Terminal),
        ] {
            let (thrust, _) = terminal_landing_guidance(
                target + DVec3::X * altitude,
                -DVec3::X * speed,
                target,
                altitude,
                30_000.0,
                2_500_000.0,
                9.81,
                &mut sequence,
            );
            assert_eq!(sequence, expected);
            if expected == LandingBurnSequence::Coast {
                assert_eq!(thrust, DVec3::ZERO);
            } else {
                assert!(thrust.x > 0.0, "committed descent must not return to coast");
            }
        }
    }

    #[test]
    fn terminal_speed_excursion_retains_tilt_envelope_and_thrust() {
        let target = DVec3::X * 6_371_000.0;
        let mut sequence = LandingBurnSequence::Terminal;
        let (thrust, _) = terminal_landing_guidance(
            target + DVec3::X * 65.0,
            -DVec3::X * 10.0 + DVec3::Z * 30.0,
            target,
            65.0,
            30_000.0,
            845_000.0,
            9.81,
            &mut sequence,
        );
        assert_eq!(sequence, LandingBurnSequence::Terminal);
        assert!(thrust.x > 0.0);
        assert!(thrust.z < 0.0);
        assert!(thrust.angle_between(DVec3::X) <= 12.0_f64.to_radians() + 1e-12);
    }
}
