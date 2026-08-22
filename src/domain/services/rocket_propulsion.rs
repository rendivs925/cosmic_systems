//! Physically consistent rocket propulsion.
//!
//! Thrust follows `T = m_dot · Isp · g0`; mass flow is `m_dot = T / (Isp · g0)`.
//! This module is the single authority for the mass-flow formula (delegated to
//! by [`Rocket::mass_flow_rate_kg_s`]), propellant consumption, staging, ISP
//! selection, and gimbal torque. Systems feed thrust and torque into the 6-DOF
//! accumulator and never write the transform directly (AGENTS.md section 17).

use crate::domain::entities::rocket::{EngineState, RocketEngine, RocketStage};
use crate::domain::services::atmosphere::SEA_LEVEL_DENSITY_KG_M3;
use crate::domain::services::rocket_dynamics::rocket_inertia_tensor;
use bevy::math::{DMat3, DQuat, DVec3};

/// Standard gravity, m/s².
pub const STANDARD_GRAVITY_MPS2: f64 = 9.80665;

/// Δv imparted to the upper stage by the separation system along the vehicle
/// longitudinal axis (pusher springs / pneumatic pushers), m/s.
pub const SEPARATION_UPPER_DV_MPS: f64 = 1.0;

/// Retro-Δv applied to the spent stage opposite the separation axis (helps
/// back it out of the interstage), m/s. Zero disables the retro impulse.
pub const SPENT_STAGE_RETRO_DV_MPS: f64 = 0.5;

/// Minimum guaranteed distance between the separated bodies at separation,
/// m (interstage collision avoidance; see AGENTS.md section 71 scope note).
pub const MIN_SEPARATION_CLEARANCE_M: f64 = 2.0;

/// Default settle time after staging before the next stage's engines may
/// ignite (ullage: propellant must settle to the tank outlet in the new
/// acceleration environment before an air-start is safe), s.
pub const DEFAULT_ULLAGE_SETTLE_TIME_S: f32 = 2.0;

/// Ullage gate for engine ignition: blocked while the post-separation settle
/// time has not elapsed.
pub fn ignition_allowed_during_ullage(time_since_separation_s: f32, settle_time_s: f32) -> bool {
    if settle_time_s <= 0.0 {
        return true;
    }
    time_since_separation_s >= settle_time_s
}

/// Result of a stage separation impulse applied to both bodies.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SeparationOutcome {
    pub upper_velocity_mps: DVec3,
    pub spent_velocity_mps: DVec3,
}

/// Apply a stage-separation impulse: a relative Δv to the upper stage along
/// `separation_axis_body` (unit vector, body frame) and an optional retro-Δv
/// to the spent stage along the opposite direction. Pure function returning
/// updated velocities; positions are untouched (the caller guarantees
/// clearance via [`MIN_SEPARATION_CLEARANCE_M`]).
///
/// The two impulses are independent actuator effects (spring + retro motor),
/// so linear momentum is not exactly conserved when both fire — documented
/// idealization, consistent with prescribed-impulse separation models.
pub fn separation_impulse(
    shared_velocity_mps: DVec3,
    orientation: DQuat,
    separation_axis_body: DVec3,
    upper_dv_mps: f64,
    spent_retro_dv_mps: f64,
) -> SeparationOutcome {
    let axis_world = (orientation * separation_axis_body).normalize_or_zero();
    SeparationOutcome {
        upper_velocity_mps: shared_velocity_mps + axis_world * upper_dv_mps,
        spent_velocity_mps: shared_velocity_mps - axis_world * spent_retro_dv_mps,
    }
}

/// Thrust from mass flow and specific impulse: `T = m_dot · Isp · g0`.
pub fn thrust_from_isp(mass_flow_kg_s: f64, isp_s: f32) -> f64 {
    mass_flow_kg_s * isp_s as f64 * STANDARD_GRAVITY_MPS2
}

/// Mass flow for a given thrust and specific impulse: `m_dot = T / (Isp · g0)`.
pub fn mass_flow_from_thrust(thrust_n: f64, isp_s: f32) -> f64 {
    thrust_n / (isp_s as f64 * STANDARD_GRAVITY_MPS2)
}

/// Select the effective specific impulse from ambient density, blending
/// between sea-level and vacuum ISP as density drops toward vacuum (back-
/// pressure effect). At standard sea-level density the sea-level ISP applies;
/// in a vacuum the vacuum ISP applies. Consumes the shared atmosphere model.
pub fn selected_isp(isp_sea_level: f32, isp_vacuum: f32, density_kg_m3: f64) -> f32 {
    let t = (1.0 - (density_kg_m3 / SEA_LEVEL_DENSITY_KG_M3).clamp(0.0, 1.0)) as f32;
    isp_sea_level + (isp_vacuum - isp_sea_level) * t
}

/// Consume propellant at the given mass flow for `dt` seconds. Returns the
/// remaining propellant and the actually consumed mass (never exceeds what is
/// available).
pub fn consume_propellant(propellant_kg: f32, mass_flow_kg_s: f64, dt: f64) -> (f32, f32) {
    let remaining = (propellant_kg - (mass_flow_kg_s * dt) as f32).max(0.0);
    let consumed = propellant_kg - remaining;
    (remaining, consumed)
}

/// Clamp a gimbal deflection to the engine's mechanical range.
pub fn clamp_gimbal(deflection_rad: f32, gimbal_range_deg: f32) -> f32 {
    let range_rad = gimbal_range_deg.to_radians();
    deflection_rad.clamp(-range_rad, range_rad)
}

/// Gimbal torque about the vehicle center of mass from an engine's thrust-line
/// offset and deflected thrust direction, in the body frame:
/// `τ = (r_engine − r_com) × F_thrust`.
pub fn gimbal_torque_body(
    engine_position_m: DVec3,
    center_of_mass_m: DVec3,
    thrust_dir_body: DVec3,
    thrust_n: f64,
    gimbal_pitch_rad: f64,
    gimbal_yaw_rad: f64,
) -> DVec3 {
    let deflected = DQuat::from_rotation_x(gimbal_pitch_rad)
        * DQuat::from_rotation_z(gimbal_yaw_rad)
        * thrust_dir_body;
    let offset = engine_position_m - center_of_mass_m;
    offset.cross(deflected * thrust_n)
}

/// Map a commanded body-frame torque into gimbal pitch/yaw deflections for the
/// active stage's engines by inverting the real gimbal torque coupling at the
/// current stage geometry. The sign/magnitude therefore match the actual engine
/// layout (including a flipped sign when the engines sit above the COM, as on
/// the second stage). Returns `(pitch_rad, yaw_rad)` before the mechanical
/// range clamp.
pub fn allocate_gimbal_deflections(
    engines: &[RocketEngine],
    center_of_mass_m: DVec3,
    torque_cmd: DVec3,
    thrust_scale: f32,
) -> (f32, f32) {
    if engines.is_empty() {
        return (0.0, 0.0);
    }
    const TEST_DEFLECTION_RAD: f64 = 1e-3;
    let scale = thrust_scale.clamp(0.0, 1.0) as f64;

    let torque_for = |pitch: f64, yaw: f64| -> DVec3 {
        let mut total = DVec3::ZERO;
        for engine in engines {
            total += gimbal_torque_body(
                engine.position_m.as_dvec3(),
                center_of_mass_m,
                engine.thrust_axis.as_dvec3(),
                engine.max_thrust_kn as f64 * 1000.0 * scale,
                pitch,
                yaw,
            );
        }
        total
    };

    let t_pitch = torque_for(TEST_DEFLECTION_RAD, 0.0);
    let t_yaw = torque_for(0.0, TEST_DEFLECTION_RAD);

    let pitch_cmd = if t_pitch.x.abs() > 1e-6 {
        (torque_cmd.x / t_pitch.x * TEST_DEFLECTION_RAD) as f32
    } else {
        0.0
    };
    let yaw_cmd = if t_yaw.z.abs() > 1e-6 {
        (torque_cmd.z / t_yaw.z * TEST_DEFLECTION_RAD) as f32
    } else {
        0.0
    };
    (pitch_cmd, yaw_cmd)
}

/// Total mass of the vehicle considering only the active and future stages.
pub fn active_vehicle_mass(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
) -> f64 {
    let mut mass = 0.0;
    for (i, stage) in stages.iter().enumerate().skip(active_stage) {
        mass += stage.dry_mass_kg as f64;
        mass += propellant_remaining_kg.get(i).copied().unwrap_or(0.0) as f64;
    }
    mass
}

/// Inertia tensor and center of mass for the active vehicle, using the shared
/// geometric rocket model with the active stages' total dry and propellant
/// mass. Updates as propellant is consumed.
pub fn active_vehicle_inertia(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
    radius_m: f64,
    height_m: f64,
) -> (DMat3, DVec3) {
    let dry: f64 = stages
        .iter()
        .skip(active_stage)
        .map(|s| s.dry_mass_kg as f64)
        .sum();
    let propellant: f64 = propellant_remaining_kg
        .iter()
        .skip(active_stage)
        .map(|p| *p as f64)
        .sum();
    rocket_inertia_tensor(dry, propellant, radius_m, height_m)
}

/// The mass shed by separating the current stage (its dry mass plus remaining
/// residual propellant), returning the new active stage index and the shed
/// mass. Returns `None` when there is no stage left to shed.
pub fn shed_stage(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
) -> Option<(usize, f64)> {
    let next = active_stage + 1;
    if next >= stages.len() {
        return None;
    }
    let shed = stages[active_stage].dry_mass_kg as f64
        + propellant_remaining_kg
            .get(active_stage)
            .copied()
            .unwrap_or(0.0) as f64;
    Some((next, shed))
}

/// Total running-engine thrust (body frame) for the active stage at a throttle,
/// honoring per-engine ISP selection by ambient density. Only engines in
/// [`EngineState::Running`] contribute — every thrust consumer routes through
/// here so shutdown state is respected consistently everywhere.
pub fn stage_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    density_kg_m3: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state != EngineState::Running {
            continue;
        }
        let isp = selected_isp(engine.isp_sea_level, engine.isp_vacuum, density_kg_m3);
        let thrust = engine.max_thrust_kn as f64 * 1000.0 * throttle as f64;
        force += engine.thrust_axis.as_dvec3() * thrust;
        mass_flow += mass_flow_from_thrust(thrust, isp);
    }
    (force, mass_flow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::services::rocket_dynamics::RocketDynamicsState;

    #[test]
    fn thrust_follows_rocket_equation() {
        let mdot = 2_500.0;
        let isp = 311.0;
        let thrust = thrust_from_isp(mdot, isp);
        assert!((thrust - mdot * isp as f64 * STANDARD_GRAVITY_MPS2).abs() < 1e-6);
        // Inverse consistency.
        let back = mass_flow_from_thrust(thrust, isp);
        assert!((back - mdot).abs() < 1e-6);
    }

    #[test]
    fn mass_loss_matches_flow_times_time() {
        let (remaining, consumed) = consume_propellant(1_000.0, 250.0, 2.0);
        assert!((remaining - 500.0).abs() < 1e-6);
        assert!((consumed - 500.0).abs() < 1e-6);
        // Cannot consume beyond available propellant.
        let (remaining, consumed) = consume_propellant(100.0, 250.0, 2.0);
        assert_eq!(remaining, 0.0);
        assert!((consumed - 100.0).abs() < 1e-6);
    }

    #[test]
    fn staging_sheds_stage_mass() {
        let rocket = Rocket::falcon9();
        let mut propellant = rocket
            .stages
            .iter()
            .map(|s| s.propellant_mass_kg)
            .collect::<Vec<_>>();
        let before = active_vehicle_mass(&rocket.stages, &propellant, 0);
        let (next, shed) = shed_stage(&rocket.stages, &propellant, 0).unwrap();
        propellant[0] = 0.0; // simulate residual
        let (next_empty, shed_empty) = shed_stage(&rocket.stages, &propellant, 0).unwrap();
        let after = active_vehicle_mass(&rocket.stages, &propellant, next);
        assert_eq!(next, 1);
        assert!((shed - rocket.stages[0].total_mass_kg() as f64).abs() < 1e-6);
        assert!((shed_empty - rocket.stages[0].dry_mass_kg as f64).abs() < 1e-6);
        assert!((before - shed - after).abs() < 1e-6, "mass not conserved");
        // Final stage has nothing left to shed.
        assert!(shed_stage(&rocket.stages, &propellant, 1).is_none());
    }

    #[test]
    fn rocket_equation_delta_v_matches_integration() {
        // Single-stage analog: dry 22 200 kg, propellant 120 000 kg,
        // 7 607 kN, vacuum ISP 311 s.
        let dry = 22_200.0;
        let propellant = 120_000.0;
        let thrust_n = 7_607_000.0;
        let isp = 311.0;
        let g0 = STANDARD_GRAVITY_MPS2;

        let m0: f64 = dry + propellant;
        let m1: f64 = dry;
        let expected_dv = isp as f64 * g0 * (m0 / m1).ln();

        let mdot = mass_flow_from_thrust(thrust_n, isp);
        let burn_time = propellant as f64 / mdot;
        let dt = 0.01;
        let steps = (burn_time / dt).ceil() as u32;

        let mut mass = m0;
        let mut velocity = 0.0;
        for _ in 0..steps {
            velocity += (thrust_n / mass) * dt;
            mass = (mass - mdot * dt).max(dry);
        }

        assert!(
            (velocity - expected_dv).abs() < expected_dv * 0.01,
            "Δv {velocity} vs rocket equation {expected_dv}"
        );
    }

    /// Regression pin: pad thrust-to-weight of the stock Falcon 9 must stay at
    /// ≈ 5.45 so ascent guidance assumptions remain valid.
    #[test]
    fn falcon9_pad_thrust_to_weight_ratio() {
        let rocket = Rocket::falcon9();
        let thrust_n = rocket.max_thrust_kn() as f64 * 1000.0;
        let weight_n = rocket.total_mass_kg() as f64 * STANDARD_GRAVITY_MPS2;
        let tw_ratio = thrust_n / weight_n;
        assert!(
            (tw_ratio - 5.45).abs() < 0.05,
            "pad T/W {tw_ratio} drifted from 5.45"
        );
    }

    /// Regression pin: the two-stage Δv budget via the Tsiolkovsky equation
    /// (vacuum ISPs) stays near 10 km/s, and step integration reproduces the
    /// closed form (same harness as the single-stage analog above).
    #[test]
    fn falcon9_two_stage_delta_v_budget_matches_closed_form() {
        let rocket = Rocket::falcon9();
        let stage1 = &rocket.stages[0];
        let stage2 = &rocket.stages[1];
        let g0 = STANDARD_GRAVITY_MPS2;

        // Closed form, one Tsiolkovsky term per stage (vacuum ISP).
        let isp1 = stage1.engines[0].isp_vacuum as f64;
        let isp2 = stage2.engines[0].isp_vacuum as f64;
        let m_full = rocket.total_mass_kg() as f64;
        let m_after_stage1_burn = m_full - stage1.propellant_mass_kg as f64;
        let dv1 = isp1 * g0 * (m_full / m_after_stage1_burn).ln();
        let m_stage2_start = stage2.total_mass_kg() as f64;
        let dv2 = isp2 * g0 * (m_stage2_start / stage2.dry_mass_kg as f64).ln();
        let closed_form_dv = dv1 + dv2;
        assert!(
            (closed_form_dv - 10_000.0).abs() < 600.0,
            "two-stage budget {closed_form_dv} m/s is not ≈ 10 km/s"
        );

        // Step integration: burn stage 1, shed its dry structure, burn stage 2.
        let dt = 0.01;
        let mut mass = m_full;
        let mut velocity = 0.0_f64;
        for (stage, next_stage) in [(stage1, Some(stage2)), (stage2, None)] {
            let engine = &stage.engines[0];
            let thrust_n = stage
                .engines
                .iter()
                .map(|e| e.max_thrust_kn as f64 * 1000.0)
                .sum::<f64>();
            let mdot = mass_flow_from_thrust(thrust_n, engine.isp_vacuum);
            let burn_time = stage.propellant_mass_kg as f64 / mdot;
            let steps = (burn_time / dt).ceil() as u32;
            // Mass floor during this burn: everything still attached after
            // the propellant is gone (this stage dry + upper stages).
            let mass_floor = match next_stage {
                Some(next) => stage.dry_mass_kg as f64 + next.total_mass_kg() as f64,
                None => stage.dry_mass_kg as f64,
            };
            for _ in 0..steps {
                velocity += (thrust_n / mass) * dt;
                mass = (mass - mdot * dt).max(mass_floor);
            }
            // Separation drops this stage's dry structure before the next burn.
            mass -= stage.dry_mass_kg as f64;
        }
        assert!(
            (velocity - closed_form_dv).abs() < closed_form_dv * 0.01,
            "integrated Δv {velocity} vs closed form {closed_form_dv}"
        );
    }

    /// Staging bookkeeping invariant across the full shed sequence: at every
    /// step, burned + accumulated shed + remaining vehicle mass equals the
    /// initial total, with partial residual propellant at separation.
    #[test]
    fn staging_bookkeeping_conserves_total_across_full_sequence() {
        let rocket = Rocket::falcon9();
        let initial_total = rocket.total_mass_kg() as f64;
        let mut propellant = rocket
            .stages
            .iter()
            .map(|s| s.propellant_mass_kg)
            .collect::<Vec<_>>();

        // Burn partway into the active stage so a residual exists at
        // separation (realistic depletion).
        let mut burned_total = 0.0_f64;
        let (remaining, consumed) = consume_propellant(
            propellant[0],
            mass_flow_from_thrust(7_607_000.0, 282.0),
            30.0,
        );
        propellant[0] = remaining;
        burned_total += consumed as f64;

        let mut shed_total = 0.0_f64;
        let mut active_stage = 0_usize;
        while let Some((next, shed)) = shed_stage(&rocket.stages, &propellant, active_stage) {
            shed_total += shed;
            active_stage = next;
            let vehicle_mass = active_vehicle_mass(&rocket.stages, &propellant, active_stage);
            assert!(
                (burned_total + shed_total + vehicle_mass - initial_total).abs() < 1e-6,
                "mass not conserved after shedding up to stage {active_stage}"
            );
        }
        // The full sequence must end on the last stage with nothing left.
        assert_eq!(active_stage, rocket.stages.len() - 1);
        assert!(shed_stage(&rocket.stages, &propellant, active_stage).is_none());
    }

    #[test]
    fn isp_selection_blends_with_density() {
        assert_eq!(selected_isp(282.0, 311.0, 1.225), 282.0); // sea level
        assert_eq!(selected_isp(282.0, 311.0, 0.0), 311.0); // vacuum
        let mid = selected_isp(282.0, 311.0, 0.6);
        assert!(mid > 282.0 && mid < 311.0);
    }

    #[test]
    fn stage_thrust_ignores_shutdown_engines() {
        use crate::domain::entities::rocket::EngineState;
        let running = RocketEngine {
            position_m: bevy::math::Vec3::ZERO,
            thrust_axis: bevy::math::Vec3::Y,
            isp_sea_level: 282.0,
            isp_vacuum: 311.0,
            gimbal_range_deg: 5.0,
            max_thrust_kn: 1000.0,
            state: EngineState::Running,
        };
        let mut shutdown = running.clone();
        shutdown.state = EngineState::Off;

        let (thrust_on, flow_on) = stage_thrust_body(&[running.clone(), shutdown], 1.0, 0.0);
        assert!(
            (thrust_on.y - 1_000_000.0).abs() < 1e-6,
            "only Running engines thrust"
        );
        assert_eq!(thrust_on.x, 0.0);

        let mut all_off = running;
        all_off.state = EngineState::Off;
        let (thrust_off, flow_off) = stage_thrust_body(&[all_off], 1.0, 0.0);
        assert_eq!(thrust_off, DVec3::ZERO);
        assert_eq!(flow_off, 0.0);
    }

    #[test]
    fn separation_applies_impulses_along_axis() {
        let velocity = DVec3::new(100.0, 2_000.0, 0.0);
        // Body +Y axis tilted 45° about Z.
        let orientation = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4);
        let outcome = separation_impulse(
            velocity,
            orientation,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
        );
        let axis_world = (orientation * DVec3::Y).normalize();
        let expected_upper = velocity + axis_world * SEPARATION_UPPER_DV_MPS;
        let expected_spent = velocity - axis_world * SPENT_STAGE_RETRO_DV_MPS;
        assert!((outcome.upper_velocity_mps - expected_upper).length() < 1e-12);
        assert!((outcome.spent_velocity_mps - expected_spent).length() < 1e-12);
        // The bodies move apart: relative velocity along the axis is positive.
        let relative = outcome.upper_velocity_mps - outcome.spent_velocity_mps;
        assert!(relative.dot(axis_world) > 0.0);
    }

    #[test]
    fn separation_with_zero_impulses_is_a_no_op() {
        let velocity = DVec3::new(1.0, 2.0, 3.0);
        let outcome = separation_impulse(velocity, DQuat::IDENTITY, DVec3::Y, 0.0, 0.0);
        assert_eq!(outcome.upper_velocity_mps, velocity);
        assert_eq!(outcome.spent_velocity_mps, velocity);
    }

    #[test]
    fn retro_dv_can_be_disabled_independently() {
        let velocity = DVec3::ZERO;
        let outcome = separation_impulse(velocity, DQuat::IDENTITY, DVec3::Y, 1.5, 0.0);
        assert_eq!(outcome.spent_velocity_mps, velocity);
        assert_eq!(outcome.upper_velocity_mps, DVec3::new(0.0, 1.5, 0.0));
    }

    #[test]
    fn ullage_gate_blocks_ignition_until_settled() {
        // Configured ullage: blocked before the settle time, allowed after.
        assert!(!ignition_allowed_during_ullage(
            0.0,
            DEFAULT_ULLAGE_SETTLE_TIME_S
        ));
        assert!(!ignition_allowed_during_ullage(
            DEFAULT_ULLAGE_SETTLE_TIME_S - 0.1,
            DEFAULT_ULLAGE_SETTLE_TIME_S
        ));
        assert!(ignition_allowed_during_ullage(
            DEFAULT_ULLAGE_SETTLE_TIME_S,
            DEFAULT_ULLAGE_SETTLE_TIME_S
        ));
        // Disabled (settle time zero): always allowed.
        assert!(ignition_allowed_during_ullage(0.0, 0.0));
    }

    #[test]
    fn gimbal_torque_direction_and_clamping() {
        // Engine offset toward -Y, thrust +Y → torque about a transverse axis.
        let engine = DVec3::new(0.0, -30.0, 0.0);
        let com = DVec3::ZERO;
        let torque = gimbal_torque_body(engine, com, DVec3::Y, 1_000_000.0, 0.0, 0.0);
        // r = (0,-30,0), F = (0,F,0) → r × F = (0,0, ...) = (-30*F in x? )
        // (0,-30,0) × (0,1e6,0) = ((-30*0 - 0*1e6), (0*0 - 0*0), (0*1e6 - (-30)*0)) = (0,0,0)?
        // Along the same axis offset produces zero torque; use a transverse offset.
        let engine = DVec3::new(1.2, -30.0, 0.0);
        let torque = gimbal_torque_body(engine, com, DVec3::Y, 1_000_000.0, 0.0, 0.0);
        // (1.2, -30, 0) × (0, 1e6, 0) = ((-30*0 - 0*1e6), (0*0 - 1.2*0), (1.2*1e6 - (-30)*0)) = (0, 0, 1.2e6)
        assert!(torque.z.abs() > 0.0, "offset thrust must produce torque");
        assert!(torque.x.abs() < 1e-6 && torque.y.abs() < 1e-6);

        // Deflection is clamped to the engine gimbal range.
        let clamped = clamp_gimbal(1.0, 5.0); // 1 rad ≫ 5°
        assert!((clamped - 5.0_f32.to_radians()).abs() < 1e-9);
        let clamped_neg = clamp_gimbal(-1.0, 5.0);
        assert!((clamped_neg - -5.0_f32.to_radians()).abs() < 1e-9);
    }

    #[test]
    fn gimbal_allocation_inverts_real_torque_coupling() {
        let rocket = Rocket::falcon9();
        let engines = &rocket.stages[0].engines;
        // First-stage COM sits below the mid-length (engines below COM).
        let com = DVec3::new(0.0, -20.0, 0.0);
        let torque_cmd = DVec3::new(5.0e6, 0.0, -3.0e6);

        let (pitch, yaw) = allocate_gimbal_deflections(engines, com, torque_cmd, 1.0);
        // Positive X torque needs a negative pitch (engines below COM).
        assert!(pitch < 0.0);
        assert!(yaw > 0.0);

        // The real torque from the allocated deflections reproduces the command.
        let mut produced = DVec3::ZERO;
        for engine in engines {
            produced += gimbal_torque_body(
                engine.position_m.as_dvec3(),
                com,
                engine.thrust_axis.as_dvec3(),
                engine.max_thrust_kn as f64 * 1000.0,
                pitch as f64,
                yaw as f64,
            );
        }
        assert!(
            (produced - torque_cmd).length() < torque_cmd.length() * 5e-3,
            "allocation did not reproduce the torque: {produced}"
        );

        // No engines → no deflections.
        assert_eq!(
            allocate_gimbal_deflections(&[], com, torque_cmd, 1.0),
            (0.0, 0.0)
        );
    }

    #[test]
    fn consumption_updates_dynamics_mass() {
        let rocket = Rocket::falcon9();
        let (inertia, com) =
            active_vehicle_inertia(&rocket.stages, &[90_000.0, 30_000.0], 0, 1.85, 70.0);
        let mut state = RocketDynamicsState::new(
            DVec3::new(6_371_000.0, 0.0, 0.0),
            DVec3::ZERO,
            DQuat::IDENTITY,
            rocket.total_mass_kg() as f64,
            inertia,
            com,
        );
        assert!((state.mass_kg - 142_200.0).abs() < 1.0);
        // After burning ~5 s at a high mass flow the mass drops accordingly.
        let (_, consumed) = consume_propellant(90_000.0, 2_500.0, 5.0);
        state.mass_kg -= consumed as f64;
        assert!((state.mass_kg - (142_200.0 - 12_500.0)).abs() < 1.0);
    }
}
