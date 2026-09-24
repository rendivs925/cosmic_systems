//! Entry and descent-phase guidance: corridor management, surface range
//! errors, and descent phase transitions.

use crate::domain::entities::rocket::RocketMissionState;
use crate::domain::math::DVec3;

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
            },
            "Mars" => Self {
                entry_interface_altitude_m: 125_000.0,
                max_g_load: 5.0,
                max_dynamic_pressure_pa: 40_000.0,
                max_heat_flux_w_m2: 500_000.0,
                powered_descent_altitude_m: 3_000.0,
                terminal_descent_altitude_m: 80.0,
                touchdown_vertical_velocity_mps: -0.8,
            },
            _ => Self::default(),
        }
    }
}

/// Advance the descent mission phase based on altitude, velocity, and propulsion state.
pub fn advance_descent_phase(
    phase: RocketMissionState,
    altitude_m: f64,
    velocity_mps: f64,
    dynamic_pressure_pa: f64,
    descending: bool,
    has_active_engines: bool,
    config: &DescentGuidanceConfig,
) -> RocketMissionState {
    let terminal_descent_phase = || {
        if has_active_engines {
            RocketMissionState::PoweredDescent
        } else {
            RocketMissionState::UnpoweredDescent
        }
    };
    match phase {
        // A failed ascent is still a descent. Without this handoff a vehicle
        // that loses usable propulsion remains labelled Ascent while falling,
        // leaving guidance at full throttle against unavailable engines.
        RocketMissionState::Ascent
            if descending && altitude_m < config.entry_interface_altitude_m =>
        {
            if velocity_mps < 340.0 && dynamic_pressure_pa < config.max_dynamic_pressure_pa {
                terminal_descent_phase()
            } else {
                RocketMissionState::ReentryCorridor
            }
        }
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
                terminal_descent_phase()
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

/// Reentry corridor guidance: computes bank angle command to maintain
/// trajectory within g-load, dynamic pressure, and heat flux limits.
/// Uses a simple bang-bang controller with predictor-corrector logic.
pub fn reentry_bank_angle(
    _altitude_m: f64,
    _velocity_mps: f64,
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

/// Target-relative surface range errors in the vehicle's local flight frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceRangeErrors {
    /// Positive when the target lies to the vehicle's left, m.
    pub crossrange_m: f64,
    /// Positive when the target lies ahead along the horizontal velocity, m.
    pub downrange_m: f64,
}

/// Both values are great-circle distances on the supplied reference sphere.
pub fn target_surface_range_errors_m(
    position_m: DVec3,
    velocity_mps: DVec3,
    target_position_m: DVec3,
    reference_radius_m: f64,
) -> SurfaceRangeErrors {
    if reference_radius_m <= 0.0
        || position_m.length_squared() <= 0.0
        || velocity_mps.length_squared() <= 0.0
        || target_position_m.length_squared() <= 0.0
    {
        return SurfaceRangeErrors {
            crossrange_m: 0.0,
            downrange_m: 0.0,
        };
    }

    let up = position_m.normalize();
    let target_up = target_position_m.normalize();
    let target_tangent = target_up - up * target_up.dot(up);
    let horizontal_velocity = velocity_mps - up * velocity_mps.dot(up);
    if target_tangent.length_squared() <= 1e-12 || horizontal_velocity.length_squared() <= 1e-12 {
        return SurfaceRangeErrors {
            crossrange_m: 0.0,
            downrange_m: 0.0,
        };
    }

    let forward = horizontal_velocity.normalize();
    let right = up.cross(forward).normalize();
    let target_direction = target_tangent.normalize();
    let arc_distance_m = reference_radius_m * up.dot(target_up).clamp(-1.0, 1.0).acos();
    let downrange_m = arc_distance_m * target_direction.dot(forward);
    let crossrange_m = -arc_distance_m * target_direction.dot(right);
    SurfaceRangeErrors {
        crossrange_m,
        downrange_m,
    }
}

/// Enhanced reentry bank-angle guidance with predictor-corrector.
/// Uses reference trajectory tracking for precise corridor management.
#[expect(
    clippy::too_many_arguments,
    reason = "The reentry model accepts independent measured state and constraint inputs."
)]
pub fn reentry_bank_angle_enhanced(
    _altitude_m: f64,
    _velocity_mps: f64,
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
