//! Aerodynamic force and torque model for the rocket.
//!
//! All formulas consume the shared atmosphere model
//! (`domain::services::atmosphere`). Forces are expressed in the vehicle body
//! frame where +Y is the longitudinal axis; systems rotate them to the
//! planet-inertial frame and feed the 6-DOF accumulators. Nothing here writes
//! a transform (AGENTS.md section 17).
//!
//! ## Model
//!
//! - Dynamic pressure: `q = ½·ρ·v²`.
//! - Mach: `M = v / a`.
//! - Angle of attack (pitch): `α = atan2(−v_x, v_y)` in body frame.
//! - Sideslip: `β = asin(v_z / |v|)` in body frame.
//! - Coefficients (analytic approximations, documented limits): constant base
//!   drag `Cd = 0.3`, linear lift `Cl = 1.5·α`, linear side `Cy = −0.8·β`.
//!   Tabulated data can replace these behind the same interface.
//! - Drag opposes velocity; lift acts along the body axis projected
//!   perpendicular to velocity; side force is mutually perpendicular.
//! - Center of pressure is a simple geometric estimate (slightly above the
//!   mid-length); aerodynamic torque is `τ = (r_CoP − r_COM) × F`.

use bevy::math::DVec3;

/// Dynamic pressure: `q = ½·ρ·v²`.
pub fn dynamic_pressure_q(density_kg_m3: f64, speed_mps: f64) -> f64 {
    0.5 * density_kg_m3 * speed_mps * speed_mps
}

/// Mach number: speed divided by the local speed of sound. Zero in vacuum.
pub fn mach_number(speed_mps: f64, speed_of_sound_mps: f64) -> f64 {
    if speed_of_sound_mps <= 1e-9 {
        0.0
    } else {
        speed_mps / speed_of_sound_mps
    }
}

/// Angle of attack (pitch), radians, in the body frame. Positive when the nose
/// is above the velocity vector.
pub fn angle_of_attack(body_velocity: DVec3) -> f64 {
    (-body_velocity.x).atan2(body_velocity.y)
}

/// Sideslip angle, radians, in the body frame. Positive when the velocity has
/// a +Z (right) body component.
pub fn angle_of_sideslip(body_velocity: DVec3) -> f64 {
    let speed = body_velocity.length();
    if speed < 1e-9 {
        0.0
    } else {
        (body_velocity.z / speed).asin()
    }
}

/// Analytic aerodynamic coefficients: `(cd, cl, cy)` from angle of attack and
/// sideslip. Linear models; documented approximations.
pub fn aerodynamic_coefficients(alpha_rad: f64, beta_rad: f64) -> (f64, f64, f64) {
    let cd = 0.3;
    let cl = 1.5 * alpha_rad;
    let cy = -0.8 * beta_rad;
    (cd, cl, cy)
}

/// How strongly relative nose bluntness raises base drag. An ablated (blunter)
/// nose increases the wave-drag contribution; the ratio is current/initial
/// nose radius, so an unablated vehicle (ratio 1) keeps the baseline Cd.
pub const NOSE_BLUNTNNESS_DRAG_FACTOR: f64 = 0.5;

/// Nose-bluntness-aware variant of [`aerodynamic_coefficients`]:
/// `nose_radius_ratio = nose_radius_current / nose_radius_initial ≥ 1`.
/// Ablation blunts the nose, raising Cd (and its base component); lift and
/// side force are unchanged. A ratio of exactly 1 reproduces
/// [`aerodynamic_coefficients`] bit-for-bit (regression-tested).
pub fn aerodynamic_coefficients_with_nose_bluntness(
    alpha_rad: f64,
    beta_rad: f64,
    nose_radius_ratio: f64,
) -> (f64, f64, f64) {
    let (base_cd, cl, cy) = aerodynamic_coefficients(alpha_rad, beta_rad);
    let bluntness_growth = (nose_radius_ratio.max(1.0) - 1.0).min(MAX_BLUNTNNESS_GROWTH);
    let cd = base_cd * (1.0 + NOSE_BLUNTNNESS_DRAG_FACTOR * bluntness_growth);
    (cd, cl, cy)
}

/// Cap on the modeled bluntness growth so extreme recession cannot produce
/// unphysical drag (Cd ≤ 2× baseline).
pub const MAX_BLUNTNNESS_GROWTH: f64 = 2.0;

/// Drag force in the body frame: opposes the velocity, magnitude `q·Cd·A`.
pub fn drag_force_body(
    dynamic_pressure_pa: f64,
    drag_coefficient: f64,
    reference_area_m2: f64,
    body_velocity: DVec3,
) -> DVec3 {
    let speed = body_velocity.length();
    if speed < 1e-9 {
        return DVec3::ZERO;
    }
    -body_velocity / speed * (dynamic_pressure_pa * drag_coefficient * reference_area_m2)
}

/// Lift force in the body frame: along the body axis projected perpendicular
/// to the velocity, magnitude `q·Cl·A`.
pub fn lift_force_body(
    dynamic_pressure_pa: f64,
    lift_coefficient: f64,
    reference_area_m2: f64,
    body_velocity: DVec3,
) -> DVec3 {
    let Some(direction) = lift_direction(body_velocity) else {
        return DVec3::ZERO;
    };
    direction * (dynamic_pressure_pa * lift_coefficient * reference_area_m2)
}

/// Side force in the body frame: perpendicular to both velocity and lift,
/// magnitude `q·Cy·A`.
pub fn side_force_body(
    dynamic_pressure_pa: f64,
    side_coefficient: f64,
    reference_area_m2: f64,
    body_velocity: DVec3,
) -> DVec3 {
    let Some(lift_dir) = lift_direction(body_velocity) else {
        return DVec3::ZERO;
    };
    let speed = body_velocity.length();
    let vel_unit = body_velocity / speed;
    let side_dir = vel_unit.cross(lift_dir);
    side_dir * (dynamic_pressure_pa * side_coefficient * reference_area_m2)
}

/// Unit vector of lift direction: the body +Y axis projected perpendicular to
/// the velocity, normalized. `None` when the projection is degenerate (flow
/// aligned with the body axis).
fn lift_direction(body_velocity: DVec3) -> Option<DVec3> {
    let speed = body_velocity.length();
    if speed < 1e-9 {
        return None;
    }
    let vel_unit = body_velocity / speed;
    let projection = DVec3::Y - vel_unit * vel_unit.y;
    let len = projection.length();
    if len < 1e-9 {
        None
    } else {
        Some(projection / len)
    }
}

/// Simple geometric center-of-pressure estimate in the body frame, meters:
/// slightly above the mid-length so a nose-heavy fueled vehicle leaves static
/// margin for the controller. Documented approximation; refine with a real
/// geometry model later.
pub fn center_of_pressure_m(height_m: f64) -> DVec3 {
    DVec3::new(0.0, height_m * 0.25, 0.0)
}

/// Aerodynamic torque about the center of mass, body frame:
/// `τ = (r_application − r_COM) × F`.
pub fn aerodynamic_torque_body(
    force_body: DVec3,
    application_point_body: DVec3,
    center_of_mass_body: DVec3,
) -> DVec3 {
    (application_point_body - center_of_mass_body).cross(force_body)
}

/// Monotonic Max Q tracking: the running maximum dynamic pressure, never
/// decreasing.
pub fn update_max_q(dynamic_pressure_pa: f64, max_q_pa: f64) -> f64 {
    dynamic_pressure_pa.max(max_q_pa)
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: f64 = std::f64::consts::PI * 1.85 * 1.85; // ~10.75 m²

    #[test]
    fn dynamic_pressure_is_half_rho_v_squared() {
        let q = dynamic_pressure_q(1.225, 250.0);
        assert!((q - 0.5 * 1.225 * 250.0 * 250.0).abs() < 1e-6);
    }

    #[test]
    fn mach_is_speed_over_speed_of_sound() {
        assert!((mach_number(680.6, 340.3) - 2.0).abs() < 1e-9);
        assert_eq!(mach_number(100.0, 0.0), 0.0); // vacuum
    }

    #[test]
    fn angle_of_attack_reflects_pitch() {
        assert!(angle_of_attack(DVec3::new(0.0, 10.0, 0.0)).abs() < 1e-9);
        // Nose above velocity → negative x component → positive alpha.
        let alpha = angle_of_attack(DVec3::new(-1.0, 10.0, 0.0));
        assert!(alpha > 0.0);
        // Nose below velocity → positive x component → negative alpha.
        let alpha_down = angle_of_attack(DVec3::new(1.0, 10.0, 0.0));
        assert!(alpha_down < 0.0);
        assert!((alpha + alpha_down).abs() < 1e-9);
    }

    #[test]
    fn sideslip_reflects_yaw_component() {
        assert!(angle_of_sideslip(DVec3::new(0.0, 10.0, 0.0)).abs() < 1e-9);
        let beta = angle_of_sideslip(DVec3::new(0.0, 10.0, 1.0));
        assert!((beta - (1.0 / (101.0f64).sqrt()).asin()).abs() < 1e-9);
        assert!(beta > 0.0);
    }

    #[test]
    fn drag_opposes_velocity_with_q_cd_a() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(0.0, 100.0, 0.0);
        let drag = drag_force_body(q, 0.3, AREA, vel);
        assert!(drag.dot(vel) < 0.0, "drag must oppose velocity");
        assert!((drag.length() - q * 0.3 * AREA).abs() < 1e-6);
        // Pure axial flow → drag straight down the body axis.
        assert!(drag.x.abs() < 1e-6 && drag.z.abs() < 1e-6);
    }

    #[test]
    fn lift_is_perpendicular_and_rises_with_alpha() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(-1.0, 10.0, 0.0);
        let (_, cl, _) = aerodynamic_coefficients(angle_of_attack(vel), angle_of_sideslip(vel));
        let lift = lift_force_body(q, cl, AREA, vel);
        assert!(
            lift.dot(vel).abs() < 1e-6,
            "lift must be perpendicular to velocity"
        );
        assert!((lift.length() - q * cl * AREA).abs() < 1e-6);
        assert!(lift.y > 0.0, "positive alpha must produce upward lift");
    }

    #[test]
    fn side_force_is_mutually_perpendicular() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(0.0, 10.0, 1.0);
        let (_, _, cy) = aerodynamic_coefficients(angle_of_attack(vel), angle_of_sideslip(vel));
        let lift = lift_force_body(q, 0.0, AREA, vel);
        let side = side_force_body(q, cy, AREA, vel);
        assert!(side.dot(vel).abs() < 1e-6);
        assert!(side.dot(lift).abs() < 1e-6);
        assert!((side.length() - q * cy.abs() * AREA).abs() < 1e-6);
    }

    #[test]
    fn no_aero_force_when_aligned_with_flow() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(0.0, 100.0, 0.0);
        let (_, cl, cy) = aerodynamic_coefficients(0.0, 0.0);
        assert_eq!(lift_force_body(q, cl, AREA, vel), DVec3::ZERO);
        assert_eq!(side_force_body(q, cy, AREA, vel), DVec3::ZERO);
    }

    #[test]
    fn aerodynamic_torque_from_cop_offset() {
        let cop = DVec3::new(0.0, 5.0, 0.0);
        let com = DVec3::new(0.0, -20.0, 0.0);
        let force = DVec3::new(0.0, 0.0, 1_000.0);
        let torque = aerodynamic_torque_body(force, cop, com);
        // r = (0, 25, 0) × F = (25*1000, 0, 0).
        assert!((torque - DVec3::new(25_000.0, 0.0, 0.0)).length() < 1e-6);
        // Torque is perpendicular to the force it comes from.
        assert!(torque.dot(force).abs() < 1e-6);
    }

    #[test]
    fn center_of_pressure_above_mid_length() {
        let cop = center_of_pressure_m(70.0);
        assert_eq!(cop, DVec3::new(0.0, 17.5, 0.0));
    }

    #[test]
    fn max_q_peak_is_monotonic() {
        assert_eq!(update_max_q(10.0, 5.0), 10.0);
        assert_eq!(update_max_q(3.0, 5.0), 5.0);
        assert_eq!(update_max_q(8.0, 8.0), 8.0);
    }

    #[test]
    fn zero_ablation_reproduces_baseline_coefficients_exactly() {
        // Old-vs-new comparison: an unablated nose (ratio 1) must give the
        // identical coefficients the pre-ablation model produced.
        for (alpha, beta) in [(0.0, 0.0), (0.1, -0.05), (-0.35, 0.2)] {
            let (cd_old, cl_old, cy_old) = aerodynamic_coefficients(alpha, beta);
            let (cd_new, cl_new, cy_new) =
                aerodynamic_coefficients_with_nose_bluntness(alpha, beta, 1.0);
            assert_eq!((cd_old, cl_old, cy_old), (cd_new, cl_new, cy_new));
        }
    }

    #[test]
    fn ablation_grows_only_base_drag() {
        let alpha = 0.15;
        let beta = -0.03;
        let (_, cl, cy) = aerodynamic_coefficients(alpha, beta);
        let mut previous_cd = aerodynamic_coefficients(alpha, beta).0;
        for ratio in [1.5_f64, 2.0, 3.0] {
            let (cd, cl_r, cy_r) = aerodynamic_coefficients_with_nose_bluntness(alpha, beta, ratio);
            assert!(cd > previous_cd, "Cd must grow with bluntness ({ratio})");
            assert_eq!((cl_r, cy_r), (cl, cy), "lift/side coefficients unchanged");
            assert!(cd <= 0.6 + 1e-12, "Cd capped at 2x baseline");
            previous_cd = cd;
        }
    }
}
