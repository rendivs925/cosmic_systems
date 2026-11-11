//! Aerodynamic force and torque model for the rocket.
//!
//! All formulas consume the shared atmosphere model
//! (`domain::services::atmosphere`). Forces are expressed in the vehicle body
//! frame where +Y is the longitudinal axis, +X and +Z are the two lateral axes
//! (`X × Y = Z`) as defined by `rocket_dynamics`; systems rotate them to the
//! planet-inertial frame and feed the 6-DOF accumulators. Nothing here writes
//! a transform (AGENTS.md section 17).
//!
//! ## Body axes
//!
//! The airframe is axisymmetric, so +X and +Z have no external landmark; they
//! are the two lateral directions of the current attitude. The model therefore
//! resolves the lateral flow into two per-axis planes instead of borrowing
//! aircraft names:
//!
//! - the **X-plane flow angle** `α_x` is in-plane: the angle between the
//!   velocity projection onto the body X-Y plane and the longitudinal +Y axis,
//!   positive toward −X; its normal force acts along body +X;
//! - the **Z flow angle** `β_z` is out-of-plane: the elevation of the velocity
//!   out of the X-Y plane along +Z; its side force acts along `lift_dir × v̂`
//!   (body +Z for nose-first flow).
//!
//! ## Model
//!
//! - Dynamic pressure: `q = ½·ρ·v²`.
//! - Mach: `M = v / a`.
//! - X-plane flow angle: `α_x = atan2(−v_x, v_y)` in the body frame.
//! - Z flow angle: `β_z = asin(v_z / |v|)` in the body frame.
//! - Coefficients (analytic approximations, documented limits): constant base
//!   drag `Cd = 0.3`, and the same linear normal-force slope in both lateral
//!   planes, `Cn_x = 1.5·α_x` and `Cn_z = −1.5·β_z`. The airframe is
//!   axisymmetric with no modelled fins, so the two lateral planes are
//!   physically equivalent; equal slopes make the force and torque model
//!   invariant under roll about the longitudinal axis. The previous `−0.8`
//!   yaw-plane slope was an arbitrary anisotropy, not a geometry result. The
//!   common magnitude retains the validated pitch-plane slope. Tabulated data
//!   can replace these behind the same interface.
//! - Drag opposes velocity. The X normal force acts along body +X projected
//!   perpendicular to velocity; the Z force acts along `lift_dir × v̂` for
//!   nose-first flow. Coefficient signs carry deflection direction.
//! - Center of pressure is a simple geometric estimate forward of the
//!   mid-length; aerodynamic torque is `τ = (r_CoP − r_COM) × F`.

use crate::domain::math::DVec3;

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

/// X-plane flow angle, radians, in the body frame: the angle of the velocity
/// projection onto the body X-Y plane, measured from the longitudinal +Y axis
/// toward −X. Positive when the velocity has a −X component.
///
/// A zero air-relative velocity has no defined flow angle. Returning `atan2`
/// on the signed-zero vector yields an arbitrary ±π, so the degenerate case is
/// reported as zero, matching [`angle_of_sideslip`]. Aerodynamic forces already
/// gate on airspeed, so this only affects recorded/sensed attitude telemetry.
pub fn angle_of_attack(body_velocity: DVec3) -> f64 {
    if body_velocity.length_squared() < 1e-18 {
        0.0
    } else {
        (-body_velocity.x).atan2(body_velocity.y)
    }
}

/// Z flow angle, radians, in the body frame: the elevation of the velocity out
/// of the body X-Y plane along +Z. Positive when the velocity has a +Z
/// component.
pub fn angle_of_sideslip(body_velocity: DVec3) -> f64 {
    let speed = body_velocity.length();
    if speed < 1e-9 {
        0.0
    } else {
        (body_velocity.z / speed).asin()
    }
}

/// Common lateral normal-force slope, per radian. The airframe is
/// axisymmetric, so both lateral planes must use this one value; the sign is
/// carried by each plane's coefficient definition below.
pub const LATERAL_NORMAL_FORCE_SLOPE_PER_RAD: f64 = 1.5;

/// Analytic aerodynamic coefficients for the two lateral body axes:
/// `(cd, cn_x, cn_z)` from the X-plane and Z-plane flow angles. `cn_x` scales
/// the normal force along +X; `cn_z` scales the side force along `lift_dir × v̂`
/// (body +Z for nose-first flow). Linear models; documented approximations.
/// Both lateral slopes have the same magnitude by axisymmetry.
pub fn aerodynamic_coefficients(alpha_rad: f64, beta_rad: f64) -> (f64, f64, f64) {
    let cd = 0.3;
    let cn_x = LATERAL_NORMAL_FORCE_SLOPE_PER_RAD * alpha_rad;
    let cn_z = -LATERAL_NORMAL_FORCE_SLOPE_PER_RAD * beta_rad;
    (cd, cn_x, cn_z)
}

/// How strongly relative nose bluntness raises base drag. An ablated (blunter)
/// nose increases the wave-drag contribution; the ratio is current/initial
/// nose radius, so an unablated vehicle (ratio 1) keeps the baseline Cd.
pub const NOSE_BLUNTNNESS_DRAG_FACTOR: f64 = 0.5;

/// Nose-bluntness-aware variant of [`aerodynamic_coefficients`]:
/// `nose_radius_ratio = nose_radius_current / nose_radius_initial ≥ 1`.
/// Ablation blunts the nose, raising Cd (and its base component); the two
/// lateral normal coefficients are unchanged. A ratio of exactly 1 reproduces
/// [`aerodynamic_coefficients`] bit-for-bit (regression-tested).
pub fn aerodynamic_coefficients_with_nose_bluntness(
    alpha_rad: f64,
    beta_rad: f64,
    nose_radius_ratio: f64,
) -> (f64, f64, f64) {
    let (base_cd, cn_x, cn_z) = aerodynamic_coefficients(alpha_rad, beta_rad);
    let bluntness_growth = (nose_radius_ratio.max(1.0) - 1.0).min(MAX_BLUNTNNESS_GROWTH);
    let cd = base_cd * (1.0 + NOSE_BLUNTNNESS_DRAG_FACTOR * bluntness_growth);
    (cd, cn_x, cn_z)
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

/// X-axis normal force in the body frame: along body +X projected
/// perpendicular to the velocity, magnitude `q·Cn_x·A`.
pub fn lift_force_body(
    dynamic_pressure_pa: f64,
    normal_x_coefficient: f64,
    reference_area_m2: f64,
    body_velocity: DVec3,
) -> DVec3 {
    let Some(direction) = lift_direction(body_velocity) else {
        return DVec3::ZERO;
    };
    direction * (dynamic_pressure_pa * normal_x_coefficient * reference_area_m2)
}

/// Z-axis side force in the body frame: perpendicular to both velocity and the
/// X normal direction, magnitude `q·Cn_z·A`.
pub fn side_force_body(
    dynamic_pressure_pa: f64,
    normal_z_coefficient: f64,
    reference_area_m2: f64,
    body_velocity: DVec3,
) -> DVec3 {
    let Some(lift_dir) = lift_direction(body_velocity) else {
        return DVec3::ZERO;
    };
    let speed = body_velocity.length();
    let vel_unit = body_velocity / speed;
    let side_dir = lift_dir.cross(vel_unit);
    side_dir * (dynamic_pressure_pa * normal_z_coefficient * reference_area_m2)
}

/// Signed X coefficients require a fixed +X normal-force basis. Projecting
/// longitudinal +Y instead would flip this basis with the flow angle and mix
/// the two lateral planes. `None` when the lateral projection is degenerate
/// (flow aligned with +X).
fn lift_direction(body_velocity: DVec3) -> Option<DVec3> {
    let speed = body_velocity.length();
    if speed < 1e-9 {
        return None;
    }
    let vel_unit = body_velocity / speed;
    let projection = DVec3::X - vel_unit * vel_unit.x;
    let len = projection.length();
    if len < 1e-9 {
        None
    } else {
        Some(projection / len)
    }
}

/// Simple geometric center-of-pressure estimate in the body frame, meters:
/// +0.25·height along +Y, forward (nose-ward) of the geometric center. The
/// modeled center of mass is aft of the geometric center while propellant
/// remains, so this is a **negative** static margin: the aerodynamic torque
/// destabilizes the pitch/yaw axes and the controller must actively stabilize
/// them. That is realistic for a launch vehicle. Documented approximation;
/// refine with a real geometry model later.
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

    #[test]
    fn signed_aerodynamic_coefficients_produce_mirrored_normal_forces() {
        let q = 20_000.0;
        for lateral in [-10.0, 10.0] {
            let x_plane_velocity = DVec3::new(lateral, 200.0, 0.0);
            let (_, cn_x, _) = aerodynamic_coefficients(angle_of_attack(x_plane_velocity), 0.0);
            let x_force = lift_force_body(q, cn_x, AREA, x_plane_velocity);
            assert!(
                x_force.x * lateral < 0.0,
                "X normal force must oppose X-plane flow: {x_force:?}"
            );
            assert!(
                x_force.dot(x_plane_velocity).abs() < 1e-6,
                "X normal force must do no work"
            );

            let z_plane_velocity = DVec3::new(0.0, 200.0, lateral);
            let (_, _, cn_z) = aerodynamic_coefficients(0.0, angle_of_sideslip(z_plane_velocity));
            let z_force = side_force_body(q, cn_z, AREA, z_plane_velocity);
            assert!(
                z_force.z * lateral < 0.0,
                "Z side force must oppose Z-plane flow: {z_force:?}"
            );
            assert!(
                z_force.dot(z_plane_velocity).abs() < 1e-6,
                "Z side force must do no work"
            );
        }
    }

    const AREA: f64 = std::f64::consts::PI * 1.85 * 1.85; // ~10.75 m²

    /// The airframe is axisymmetric, so a given flow angle must produce the
    /// same lateral force magnitude in either plane. Rotating the same
    /// body-relative flow by 90° about the longitudinal axis must not change
    /// the force, i.e. the model is roll-invariant (SC-03/SC-04 audit).
    #[test]
    fn lateral_planes_have_equal_normal_force_slopes_by_axisymmetry() {
        let angle_rad = 0.05_f64;
        let speed_mps = 200.0;
        let (_, cn_x, _) = aerodynamic_coefficients(angle_rad, 0.0);
        let (_, _, cn_z) = aerodynamic_coefficients(0.0, angle_rad);
        assert!((cn_x.abs() - cn_z.abs()).abs() < 1e-12);

        let q = 20_000.0;
        let x_plane_velocity = DVec3::new(-angle_rad.sin(), angle_rad.cos(), 0.0) * speed_mps;
        let z_plane_velocity = DVec3::new(0.0, angle_rad.cos(), angle_rad.sin()) * speed_mps;
        let (_, cn_x, _) = aerodynamic_coefficients(angle_of_attack(x_plane_velocity), 0.0);
        let (_, _, cn_z) = aerodynamic_coefficients(0.0, angle_of_sideslip(z_plane_velocity));
        let x_force = lift_force_body(q, cn_x, AREA, x_plane_velocity);
        let z_force = side_force_body(q, cn_z, AREA, z_plane_velocity);
        assert!(
            (x_force.length() - z_force.length()).abs() < 1e-6,
            "the two lateral planes must have equal force magnitude"
        );
    }

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
    fn x_plane_flow_angle_reflects_x_velocity() {
        assert!(angle_of_attack(DVec3::new(0.0, 10.0, 0.0)).abs() < 1e-9);
        // Velocity toward −X puts the nose on the +X side of the flow.
        let alpha = angle_of_attack(DVec3::new(-1.0, 10.0, 0.0));
        assert!(alpha > 0.0);
        // Velocity toward +X gives the mirrored negative angle.
        let alpha_down = angle_of_attack(DVec3::new(1.0, 10.0, 0.0));
        assert!(alpha_down < 0.0);
        assert!((alpha + alpha_down).abs() < 1e-9);
    }

    #[test]
    fn zero_air_relative_velocity_has_zero_flow_angles() {
        assert_eq!(angle_of_attack(DVec3::ZERO), 0.0);
        // Signed zeros must not produce ±π from atan2.
        assert_eq!(angle_of_attack(DVec3::new(-0.0, -0.0, 0.0)), 0.0);
        assert_eq!(angle_of_attack(DVec3::new(0.0, -0.0, -0.0)), 0.0);
        assert_eq!(angle_of_sideslip(DVec3::new(-0.0, -0.0, -0.0)), 0.0);
    }

    #[test]
    fn z_plane_flow_angle_reflects_z_velocity() {
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
    fn x_normal_force_is_perpendicular_and_rises_with_x_flow() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(-1.0, 10.0, 0.0);
        let (_, cn_x, _) = aerodynamic_coefficients(angle_of_attack(vel), angle_of_sideslip(vel));
        let lift = lift_force_body(q, cn_x, AREA, vel);
        assert!(
            lift.dot(vel).abs() < 1e-6,
            "the X normal force must be perpendicular to velocity"
        );
        assert!((lift.length() - q * cn_x * AREA).abs() < 1e-6);
        assert!(
            lift.x * vel.x < 0.0 && lift.y > 0.0,
            "a positive X-plane flow angle must produce a restoring +X force"
        );
    }

    #[test]
    fn lateral_force_planes_are_the_two_body_axes() {
        let q = dynamic_pressure_q(1.225, 100.0);
        // X-plane flow: force in the X-Y plane, no Z component.
        let x_vel = DVec3::new(1.0, 10.0, 0.0);
        let (_, cn_x, _) =
            aerodynamic_coefficients(angle_of_attack(x_vel), angle_of_sideslip(x_vel));
        let x_force = lift_force_body(q, cn_x, AREA, x_vel);
        assert!(x_force.x.abs() > 0.0);
        assert!(x_force.z.abs() < 1e-9, "X-plane force must not leak into Z");

        // Z-plane flow: force in the Y-Z plane, no X component.
        let z_vel = DVec3::new(0.0, 10.0, 1.0);
        let (_, _, cn_z) =
            aerodynamic_coefficients(angle_of_attack(z_vel), angle_of_sideslip(z_vel));
        let z_force = side_force_body(q, cn_z, AREA, z_vel);
        assert!(z_force.z.abs() > 0.0);
        assert!(z_force.x.abs() < 1e-9, "Z-plane force must not leak into X");
    }

    #[test]
    fn z_side_force_is_mutually_perpendicular() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(0.0, 10.0, 1.0);
        let (_, _, cn_z) = aerodynamic_coefficients(angle_of_attack(vel), angle_of_sideslip(vel));
        let lift = lift_force_body(q, 0.0, AREA, vel);
        let side = side_force_body(q, cn_z, AREA, vel);
        assert!(side.dot(vel).abs() < 1e-6);
        assert!(side.dot(lift).abs() < 1e-6);
        assert!((side.length() - q * cn_z.abs() * AREA).abs() < 1e-6);
    }

    #[test]
    fn no_aero_force_when_aligned_with_flow() {
        let q = dynamic_pressure_q(1.225, 100.0);
        let vel = DVec3::new(0.0, 100.0, 0.0);
        let (_, cn_x, cn_z) = aerodynamic_coefficients(0.0, 0.0);
        assert_eq!(lift_force_body(q, cn_x, AREA, vel), DVec3::ZERO);
        assert_eq!(side_force_body(q, cn_z, AREA, vel), DVec3::ZERO);
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
    fn center_of_pressure_is_forward_of_the_geometric_center() {
        let cop = center_of_pressure_m(70.0);
        assert_eq!(cop, DVec3::new(0.0, 17.5, 0.0));
    }

    #[test]
    fn fueled_center_of_mass_is_aft_of_the_center_of_pressure() {
        use crate::domain::services::rocket_dynamics::rocket_inertia_tensor;
        let (_, com) = rocket_inertia_tensor(25_600.0, 100_000.0, 1.85, 70.0);
        let cop = center_of_pressure_m(70.0);
        assert!(
            com.y < 0.0,
            "fuel-weighted center of mass must be aft of the geometric center"
        );
        assert!(
            cop.y > com.y,
            "CP forward of COM is a negative (destabilizing) static margin"
        );
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
            let (cd_old, cn_x_old, cn_z_old) = aerodynamic_coefficients(alpha, beta);
            let (cd_new, cn_x_new, cn_z_new) =
                aerodynamic_coefficients_with_nose_bluntness(alpha, beta, 1.0);
            assert_eq!((cd_old, cn_x_old, cn_z_old), (cd_new, cn_x_new, cn_z_new));
        }
    }

    #[test]
    fn ablation_grows_only_base_drag() {
        let alpha = 0.15;
        let beta = -0.03;
        let (_, cn_x, cn_z) = aerodynamic_coefficients(alpha, beta);
        let mut previous_cd = aerodynamic_coefficients(alpha, beta).0;
        for ratio in [1.5_f64, 2.0, 3.0] {
            let (cd, cn_x_r, cn_z_r) =
                aerodynamic_coefficients_with_nose_bluntness(alpha, beta, ratio);
            assert!(cd > previous_cd, "Cd must grow with bluntness ({ratio})");
            assert_eq!(
                (cn_x_r, cn_z_r),
                (cn_x, cn_z),
                "lateral normal coefficients unchanged"
            );
            assert!(cd <= 0.6 + 1e-12, "Cd capped at 2x baseline");
            previous_cd = cd;
        }
    }
}
