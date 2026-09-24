//! Thrust-vector gimbal geometry: deflected thrust direction, torque, and
//! command allocation.

use super::engine::{engine_thrust_n, EngineOperatingPoint};
use crate::domain::entities::rocket::{EngineState, RocketEngine};
use crate::domain::math::{DQuat, DVec3};

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
    let deflected =
        gimbaled_thrust_direction_body(thrust_dir_body, gimbal_pitch_rad, gimbal_yaw_rad);
    let offset = engine_position_m - center_of_mass_m;
    offset.cross(deflected * thrust_n)
}

/// Gimbal torque for engines mounted on one stage of an attached stack.
/// `stage_origin_in_stack_m` converts the stage-local stations declared by the
/// catalog into the current assembly frame. It is zero for a detached stage.
pub fn stage_gimbal_torque_body(
    engines: &[RocketEngine],
    stage_origin_in_stack_m: DVec3,
    center_of_mass_m: DVec3,
    throttle: f32,
    ambient_pressure_pa: f64,
    gimbal_pitch_rad: f64,
    gimbal_yaw_rad: f64,
) -> DVec3 {
    engines
        .iter()
        .filter(|engine| engine.state == EngineState::Running)
        .map(|engine| {
            let operating_point =
                EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
            gimbal_torque_body(
                stage_origin_in_stack_m + engine.position_m.as_dvec3(),
                center_of_mass_m,
                engine.thrust_axis.as_dvec3(),
                operating_point.thrust_n,
                clamp_gimbal(gimbal_pitch_rad as f32, engine.gimbal_range_deg) as f64,
                clamp_gimbal(gimbal_yaw_rad as f32, engine.gimbal_range_deg) as f64,
            )
        })
        .sum()
}

/// The physical thrust axis after pitch/yaw gimbal deflection, in body frame.
/// Force and torque consumers must use this same direction.
pub fn gimbaled_thrust_direction_body(
    thrust_dir_body: DVec3,
    gimbal_pitch_rad: f64,
    gimbal_yaw_rad: f64,
) -> DVec3 {
    (DQuat::from_rotation_x(gimbal_pitch_rad)
        * DQuat::from_rotation_z(gimbal_yaw_rad)
        * thrust_dir_body)
        .normalize_or_zero()
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
    ambient_pressure_pa: f64,
) -> (f32, f32) {
    allocate_gimbal_deflections_at_stage_origin(
        engines,
        DVec3::ZERO,
        center_of_mass_m,
        torque_cmd,
        thrust_scale,
        ambient_pressure_pa,
    )
}

/// Allocate gimbal commands using stage-local engine stations translated into
/// the current attached-stack frame.
pub fn allocate_gimbal_deflections_at_stage_origin(
    engines: &[RocketEngine],
    stage_origin_in_stack_m: DVec3,
    center_of_mass_m: DVec3,
    torque_cmd: DVec3,
    thrust_scale: f32,
    ambient_pressure_pa: f64,
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
                stage_origin_in_stack_m + engine.position_m.as_dvec3(),
                center_of_mass_m,
                engine.thrust_axis.as_dvec3(),
                engine_thrust_n(engine, scale as f32, ambient_pressure_pa),
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

/// Running-engine thrust with the actual shared gimbal deflection applied.
pub fn stage_gimbaled_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    ambient_pressure_pa: f64,
    gimbal_pitch_rad: f64,
    gimbal_yaw_rad: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state != EngineState::Running {
            continue;
        }
        let point = EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
        force += gimbaled_thrust_direction_body(
            engine.thrust_axis.as_dvec3(),
            clamp_gimbal(gimbal_pitch_rad as f32, engine.gimbal_range_deg) as f64,
            clamp_gimbal(gimbal_yaw_rad as f32, engine.gimbal_range_deg) as f64,
        ) * point.thrust_n;
        mass_flow += point.mass_flow_kg_s;
    }
    (force, mass_flow)
}
