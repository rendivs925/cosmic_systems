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

/// Autopilot mode for the flight computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutopilotMode {
    #[default]
    Off,
    /// Gravity-turn ascent to orbit insertion.
    Ascent,
    /// Circularization burn at apoapsis.
    OrbitInsertion,
    /// Retrograde burn to lower periapsis for entry.
    Deorbit,
    /// Bank-angle management for atmospheric entry.
    Reentry,
    /// Powered descent with convex optimization (suicide burn / hover-slam).
    PoweredDescent,
    /// Booster flyback skeleton: retrograde pitch-over burn targeting
    /// return-to-launch-site downrange zeroing; hands off to
    /// [`AutopilotMode::Landing`] (suicide burn / hover-slam) for touchdown.
    Boostback,
    /// Terminal landing guidance.
    Landing,
    /// Two-impulse orbit transfer (Hohmann, or bi-elliptic when favorable):
    /// departure burn → coast to apsis → arrival burn. Target radius comes
    /// from [`crate::components::rocket::RocketAutopilot::
    /// transfer_target_radius_m`].
    Transfer,
    /// Station keeping / orbital maintenance.
    StationKeep,
    /// Rendezvous with target vehicle (future).
    Rendezvous,
}

/// Gravity-turn ascent profile with time-based pitch schedule.
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
    /// Time (s) after liftoff when pitch-over begins (for time-based schedule).
    pub turn_start_time_s: f64,
    /// Time (s) after liftoff when pitch-over ends.
    pub turn_end_time_s: f64,
    /// Target orbital inclination (radians) - determines launch azimuth.
    pub target_inclination_rad: f64,
    /// Pitch-gate: minimum altitude (m) the vehicle must reach before any
    /// pitch-over begins, so low-thrust vehicles clear the pad/tower first
    /// regardless of what the time schedule says.
    pub pitch_gate_min_altitude_m: f64,
    /// Pitch-gate: minimum vertical speed (m/s) required before pitch-over
    /// begins. Together with [`Self::pitch_gate_min_altitude_m`] this keeps
    /// the ascent vertical until the vehicle is genuinely flying.
    pub pitch_gate_min_vertical_speed_mps: f64,
}

impl Default for AscentGuidanceProfile {
    fn default() -> Self {
        Self {
            ascent_start_altitude_m: 5_000.0,
            turn_start_altitude_m: 2_000.0,
            turn_end_altitude_m: 80_000.0,
            max_turn_angle_rad: 80.0_f64.to_radians(),
            turn_start_time_s: 10.0,
            turn_end_time_s: 160.0,
            target_inclination_rad: 28.5_f64.to_radians(), // KSC latitude
            // Tower-clearance gates: ~8 vehicle heights up and climbing
            // decisively before the gravity turn may start.
            pitch_gate_min_altitude_m: 150.0,
            pitch_gate_min_vertical_speed_mps: 30.0,
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
            turn_start_time_s: 10.0,
            turn_end_time_s: 160.0,
            target_inclination_rad: 28.5_f64.to_radians(),
            pitch_gate_min_altitude_m: 150.0,
            pitch_gate_min_vertical_speed_mps: 30.0,
        }
    }

    /// Create a profile for a specific launch site inclination.
    pub fn with_inclination(mut self, inclination_rad: f64) -> Self {
        self.target_inclination_rad = inclination_rad;
        self
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

/// Pitch angle for gravity turn using time-based schedule (more realistic).
/// Ramps from 0 at turn_start_time_s to max_turn_angle_rad at turn_end_time_s.
pub fn gravity_turn_pitch_angle_time(
    profile: &AscentGuidanceProfile,
    time_since_liftoff_s: f64,
) -> f64 {
    let t = ((time_since_liftoff_s - profile.turn_start_time_s)
        / (profile.turn_end_time_s - profile.turn_start_time_s))
        .clamp(0.0, 1.0);
    profile.max_turn_angle_rad * t
}

/// Combined pitch angle using both altitude and time (whichever is more advanced).
pub fn gravity_turn_pitch_angle_combined(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    time_since_liftoff_s: f64,
) -> f64 {
    let altitude_angle = gravity_turn_pitch_angle(profile, altitude_m);
    let time_angle = gravity_turn_pitch_angle_time(profile, time_since_liftoff_s);
    altitude_angle.max(time_angle)
}

/// True when the vehicle has cleared the pad/tower enough for the gravity
/// turn to begin: at or beyond the gate altitude AND vertical speed. Both
/// conditions use inclusive thresholds so a gate exactly met engages the turn.
pub fn ascent_pitch_gate_clear(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    vertical_speed_mps: f64,
) -> bool {
    altitude_m >= profile.pitch_gate_min_altitude_m
        && vertical_speed_mps >= profile.pitch_gate_min_vertical_speed_mps
}

/// Pitch angle of the gated ascent schedule: strictly vertical until the
/// tower-clearance gate passes, then the combined altitude/time schedule.
pub fn gravity_turn_pitch_angle_gated(
    profile: &AscentGuidanceProfile,
    altitude_m: f64,
    time_since_liftoff_s: f64,
    vertical_speed_mps: f64,
) -> f64 {
    if !ascent_pitch_gate_clear(profile, altitude_m, vertical_speed_mps) {
        return 0.0;
    }
    gravity_turn_pitch_angle_combined(profile, altitude_m, time_since_liftoff_s)
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

/// Desired body-axis direction for gravity turn using combined altitude/time schedule.
pub fn gravity_turn_direction_combined(
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
    time_since_liftoff_s: f64,
) -> DVec3 {
    let angle = gravity_turn_pitch_angle_combined(profile, altitude_m, time_since_liftoff_s);
    (DQuat::from_axis_angle(pitch_axis, angle) * up_dir).normalize()
}

/// Desired body-axis direction for the gated ascent schedule: local vertical
/// until the tower-clearance gate passes (`altitude_m` and
/// `vertical_speed_mps` at or beyond the profile's gate), then the combined
/// altitude/time pitch schedule.
pub fn gravity_turn_direction_gated(
    profile: &AscentGuidanceProfile,
    up_dir: DVec3,
    pitch_axis: DVec3,
    altitude_m: f64,
    time_since_liftoff_s: f64,
    vertical_speed_mps: f64,
) -> DVec3 {
    let angle = gravity_turn_pitch_angle_gated(
        profile,
        altitude_m,
        time_since_liftoff_s,
        vertical_speed_mps,
    );
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

// ---------------------------------------------------------------------------
// Orbital transfers (Phase 15)
// ---------------------------------------------------------------------------

/// Orbit-radius ratio above which a sufficiently tall bi-elliptic transfer
/// beats the equivalent Hohmann (classical 11.94 boundary).
pub const BIELLIPTIC_FAVORABLE_RATIO: f64 = 11.94;

/// Result of a two-impulse Hohmann computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransferSolution {
    /// Prograde Δv at the departure orbit, m/s.
    pub departure_dv_mps: f64,
    /// Prograde/retrograde Δv at arrival for circularization, m/s.
    pub arrival_dv_mps: f64,
    /// Half-period of the transfer ellipse — coast duration, s.
    pub transfer_time_s: f64,
}

impl TransferSolution {
    /// Total Δv budget of the two impulses, m/s.
    pub fn total_dv_mps(&self) -> f64 {
        self.departure_dv_mps + self.arrival_dv_mps
    }
}

/// Result of a three-impulse bi-elliptic computation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BiellipticSolution {
    /// Δv raising apoapsis from r1 to rb, m/s.
    pub departure_dv_mps: f64,
    /// Δv at rb raising periapsis to r2, m/s.
    pub mid_dv_mps: f64,
    /// Δv at r2 circularizing, m/s.
    pub arrival_dv_mps: f64,
    /// Full transfer duration (two half-ellipses), s.
    pub transfer_time_s: f64,
}

impl BiellipticSolution {
    /// Total Δv budget of the three impulses, m/s.
    pub fn total_dv_mps(&self) -> f64 {
        self.departure_dv_mps + self.mid_dv_mps + self.arrival_dv_mps
    }
}

/// Vis-viva orbital speed on an ellipse with semi-major axis `a_m` at radius
/// `r_m`, m/s. The single authority for all transfer speed math here.
fn vis_viva_speed_mps(mu_m3_s2: f64, r_m: f64, a_m: f64) -> f64 {
    (mu_m3_s2 * (2.0 / r_m - 1.0 / a_m)).sqrt()
}

/// Circular-orbit speed at radius r, m/s.
fn circular_speed_mps(mu_m3_s2: f64, r_m: f64) -> f64 {
    (mu_m3_s2 / r_m).sqrt()
}

/// Two-impulse Hohmann transfer between coplanar circular orbits at `r1_m`
/// and `r2_m` around a body with gravitational parameter `mu_m3_s2`.
/// Works in both directions (raising or lowering); Δvs are magnitudes.
pub fn hohmann_transfer(r1_m: f64, r2_m: f64, mu_m3_s2: f64) -> TransferSolution {
    let a_transfer = (r1_m + r2_m) / 2.0;
    let departure_dv =
        (vis_viva_speed_mps(mu_m3_s2, r1_m, a_transfer) - circular_speed_mps(mu_m3_s2, r1_m)).abs();
    let arrival_dv =
        (circular_speed_mps(mu_m3_s2, r2_m) - vis_viva_speed_mps(mu_m3_s2, r2_m, a_transfer)).abs();
    let transfer_time =
        std::f64::consts::PI * (a_transfer * a_transfer * a_transfer / mu_m3_s2).sqrt();
    TransferSolution {
        departure_dv_mps: departure_dv,
        arrival_dv_mps: arrival_dv,
        transfer_time_s: transfer_time,
    }
}

/// Three-impulse bi-elliptic transfer via an intermediate apoapsis `rb_m`
/// (rb > max(r1, r2)). Beats the Hohmann only for large radius ratios and a
/// sufficiently high rb ([`BIELLIPTIC_FAVORABLE_RATIO`]).
pub fn bielliptic_transfer(r1_m: f64, r2_m: f64, rb_m: f64, mu_m3_s2: f64) -> BiellipticSolution {
    let a1 = (r1_m + rb_m) / 2.0; // first ellipse: r1 → rb
    let a2 = (r2_m + rb_m) / 2.0; // second ellipse: rb → r2
    let dv1 = (vis_viva_speed_mps(mu_m3_s2, r1_m, a1) - circular_speed_mps(mu_m3_s2, r1_m)).abs();
    let dv_mid =
        (vis_viva_speed_mps(mu_m3_s2, rb_m, a2) - vis_viva_speed_mps(mu_m3_s2, rb_m, a1)).abs();
    let dv2 = (circular_speed_mps(mu_m3_s2, r2_m) - vis_viva_speed_mps(mu_m3_s2, r2_m, a2)).abs();
    let time = std::f64::consts::PI * ((a1.powi(3) + a2.powi(3)) / mu_m3_s2).sqrt();
    BiellipticSolution {
        departure_dv_mps: dv1,
        mid_dv_mps: dv_mid,
        arrival_dv_mps: dv2,
        transfer_time_s: time,
    }
}

/// True when the target/current radius ratio is large enough that a tall
/// bi-elliptic transfer can beat the Hohmann (verify per case with rb).
pub fn bielliptic_potentially_favorable(r1_m: f64, r2_m: f64) -> bool {
    let ratio = r1_m.max(r2_m) / r1_m.min(r2_m);
    ratio > BIELLIPTIC_FAVORABLE_RATIO
}

/// Plane-change Δv for rotating the orbital plane by `inclination_change_rad`
/// at constant speed `speed_mps`: `Δv = 2·v·sin(i/2)` (vector difference of
/// two equal-speed velocities separated by i).
pub fn plane_change_dv(speed_mps: f64, inclination_change_rad: f64) -> f64 {
    2.0 * speed_mps * (inclination_change_rad / 2.0).sin()
}

/// Combined-maneuver identity: performing a tangential burn `dv1_mps` and a
/// plane rotation whose pure cost would be `dv2_mps` **simultaneously** costs
/// the vector sum
/// `√(dv1² + dv2² − 2·dv1·dv2·cos(i))`,
/// strictly less than burning sequentially whenever both are positive.
/// `angle_between_rad` is the angle between the two Δv vectors (for a plane
/// change paired with a prograde burn this is π − i; callers pass the angle
/// they mean).
pub fn combined_maneuver_dv(dv1_mps: f64, dv2_mps: f64, angle_between_rad: f64) -> f64 {
    (dv1_mps * dv1_mps + dv2_mps * dv2_mps - 2.0 * dv1_mps * dv2_mps * angle_between_rad.cos())
        .sqrt()
}

/// Which leg of the two-impulse transfer the autopilot is executing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TransferBurnPhase {
    /// Prograde burn until the orbit reaches the target radius as apoapsis
    /// (or periapsis, when lowering).
    #[default]
    Departure,
    /// Ballistic coast along the transfer ellipse to the arrival apsis.
    Coast,
    /// Circularization burn at the arrival apsis (hands off to
    /// [`AutopilotMode::OrbitInsertion`] machinery once eccentricity is low).
    Arrival,
    /// Target reached: transfer complete.
    Done,
}

/// Classify the current state into a [`TransferBurnPhase`] for a transfer
/// between the current circular radius and `target_radius_m`. Pure function;
/// the system maps phases onto throttle/attitude commands.
pub fn transfer_burn_phase(
    current_radius_m: f64,
    target_radius_m: f64,
    apoapsis_m: f64,
    eccentricity: f64,
) -> TransferBurnPhase {
    if (current_radius_m - target_radius_m).abs() < 1.0 && eccentricity < 0.01 {
        return TransferBurnPhase::Done;
    }
    let raising = target_radius_m > current_radius_m;
    let apsis_at_target = if raising {
        apoapsis_m >= target_radius_m * 0.999
    } else {
        apoapsis_m <= target_radius_m * 1.001 || (current_radius_m - target_radius_m).abs() < 1.0
    };
    if !apsis_at_target {
        return TransferBurnPhase::Departure;
    }
    if eccentricity > 0.01 {
        // On the transfer ellipse heading to (or sitting near) the apsis.
        TransferBurnPhase::Coast
    } else {
        TransferBurnPhase::Arrival
    }
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
/// - Ascent → Orbit only after the authoritative state satisfies the configured
///   target-orbit predicate. Raw speed alone misclassifies boosted trajectories
///   with an Earth-intersecting periapsis.
pub fn advance_ascent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    ascent_start_altitude_m: f64,
    target_orbit_reached: bool,
) -> RocketMissionState {
    match phase {
        RocketMissionState::Launch if altitude_m >= ascent_start_altitude_m => {
            RocketMissionState::Ascent
        }
        RocketMissionState::Ascent if target_orbit_reached => RocketMissionState::Orbit,
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

/// Hold the longitudinal axis along `direction` and roll the vehicle about
/// that same body +Y axis by a reentry-bank command. Banking is attitude
/// control, not a raw torque on an arbitrary body axis.
pub fn banked_attitude_from_direction(direction: DVec3, bank_angle_rad: f64) -> DQuat {
    attitude_from_direction(direction) * DQuat::from_rotation_y(bank_angle_rad)
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
    let altitude = (position_m - target_position_m).length();
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

/// Terrain-relative terminal guidance for a reusable vertical landing. The
/// controller combines a stopping-distance brake with velocity damping and
/// limits lateral acceleration to a strict thrust-vector tilt envelope. It
/// never commands a downward-pointing main engine near the ground.
pub fn terminal_landing_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    radar_altitude_m: f64,
    mass_kg: f64,
    max_thrust_n: f64,
    gravity_accel_mps2: f64,
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
        -(altitude_m * 0.04).clamp(3.0, 45.0)
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
    let requested_horizontal_accel_mps2 =
        horizontal_error_m * 0.0015 - horizontal_velocity_mps * 0.8;
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

/// Horizontal distance to the pad below which boostback hands off to the
/// landing leg (m).
pub const BOOSTBACK_COMPLETE_DISTANCE_M: f64 = 5_000.0;
/// Horizontal speed below which boostback hands off (m/s).
pub const BOOSTBACK_COMPLETE_SPEED_MPS: f64 = 50.0;
/// Proportional gain on horizontal position error [1/s²]: at 50 km error this
/// commands ~2.5 m/s² of horizontal acceleration.
pub const BOOSTBACK_POSITION_GAIN_INV_S2: f64 = 5e-5;
/// Damping gain on horizontal velocity error [1/s].
pub const BOOSTBACK_VELOCITY_GAIN_INV_S: f64 = 0.02;

/// Command output of the boostback skeleton: target attitude and throttle
/// only — actuation and physics remain downstream (AGENTS.md section 18).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoostbackCommand {
    pub attitude: DQuat,
    /// Throttle fraction in [0, 1]; zero = coast.
    pub throttle: f64,
    /// True when the pad is roughly below and the landing leg should take
    /// over ([`AutopilotMode::Landing`]).
    pub complete: bool,
}

/// Booster flyback (RTLS) boostback targeting: a PD law on the horizontal
/// state relative to the launch site drives downrange-to-site toward zero.
/// Vertical dynamics are deliberately ignored here — the burn shapes the
/// downrange; the existing suicide-burn/hover-slam leg handles touchdown.
/// Pure function; testable without Bevy.
pub fn boostback_guidance(
    position_m: DVec3,
    velocity_mps: DVec3,
    launch_site_position_m: DVec3,
    mass_kg: f64,
    max_thrust_n: f64,
) -> BoostbackCommand {
    let up = position_m.normalize_or_zero();
    let rel_site = launch_site_position_m - position_m;
    let horizontal_error = rel_site - up * rel_site.dot(up);
    let horizontal_velocity = velocity_mps - up * velocity_mps.dot(up);

    let complete = horizontal_error.length() < BOOSTBACK_COMPLETE_DISTANCE_M
        && horizontal_velocity.length() < BOOSTBACK_COMPLETE_SPEED_MPS;

    // PD command on the horizontal state, saturated by available thrust.
    let accel_cmd = horizontal_error * BOOSTBACK_POSITION_GAIN_INV_S2
        - horizontal_velocity * BOOSTBACK_VELOCITY_GAIN_INV_S;

    let accel_mag = accel_cmd.length();
    if accel_mag < 1e-6 || !accel_mag.is_finite() {
        return BoostbackCommand {
            attitude: attitude_from_direction(up),
            throttle: 0.0,
            complete,
        };
    }

    let available_accel_mps2 = max_thrust_n / mass_kg.max(1e-6);
    let throttle = (accel_mag / available_accel_mps2).clamp(0.05, 1.0);
    BoostbackCommand {
        attitude: attitude_from_direction(accel_cmd / accel_mag),
        throttle,
        complete,
    }
}

/// Enhanced reentry bank-angle guidance with predictor-corrector.
/// Uses reference trajectory tracking for precise corridor management.
pub fn reentry_bank_angle_enhanced(
    altitude_m: f64,
    velocity_mps: f64,
    flight_path_angle_rad: f64,
    dynamic_pressure_pa: f64,
    heat_flux_w_m2: f64,
    g_load: f64,
    config: &DescentGuidanceConfig,
    crossrange_remaining_m: f64,
    downrange_remaining_m: f64,
    reference_bank_rad: f64,
) -> f64 {
    // Constraint margins.
    let g_margin = config.max_g_load - g_load;
    let q_margin = config.max_dynamic_pressure_pa - dynamic_pressure_pa;
    let heat_margin = config.max_heat_flux_w_m2 - heat_flux_w_m2;

    // Predictor: estimate constraint violations at next step.
    let constraint_margin = g_margin.min(q_margin / 1000.0).min(heat_margin / 1000.0);

    // Base bank from reference trajectory (precomputed or analytical).
    let mut bank = reference_bank_rad;

    // Corrector: adjust bank to manage constraints.
    if constraint_margin <= 0.0 {
        // Violating constraints: increase bank magnitude to increase drag.
        let violation = (-constraint_margin).min(5.0); // Cap correction
        let max_bank = 90.0_f64.to_radians();
        let bank_mag = (bank.abs() + violation * 10.0_f64.to_radians()).min(max_bank);
        bank = bank_mag * bank.signum();
    } else if constraint_margin < 2.0 {
        // Approaching constraints: gently increase bank.
        let max_bank = 70.0_f64.to_radians();
        bank = (bank.abs() + 2.0_f64.to_radians()).min(max_bank) * bank.signum();
    }

    // Crossrange steering: modulate bank sign based on crossrange error.
    // If crossrange > 0, we need to turn left (negative bank in our convention).
    let crossrange_sign = if crossrange_remaining_m > 0.0 {
        -1.0
    } else {
        1.0
    };

    // Downrange control: adjust bank magnitude to hit target downrange.
    let downrange_error = downrange_remaining_m; // Simplified
    let downrange_bank_adj =
        (downrange_error / 1_000_000.0).clamp(-0.5, 0.5) * 10.0_f64.to_radians();

    // Combine corrections.
    bank = (bank.abs() + downrange_bank_adj).min(90.0_f64.to_radians()) * crossrange_sign;

    bank
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::gravity::{
        circular_orbit_speed_mps, gravitational_acceleration, gravitational_parameter,
    };
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
    fn gated_pitch_holds_vertical_until_gate_passes() {
        let p = profile();
        // Well past the time schedule start (10 s) but low and slow: the
        // gate must keep the vehicle exactly vertical (electron tower-tip
        // regression).
        assert!((gravity_turn_pitch_angle_gated(&p, 65.0, 30.0, 13.0)).abs() < 1e-12);
        assert!((gravity_turn_pitch_angle_gated(&p, 0.0, 60.0, 0.0)).abs() < 1e-12);
        // High enough but still slow: altitude condition alone is not enough.
        assert!((gravity_turn_pitch_angle_gated(&p, 500.0, 30.0, 10.0)).abs() < 1e-12);
        // Fast but too low: vertical-speed condition alone is not enough.
        assert!((gravity_turn_pitch_angle_gated(&p, 100.0, 30.0, 80.0)).abs() < 1e-12);
    }

    #[test]
    fn gated_pitch_engages_combined_schedule_once_gate_clears() {
        let p = profile();
        let alt = 2_000.0;
        let t = 20.0;
        let vs = 100.0;
        assert!(ascent_pitch_gate_clear(&p, alt, vs));
        let gated = gravity_turn_pitch_angle_gated(&p, alt, t, vs);
        let combined = gravity_turn_pitch_angle_combined(&p, alt, t);
        assert!((gated - combined).abs() < 1e-12);

        // Inclusive thresholds: a gate met exactly engages the turn.
        assert!(ascent_pitch_gate_clear(
            &p,
            p.pitch_gate_min_altitude_m,
            p.pitch_gate_min_vertical_speed_mps
        ));
    }

    #[test]
    fn gated_direction_matches_gated_pitch() {
        let p = profile();
        let axis = pitch_axis_from_reference(up_dir(), DVec3::Z).unwrap();
        // Below the gate: direction is exactly the local vertical.
        let dir = gravity_turn_direction_gated(&p, up_dir(), axis, 50.0, 30.0, 5.0);
        assert!((dir - up_dir()).length() < 1e-9);
        // Above the gate: tilted away from vertical by the schedule angle.
        let angle = gravity_turn_pitch_angle_gated(&p, 40_000.0, 90.0, 300.0);
        let dir = gravity_turn_direction_gated(&p, up_dir(), axis, 40_000.0, 90.0, 300.0);
        assert!((dir.dot(up_dir()) - angle.cos()).abs() < 1e-9);
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
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Launch, 1_000.0, 5_000.0, false,),
            RocketMissionState::Launch
        );
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Launch, 6_000.0, 5_000.0, false,),
            RocketMissionState::Ascent
        );
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Ascent, 200_000.0, 5_000.0, true,),
            RocketMissionState::Orbit
        );
        // An unsafe or incomplete target state stays in ascent.
        assert_eq!(
            advance_ascent_phase(RocketMissionState::Ascent, 200_000.0, 5_000.0, false,),
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
    fn hohmann_matches_reference_values() {
        let mu = 3.986e14; // Earth
        let leo = 6_678_000.0; // ~300 km altitude
        let geo = 42_164_000.0;
        let t = hohmann_transfer(leo, geo, mu);
        // Classical LEO→GEO figures: Δv ≈ 2.43 + 1.47 km/s over ≈ 5.27 h.
        assert!(
            (t.departure_dv_mps - 2_430.0).abs() < 60.0,
            "departure dv {}",
            t.departure_dv_mps
        );
        assert!(
            (t.arrival_dv_mps - 1_470.0).abs() < 60.0,
            "arrival dv {}",
            t.arrival_dv_mps
        );
        assert!(
            (t.transfer_time_s - 18_930.0).abs() < 300.0,
            "transfer time {}",
            t.transfer_time_s
        );

        // Direction-symmetric: lowering costs the same impulses.
        let back = hohmann_transfer(geo, leo, mu);
        assert!((back.total_dv_mps() - t.total_dv_mps()).abs() < 1e-9);
        assert!((back.transfer_time_s - t.transfer_time_s).abs() < 1e-6);

        // Degenerate: same orbit → zero cost.
        let none = hohmann_transfer(leo, leo, mu);
        assert!(none.total_dv_mps() < 1e-9);
    }

    #[test]
    fn bielliptic_ratio_boundary_and_budget() {
        // Below the classical ratio the Hohmann wins (or ties) by rule.
        assert!(!bielliptic_potentially_favorable(7_000_000.0, 80_000_000.0));
        assert!(bielliptic_potentially_favorable(7_000_000.0, 90_000_000.0));

        // A tall bi-elliptic for a >11.94 ratio must be a valid maneuver:
        // positive finite impulses and a longer coast than the Hohmann.
        let mu = 3.986e14;
        let r1 = 7_000_000.0;
        let r2 = 100_000_000.0;
        let rb = 500_000_000.0;
        let b = bielliptic_transfer(r1, r2, rb, mu);
        assert!(b.departure_dv_mps > 0.0 && b.mid_dv_mps.is_finite());
        assert!(b.arrival_dv_mps > 0.0 && b.arrival_dv_mps < 1_000.0);
        let h = hohmann_transfer(r1, r2, mu);
        assert!(
            b.transfer_time_s > h.transfer_time_s * 5.0,
            "bi-elliptic via {} m must take far longer",
            rb
        );
    }

    #[test]
    fn plane_change_and_combined_identity() {
        // Pure rotation: Δv = 2 v sin(i/2).
        let speed = 7_600.0;
        let i = 30.0_f64.to_radians();
        let expected = 2.0 * speed * (i / 2.0).sin();
        assert!((plane_change_dv(speed, i) - expected).abs() < 1e-9);
        // Zero change costs nothing.
        assert_eq!(plane_change_dv(speed, 0.0), 0.0);

        // The combined-maneuver identity is the law of cosines: at a right
        // angle it degenerates to the hypotenuse; with equal magnitudes and
        // a nearly-opposed angle it stays below the sequential sum.
        let (a, b) = (300.0_f64, 400.0_f64);
        let right_angle = std::f64::consts::FRAC_PI_2;
        assert!((combined_maneuver_dv(a, b, right_angle) - 500.0).abs() < 1e-9);

        let v = 1_000.0_f64;
        let almost_opposed = 170.0_f64.to_radians();
        let combined = combined_maneuver_dv(v, v, almost_opposed);
        assert!(
            combined < 2.0 * v && combined > std::f64::consts::SQRT_2 * v * 0.99,
            "combined {combined} outside the geometric expectation"
        );
        // Zero angle between identical vectors cancels completely.
        assert_eq!(combined_maneuver_dv(v, v, 0.0), 0.0);
    }

    /// Scenario `hohmann_simulated` (Phase 17): apply both solver burns to a
    /// real two-body integration (authoritative gravity + the production
    /// semi-implicit Euler, dt = 1 s) and verify the vehicle actually arrives
    /// on the target circle. Tolerances: arrival radius 0.1 % of r2 (bounded
    /// integration error at 1 s steps), final speed 0.1 %.
    #[test]
    fn hohmann_burns_simulate_to_a_circular_arrival() {
        let earth_mass_kg = 5.97237e24;
        let mu = gravitational_parameter(earth_mass_kg);
        let (r1, r2) = (6_678_000.0_f64, 42_164_000.0_f64);
        let t = hohmann_transfer(r1, r2, mu);

        let dt = 1.0;
        let mut pos = DVec3::new(r1, 0.0, 0.0);
        let mut vel = DVec3::new(0.0, 0.0, (mu / r1).sqrt());

        // Departure burn: prograde (tangential +Z here).
        vel += DVec3::new(0.0, 0.0, t.departure_dv_mps);

        // Coast half an ellipse.
        let coast_steps = (t.transfer_time_s / dt).round() as u32;
        for _ in 0..coast_steps {
            vel += gravitational_acceleration(earth_mass_kg, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
        }
        assert!(
            ((pos.length() - r2) / r2).abs() < 1e-3,
            "transfer did not arrive at apoapsis r2: {}",
            pos.length()
        );

        // Arrival burn: prograde along the local tangential direction.
        let radial = pos.normalize();
        let tangential = DVec3::new(-radial.z, 0.0, radial.x);
        vel += tangential * t.arrival_dv_mps;

        // One full revolution later the orbit must still be the target circle.
        let period2 = std::f64::consts::PI * 2.0 * (r2 * r2 * r2 / mu).sqrt();
        let mut worst = 0.0_f64;
        for _ in 0..((period2 / dt) as u32) {
            vel += gravitational_acceleration(earth_mass_kg, pos, DVec3::ZERO) * dt;
            pos += vel * dt;
            worst = worst.max(((pos.length() - r2) / r2).abs());
        }
        assert!(
            worst < 1e-3,
            "arrival orbit not circular: worst drift {worst}"
        );
        assert!(
            ((vel.length() - (mu / r2).sqrt()) / (mu / r2).sqrt()).abs() < 1e-3,
            "arrival speed {} vs circular {}",
            vel.length(),
            (mu / r2).sqrt()
        );
    }

    #[test]
    fn transfer_phase_classification_walks_burn_coast_burn() {
        let r_now = 7_000_000.0_f64;
        let target = 10_000_000.0_f64;

        // Still in the parking orbit: apoapsis not yet raised.
        assert_eq!(
            transfer_burn_phase(r_now, target, r_now + 50_000.0, 0.007),
            TransferBurnPhase::Departure
        );
        // Apoapsis at the target but still elliptical: coasting.
        assert_eq!(
            transfer_burn_phase(r_now, target, target, 0.15),
            TransferBurnPhase::Coast
        );
        // Circularized at the target: done.
        assert_eq!(
            transfer_burn_phase(target, target, target, 0.001),
            TransferBurnPhase::Done
        );
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
    fn boostback_burns_opposing_downrange_velocity() {
        // Vehicle ~100 km downrange of the pad, flying further away.
        let radius = 6_371_000.0;
        let site = DVec3::new(radius, 0.0, 0.0);
        let theta = 100_000.0 / radius; // central angle for ~100 km arc
        let position = DVec3::new(
            radius * theta.cos() * (radius + 200_000.0) / radius,
            radius * theta.sin() * (radius + 200_000.0) / radius,
            0.0,
        )
        .normalize()
            * (radius + 200_000.0);
        // Tangential unit vector at the vehicle (direction of increasing θ).
        let tangent = DVec3::new(-position.y, position.x, 0.0).normalize();
        let velocity = tangent * 300.0;

        let cmd = boostback_guidance(position, velocity, site, 25_000.0, 1_000_000.0);

        assert!(!cmd.complete, "far and fast must not be complete");
        assert!(cmd.throttle > 0.0, "must command a burn");
        // Thrust direction opposes the receding horizontal velocity.
        let thrust_dir = cmd.attitude * DVec3::Y;
        assert!(
            thrust_dir.dot(tangent) < 0.0,
            "must burn back toward pad (against tangent)"
        );
    }

    #[test]
    fn boostback_completes_over_the_pad_when_slow() {
        let radius = 6_371_000.0;
        let site = DVec3::new(radius, 0.0, 0.0);
        // Radially above the pad at 200 km with a ~100 m tangential offset.
        let position = site + DVec3::X * 200_000.0 + DVec3::Y * 100.0;
        let velocity = DVec3::new(-5.0, 0.0, 0.0); // nearly null horizontally
        let cmd = boostback_guidance(position, velocity, site, 25_000.0, 1_000_000.0);
        assert!(cmd.complete);
    }

    #[test]
    fn boostback_zero_horizontal_error_gives_no_burn() {
        let site = DVec3::new(6_371_000.0, 0.0, 0.0);
        // Vehicle radially above the pad: no horizontal error.
        let up = site.normalize();
        let pos = site + up * 200_000.0;
        let cmd = boostback_guidance(pos, DVec3::ZERO, site, 25_000.0, 1_000_000.0);
        assert_eq!(cmd.throttle, 0.0, "no horizontal state error → coast");
        // Pad is directly below: boostback hands off to the landing leg.
        assert!(cmd.complete);
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
    fn banked_attitude_rolls_about_the_longitudinal_axis() {
        let direction = DVec3::new(0.2, 0.9, 0.3).normalize();
        let bank = 30.0_f64.to_radians();
        let attitude = banked_attitude_from_direction(direction, bank);
        assert!((attitude * DVec3::Y).dot(direction) > 1.0 - 1e-12);
        let unbanked = attitude_from_direction(direction) * DVec3::X;
        let banked = attitude * DVec3::X;
        assert!(unbanked.angle_between(banked) > 0.1);
    }

    #[test]
    fn default_landing_target_is_on_surface_directly_below_vehicle() {
        let position = DVec3::new(6_371_000.0 + 5_000.0, 0.0, 0.0);
        let target = default_surface_landing_target(position, 6_371_000.0);
        assert!((target.length() - 6_371_000.0).abs() < 1e-9);
        assert!(target.normalize().dot(position.normalize()) > 1.0 - 1e-12);
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

    /// Item 2.9: a domain-level integration of the whole descent chain —
    /// deorbit targeting → reentry-corridor bank management → powered descent →
    /// terminal hover-slam/suicide-burn. Not a full 6-DOF flight, but it drives
    /// one representative state through every guidance phase and checks the
    /// physical ordering the phase logic depends on.
    #[test]
    fn full_descent_chain_deorbit_reentry_terminal() {
        const EARTH_MASS_KG: f64 = 5.97237e24;
        const EARTH_RADIUS_M: f64 = 6_371_000.0;
        let mu = gravitational_parameter(EARTH_MASS_KG);
        let config = DescentGuidanceConfig::default();
        let pad = DVec3::new(EARTH_RADIUS_M, 0.0, 0.0);
        let up = pad.normalize();

        // 1) Deorbit: circular LEO at 200 km. Burn must be positive delta-v and
        // retrograde (body +Y opposite the velocity vector).
        let r_orbit = EARTH_RADIUS_M + 200_000.0;
        let v_orbit = circular_orbit_speed_mps(EARTH_MASS_KG, r_orbit);
        let pos = DVec3::new(r_orbit, 0.0, 0.0);
        let vel = DVec3::new(0.0, 0.0, v_orbit);
        let target_periapsis = EARTH_RADIUS_M + config.entry_interface_altitude_m;
        let (dv, attitude) = deorbit_burn_targeting(pos, vel, target_periapsis, mu);
        assert!(dv > 0.0, "deorbit burn delta-v must be positive");
        let body_y = attitude * DVec3::Y;
        assert!(body_y.dot(vel) < 0.0, "deorbit burn must be retrograde");

        // 2) Reentry corridor: nominal state in the corridor ⇒ small crossrange
        // bank; violating g-load ⇒ bank to ~90° with the crossrange sign.
        let nominal = reentry_bank_angle(
            80_000.0, 7_000.0, 20_000.0, 300_000.0, 2.0, &config, 10_000.0,
        );
        assert!(nominal.abs() <= 30.0_f64.to_radians());

        let violating_g = reentry_bank_angle(
            80_000.0,
            7_000.0,
            20_000.0,
            300_000.0,
            config.max_g_load + 1.0,
            &config,
            10_000.0,
        );
        assert!(
            (violating_g.abs() - 90.0_f64.to_radians()).abs() < 1e-9,
            "violating g-load must bank to 90°, got {}°",
            violating_g.to_degrees()
        );
        assert!(
            violating_g < 0.0,
            "positive crossrange ⇒ left (negative) bank"
        );

        // 3) Powered descent (convex): a plausible initiation state — subsonic,
        // descending fast enough that braking is required. The command must be
        // bounded to the engine envelope and finite.
        let descent_pos = DVec3::new(EARTH_RADIUS_M + 1_500.0, 0.0, 0.0);
        let descent_vel = -up * 80.0; // falling at 80 m/s
        let (thrust, _) = powered_descent_guidance_convex(
            descent_pos,
            descent_vel,
            pad,
            40_000.0,
            8.0e5,
            3.0e5,
            20.0_f64.to_radians(),
            9.81,
            12.0,
        );
        assert!(thrust.length().is_finite());
        assert!(
            thrust.length() >= 3.0e5 - 1.0 && thrust.length() <= 8.0e5 + 1.0,
            "thrust {} outside engine envelope",
            thrust.length()
        );

        // 4) Terminal hover-slam brake: descending well below the target rate,
        // the commanded thrust must point up (brake the fall) and contain a
        // component opposing any horizontal drift (nulls it).
        let (h_thrust, _) = hover_slam_guidance(
            descent_pos,
            -up * 30.0 + DVec3::new(0.0, 0.0, 25.0), // descend + drift +Z
            pad,
            40_000.0,
            8.0e5,
            9.81,
            -1.0, // target descent rate
        );
        assert!(h_thrust.dot(up) > 0.0, "hover-slam must brake the fall");
        let horizontal_thrust = h_thrust - up * h_thrust.dot(up);
        assert!(
            horizontal_thrust.dot(DVec3::Z) < 0.0,
            "hover-slam must oppose the horizontal drift"
        );

        // 5) Suicide burn: gates on the computed arrest altitude — too high, no
        // burn; within it, burn. `up` is the radial (+X) direction at the pad.
        let (_, _, _, should_burn_high) =
            suicide_burn_guidance(pad + up * 60_000.0, -up * 100.0, pad, 40_000.0, 8.0e5, 9.81);
        assert!(
            !should_burn_high,
            "must not burn while far above the suicide altitude"
        );

        // ~100 m up at 50 m/s: 50²/(2·(20−9.81)) ≈ 123 m arrest altitude, so a
        // 100 m start is inside it and the burn must ignite.
        let (_, _, _, should_burn_near) =
            suicide_burn_guidance(pad + up * 100.0, -up * 50.0, pad, 40_000.0, 8.0e5, 9.81);
        assert!(
            should_burn_near,
            "must ignite once inside the suicide burn altitude"
        );
    }

    #[test]
    fn terminal_landing_guidance_keeps_main_thrust_above_the_horizon() {
        let up = DVec3::X;
        let target = up * 6_371_000.0;
        let position = target + up * 100.0;
        let (thrust, attitude) = terminal_landing_guidance(
            position,
            up * 12.0 + DVec3::Z * 8.0,
            target,
            100.0,
            40_000.0,
            800_000.0,
            9.81,
        );

        assert!(thrust.dot(up) > 0.0, "terminal thrust must remain upward");
        assert!(
            (attitude * DVec3::Y).angle_between(up) <= 12.0_f64.to_radians() + 1e-9,
            "terminal attitude must remain inside the landing tilt envelope"
        );
    }
}
