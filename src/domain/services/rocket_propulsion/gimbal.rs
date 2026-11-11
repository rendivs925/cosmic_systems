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
    trim_static_torque: bool,
) -> (f32, f32) {
    allocate_gimbal_deflections_at_stage_origin(
        engines,
        DVec3::ZERO,
        center_of_mass_m,
        torque_cmd,
        thrust_scale,
        ambient_pressure_pa,
        trim_static_torque,
    )
}

/// Allocate gimbal commands using stage-local engine stations translated into
/// the current attached-stack frame. `trim_static_torque` inverts the affine
/// torque map around the zero-deflection operating point, which a selected
/// subset needs because its thrust line misses the center of mass. A full
/// symmetric stage passes `false` and keeps the original inversion bit-for-bit.
pub fn allocate_gimbal_deflections_at_stage_origin(
    engines: &[RocketEngine],
    stage_origin_in_stack_m: DVec3,
    center_of_mass_m: DVec3,
    torque_cmd: DVec3,
    thrust_scale: f32,
    ambient_pressure_pa: f64,
    trim_static_torque: bool,
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

    let t_zero = torque_for(0.0, 0.0);
    let t_pitch_absolute = torque_for(TEST_DEFLECTION_RAD, 0.0);
    let t_yaw_absolute = torque_for(0.0, TEST_DEFLECTION_RAD);
    // The original full-stage inversion used the absolute torque at a test
    // deflection. Preserve it exactly; only the selected-subset path uses the
    // affine slope around the nonzero zero-deflection operating point.
    let t_pitch = t_pitch_absolute - t_zero;
    let t_yaw = t_yaw_absolute - t_zero;

    // A symmetric full stage has no zero-deflection torque, and the original
    // independent-axis inversion is exact enough. A selected subset (for
    // example one ring engine on a landing burn) has a large static torque
    // because the thrust line misses the center of mass; invert the affine map
    // around that operating point instead of assuming it passes through zero.
    if trim_static_torque && t_zero.length() > 1.0e-6 {
        let (a, b) = (t_pitch.x, t_yaw.x);
        let (c, d) = (t_pitch.z, t_yaw.z);
        let determinant = a * d - b * c;
        if determinant.abs() > 1.0e-9 {
            let residual_x = torque_cmd.x - t_zero.x;
            let residual_z = torque_cmd.z - t_zero.z;
            let pitch_cmd = (residual_x * d - b * residual_z) / determinant * TEST_DEFLECTION_RAD;
            let yaw_cmd = (a * residual_z - residual_x * c) / determinant * TEST_DEFLECTION_RAD;
            return (pitch_cmd as f32, yaw_cmd as f32);
        }
        return (0.0, 0.0);
    }

    let pitch_cmd = if t_pitch_absolute.x.abs() > 1e-6 {
        (torque_cmd.x / t_pitch_absolute.x * TEST_DEFLECTION_RAD) as f32
    } else {
        0.0
    };
    let yaw_cmd = if t_yaw_absolute.z.abs() > 1e-6 {
        (torque_cmd.z / t_yaw_absolute.z * TEST_DEFLECTION_RAD) as f32
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::ThrustReference;
    use bevy::math::Vec3;

    fn ring_engine() -> RocketEngine {
        RocketEngine {
            position_m: Vec3::new(0.2, -20.6, 0.0),
            thrust_axis: Vec3::Y,
            isp_sea_level: 282.0,
            isp_vacuum: 311.0,
            gimbal_range_deg: 5.0,
            rated_thrust_kn: 845.222,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: 0.0,
            throttle_max: 1.0,
            max_ignitions: 5,
            ignition_count: 1,
            state: EngineState::Running,
        }
    }

    /// A selected off-axis engine has a static torque at zero deflection. The
    /// allocator must trim it rather than assume the thrust line passes through
    /// the center of mass.
    #[test]
    fn off_axis_engine_trims_its_static_torque() {
        let engine = ring_engine();
        let (pitch, yaw) = allocate_gimbal_deflections(
            std::slice::from_ref(&engine),
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            0.0,
            true,
        );
        assert!(
            pitch.abs() > 1.0e-6 || yaw.abs() > 1.0e-6,
            "the allocator must find a nonzero trim, got {pitch}, {yaw}"
        );
        let produced = stage_gimbal_torque_body(
            std::slice::from_ref(&engine),
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            0.0,
            f64::from(pitch),
            f64::from(yaw),
        );
        assert!(
            produced.length() < 10.0,
            "trim must cancel the static torque, produced {produced:?}"
        );
    }

    #[test]
    fn off_axis_engine_tracks_a_commanded_pitch_torque() {
        let engine = ring_engine();
        let commanded = DVec3::new(1_000.0, 0.0, 0.0);
        let (pitch, yaw) = allocate_gimbal_deflections(
            std::slice::from_ref(&engine),
            DVec3::ZERO,
            commanded,
            1.0,
            0.0,
            true,
        );
        let produced = stage_gimbal_torque_body(
            std::slice::from_ref(&engine),
            DVec3::ZERO,
            DVec3::ZERO,
            1.0,
            0.0,
            f64::from(pitch),
            f64::from(yaw),
        );
        assert!(
            (produced.x - commanded.x).abs() < 1.0,
            "commanded pitch torque not produced: {produced:?}"
        );
        assert!(
            produced.z.abs() < 10.0,
            "pitch command leaked into yaw: {produced:?}"
        );
    }
}
