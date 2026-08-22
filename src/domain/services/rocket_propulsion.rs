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
