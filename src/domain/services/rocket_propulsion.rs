//! Physically consistent rocket propulsion.
//!
//! Thrust follows `T = m_dot · Isp · g0`; mass flow is `m_dot = T / (Isp · g0)`.
//! This module is the single authority for the mass-flow formula (delegated to
//! by [`Rocket::mass_flow_rate_kg_s`]), propellant consumption, staging, ISP
//! selection, and gimbal torque. Systems feed thrust and torque into the 6-DOF
//! accumulator and never write the transform directly (AGENTS.md section 17).

use crate::domain::entities::rocket::{
    EngineState, ParallelBoosters, RocketEngine, RocketStage, ThrustReference,
};
use crate::domain::math::{DMat3, DQuat, DVec3};
use crate::domain::services::atmosphere::SEA_LEVEL_PRESSURE_PA;
use crate::domain::services::rocket_dynamics::rocket_inertia_tensor_with_mass_adjustments;

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

/// Radial relative velocity given to each detached parallel booster, m/s.
/// Symmetric pairs have zero net linear impulse on the surviving core.
pub const PARALLEL_BOOSTER_SEPARATION_DV_MPS: f64 = 2.0;

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

/// Apply a stage-separation impulse: a prescribed Δv to the upper stage along
/// `separation_axis_body` (unit vector, body frame) and an optional retro-Δv
/// to the spent stage along the opposite direction.
///
/// The pusher response on the spent stage is mass-weighted so that the pusher
/// itself conserves linear momentum. The optional retro motor remains a
/// prescribed external Δv because this model does not represent its expelled
/// reaction mass.
pub fn separation_impulse(
    shared_velocity_mps: DVec3,
    orientation: DQuat,
    separation_axis_body: DVec3,
    upper_mass_kg: f64,
    spent_mass_kg: f64,
    upper_dv_mps: f64,
    spent_retro_dv_mps: f64,
) -> SeparationOutcome {
    let axis_world = (orientation * separation_axis_body).normalize_or_zero();
    let spent_pusher_dv_mps = if spent_mass_kg > 0.0 {
        upper_dv_mps * upper_mass_kg.max(0.0) / spent_mass_kg
    } else {
        0.0
    };
    SeparationOutcome {
        upper_velocity_mps: shared_velocity_mps + axis_world * upper_dv_mps,
        spent_velocity_mps: shared_velocity_mps
            - axis_world * (spent_pusher_dv_mps + spent_retro_dv_mps),
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

/// Select the effective specific impulse from ambient pressure, linearly
/// interpolating the configured sea-level and vacuum endpoints. With a fixed
/// nozzle mass flow, the pressure-thrust term is linear in back pressure.
pub fn selected_isp(isp_sea_level: f32, isp_vacuum: f32, ambient_pressure_pa: f64) -> f32 {
    let t = (1.0 - (ambient_pressure_pa / SEA_LEVEL_PRESSURE_PA).clamp(0.0, 1.0)) as f32;
    isp_sea_level + (isp_vacuum - isp_sea_level) * t
}

/// One engine's coherent operating point at a fixed throttle and back pressure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EngineOperatingPoint {
    pub specific_impulse_s: f32,
    pub mass_flow_kg_s: f64,
    pub thrust_n: f64,
}

impl EngineOperatingPoint {
    pub fn from_engine(engine: &RocketEngine, throttle: f32, ambient_pressure_pa: f64) -> Self {
        let rated_thrust_n =
            engine.rated_thrust_kn as f64 * 1000.0 * throttle.clamp(0.0, 1.0) as f64;
        let rated_isp_s = match engine.thrust_reference {
            ThrustReference::SeaLevel => engine.isp_sea_level,
            ThrustReference::Vacuum => engine.isp_vacuum,
        };
        let mass_flow_kg_s = mass_flow_from_thrust(rated_thrust_n, rated_isp_s);
        let specific_impulse_s =
            selected_isp(engine.isp_sea_level, engine.isp_vacuum, ambient_pressure_pa);
        Self {
            specific_impulse_s,
            mass_flow_kg_s,
            thrust_n: thrust_from_isp(mass_flow_kg_s, specific_impulse_s),
        }
    }
}

/// Full-throttle thrust at an ambient pressure. Fixed mass flow is derived from
/// the engine's declared rated-thrust endpoint; pressure-selected Isp then
/// determines force at every other ambient pressure.
pub fn engine_thrust_n(engine: &RocketEngine, throttle: f32, ambient_pressure_pa: f64) -> f64 {
    EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa).thrust_n
}

/// Result of one bounded propellant-consumption step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropellantConsumption {
    /// Propellant left after the step, kg.
    pub remaining_kg: f32,
    /// Propellant actually consumed during the step, kg.
    pub consumed_kg: f32,
}

/// Consume propellant at the given mass flow for `dt` seconds. Consumption
/// never exceeds the available propellant.
pub fn consume_propellant(
    propellant_kg: f32,
    mass_flow_kg_s: f64,
    dt: f64,
) -> PropellantConsumption {
    let remaining_kg = (propellant_kg - (mass_flow_kg_s * dt) as f32).max(0.0);
    PropellantConsumption {
        remaining_kg,
        consumed_kg: propellant_kg - remaining_kg,
    }
}

/// Duration for which a stage can actually produce thrust during one fixed
/// step. A nearly empty tank cannot deliver a full-step impulse.
pub fn burn_duration_s(propellant_kg: f32, mass_flow_kg_s: f64, dt: f64) -> f64 {
    if propellant_kg <= 0.0 || mass_flow_kg_s <= 0.0 || dt <= 0.0 {
        return 0.0;
    }
    (propellant_kg as f64 / mass_flow_kg_s).min(dt)
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

/// Clamp a commanded throttle to a per-engine throttle range and then to the
/// physical 0..1 band. A zero (or negative) command means "engine off" and
/// stays zero — otherwise an engine with a positive minimum throttle could
/// never be shut down.
pub fn clamp_throttle_range(cmd: f32, throttle_min: f32, throttle_max: f32) -> f32 {
    if cmd <= 0.0 {
        return 0.0;
    }
    cmd.clamp(
        throttle_min.clamp(0.0, 1.0),
        throttle_max.clamp(throttle_min.min(1.0), 1.0),
    )
}

/// Commanded-throttle envelope shared by every engine in a stage: the command
/// must be valid for each engine individually, so the stage envelope is the
/// intersection (highest lower bound, lowest upper bound).
pub fn stage_throttle_envelope(engines: &[RocketEngine]) -> (f32, f32) {
    let Some(first) = engines.first() else {
        return (0.0, 1.0);
    };
    (
        engines
            .iter()
            .map(|e| e.throttle_min)
            .fold(first.throttle_min, f32::max),
        engines
            .iter()
            .map(|e| e.throttle_max)
            .fold(first.throttle_max, f32::min),
    )
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

/// Vehicle mass including any attached payload hardware (fairing): one
/// authority so consumption/staging/jettison can never disagree about what
/// the vehicle currently weighs.
pub fn active_vehicle_mass_with_payload(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
    attached_payload_kg: f32,
) -> f64 {
    active_vehicle_mass(stages, propellant_remaining_kg, active_stage) + attached_payload_kg as f64
}

/// Active serial-stack mass plus optional attached parallel boosters. Passing
/// `None` retains the serial-only result exactly.
pub fn active_vehicle_mass_with_payload_and_boosters(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
    attached_payload_kg: f32,
    boosters: Option<&ParallelBoosters>,
    booster_propellant_remaining_kg: &[f32],
) -> f64 {
    active_vehicle_mass_with_payload(
        stages,
        propellant_remaining_kg,
        active_stage,
        attached_payload_kg,
    ) + boosters.map_or(0.0, |boosters| {
        booster_propellant_remaining_kg
            .iter()
            .take(boosters.count())
            .map(|propellant_kg| {
                boosters.stage.dry_mass_kg as f64 + (*propellant_kg).max(0.0) as f64
            })
            .sum::<f64>()
    })
}

/// Inertia tensor and center of mass for the active vehicle, using the shared
/// geometric rocket model with active stages, attached payload, and accumulated
/// ablation mass loss. Updates as the attached mass inventory changes.
pub fn active_vehicle_inertia(
    stages: &[RocketStage],
    propellant_remaining_kg: &[f32],
    active_stage: usize,
    attached_payload_kg: f32,
    ablation_mass_loss_kg: f64,
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
    rocket_inertia_tensor_with_mass_adjustments(
        dry,
        propellant,
        attached_payload_kg as f64,
        ablation_mass_loss_kg,
        radius_m,
        height_m,
    )
}

/// Inputs for calculating the rigid-body properties of the currently attached
/// vehicle. The inventory, geometry, and optional boosters are evaluated as
/// one assembly so its mass, inertia, and center of mass cannot diverge.
pub struct ActiveVehicleMassPropertiesInput<'a> {
    pub stages: &'a [RocketStage],
    pub propellant_remaining_kg: &'a [f32],
    pub active_stage: usize,
    pub attached_payload_kg: f32,
    pub ablation_mass_loss_kg: f64,
    pub radius_m: f64,
    pub height_m: f64,
    pub boosters: Option<&'a ParallelBoosters>,
    pub booster_propellant_remaining_kg: &'a [f32],
}

/// Rigid-body properties derived from one attached vehicle inventory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActiveVehicleMassProperties {
    pub mass_kg: f64,
    pub inertia_body: DMat3,
    pub center_of_mass_m: DVec3,
}

impl ActiveVehicleMassPropertiesInput<'_> {
    /// Calculate the active stack from the established cylinder approximation.
    /// Each attached booster contributes stage-local properties translated from
    /// its declared full-stack attachment origin via the parallel-axis theorem.
    pub fn calculate(self) -> ActiveVehicleMassProperties {
        let (serial_inertia, serial_com) = active_vehicle_inertia(
            self.stages,
            self.propellant_remaining_kg,
            self.active_stage,
            self.attached_payload_kg,
            self.ablation_mass_loss_kg,
            self.radius_m,
            self.height_m,
        );
        let serial_mass_kg = (active_vehicle_mass_with_payload(
            self.stages,
            self.propellant_remaining_kg,
            self.active_stage,
            self.attached_payload_kg,
        ) - self.ablation_mass_loss_kg.max(0.0))
        .max(1.0);
        let total_mass_kg = (active_vehicle_mass_with_payload_and_boosters(
            self.stages,
            self.propellant_remaining_kg,
            self.active_stage,
            self.attached_payload_kg,
            self.boosters,
            self.booster_propellant_remaining_kg,
        ) - self.ablation_mass_loss_kg.max(0.0))
        .max(1.0);
        let Some(boosters) = self.boosters else {
            return ActiveVehicleMassProperties {
                mass_kg: total_mass_kg,
                inertia_body: serial_inertia,
                center_of_mass_m: serial_com,
            };
        };

        let mut weighted_center_m = serial_com * serial_mass_kg;
        for (attachment_m, propellant_kg) in boosters
            .attachment_positions()
            .iter()
            .zip(self.booster_propellant_remaining_kg.iter())
        {
            let properties = stage_mass_properties(&boosters.stage, *propellant_kg, 0.0, 0.0);
            let center_m = attachment_m.as_dvec3() + properties.center_of_mass_m;
            weighted_center_m += center_m * properties.mass_kg;
        }
        let center_of_mass_m = weighted_center_m / total_mass_kg;
        let parallel_axis = |mass_kg: f64, offset_m: DVec3| {
            let squared_distance_m2 = offset_m.length_squared();
            mass_kg
                * (DMat3::from_diagonal(DVec3::splat(squared_distance_m2))
                    - DMat3::from_cols(
                        offset_m * offset_m.x,
                        offset_m * offset_m.y,
                        offset_m * offset_m.z,
                    ))
        };
        let mut inertia =
            serial_inertia + parallel_axis(serial_mass_kg, serial_com - center_of_mass_m);
        for (attachment_m, propellant_kg) in boosters
            .attachment_positions()
            .iter()
            .zip(self.booster_propellant_remaining_kg.iter())
        {
            let properties = stage_mass_properties(&boosters.stage, *propellant_kg, 0.0, 0.0);
            let center_m = attachment_m.as_dvec3() + properties.center_of_mass_m;
            inertia += properties.inertia_body
                + parallel_axis(properties.mass_kg, center_m - center_of_mass_m);
        }
        ActiveVehicleMassProperties {
            mass_kg: total_mass_kg,
            inertia_body: inertia,
            center_of_mass_m,
        }
    }
}

/// Mass properties for one separated physical stage. Geometry is deliberately
/// taken from the stage itself so a detached body never inherits the full-stack
/// inertia or center of mass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageMassProperties {
    pub mass_kg: f64,
    pub inertia_body: DMat3,
    pub center_of_mass_m: DVec3,
    pub height_m: f64,
}

/// Rebuild one stage's rigid-body properties from its own dry mass, remaining
/// propellant, attached payload, and ablated mass.
pub fn stage_mass_properties(
    stage: &RocketStage,
    propellant_remaining_kg: f32,
    attached_payload_kg: f32,
    ablation_mass_loss_kg: f64,
) -> StageMassProperties {
    let dry_mass_kg = (stage.dry_mass_kg as f64 - ablation_mass_loss_kg.max(0.0)).max(0.0);
    let propellant_mass_kg = propellant_remaining_kg.max(0.0) as f64;
    let attached_payload_kg = attached_payload_kg.max(0.0) as f64;
    let radius_m = stage.diameter_m as f64 * 0.5;
    let height_m = stage.height_m as f64;
    let (inertia_body, center_of_mass_m) = rocket_inertia_tensor_with_mass_adjustments(
        dry_mass_kg,
        propellant_mass_kg,
        attached_payload_kg,
        0.0,
        radius_m,
        height_m,
    );
    StageMassProperties {
        mass_kg: dry_mass_kg + propellant_mass_kg + attached_payload_kg,
        inertia_body,
        center_of_mass_m,
        height_m,
    }
}

/// The independent f64 states created by a stage separation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSeparationDynamics {
    pub upper: crate::domain::services::rocket_dynamics::RocketDynamicsState,
    pub spent: crate::domain::services::rocket_dynamics::RocketDynamicsState,
}

/// Independent f64 dynamics for simultaneously jettisoned parallel boosters.
/// Each attachment is a stage geometric origin in the parent stack frame.
pub fn separate_parallel_boosters_dynamics(
    pre_separation: crate::domain::services::rocket_dynamics::RocketDynamicsState,
    boosters: &ParallelBoosters,
    booster_propellant_remaining_kg: &[f32],
    separation_dv_mps: f64,
) -> Vec<crate::domain::services::rocket_dynamics::RocketDynamicsState> {
    let angular_velocity_world_radps =
        pre_separation.orientation * pre_separation.angular_velocity_radps;
    boosters
        .attachment_positions()
        .iter()
        .zip(booster_propellant_remaining_kg.iter())
        .map(|(attachment_m, propellant_kg)| {
            let properties = stage_mass_properties(&boosters.stage, *propellant_kg, 0.0, 0.0);
            let attachment_world_m = pre_separation.orientation * attachment_m.as_dvec3();
            let radial_body =
                DVec3::new(attachment_m.x as f64, 0.0, attachment_m.z as f64).normalize_or_zero();
            let mut dynamics = pre_separation;
            dynamics.position_m = pre_separation.position_m + attachment_world_m;
            dynamics.velocity_mps = pre_separation.velocity_mps
                + angular_velocity_world_radps.cross(attachment_world_m)
                + (pre_separation.orientation * radial_body) * separation_dv_mps;
            dynamics.angular_acceleration_radps2 = DVec3::ZERO;
            dynamics.mass_kg = properties.mass_kg;
            dynamics.inertia_body = properties.inertia_body;
            dynamics.center_of_mass_m = properties.center_of_mass_m;
            dynamics
        })
        .collect()
}

/// Split a previously rigid launch stack into two non-overlapping bodies.
///
/// Positions in [`RocketDynamicsState`] are body origins. The offset therefore
/// accounts for each stage-local center of mass while keeping the pre-separation
/// composite center of mass fixed. Both stage centers inherit the pre-separation
/// rigid-body velocity before receiving the axial separation impulse.
pub fn separate_stage_dynamics(
    pre_separation: crate::domain::services::rocket_dynamics::RocketDynamicsState,
    upper_properties: StageMassProperties,
    spent_properties: StageMassProperties,
    separation_axis_body: DVec3,
    upper_dv_mps: f64,
    spent_retro_dv_mps: f64,
    minimum_clearance_m: f64,
) -> StageSeparationDynamics {
    let axis_world = (pre_separation.orientation * separation_axis_body).normalize_or_zero();
    let total_mass_kg = (upper_properties.mass_kg + spent_properties.mass_kg).max(1e-9);
    let center_spacing_m = (upper_properties.height_m + spent_properties.height_m) * 0.5
        + minimum_clearance_m.max(0.0);
    let upper_origin_offset_from_spent_m = axis_world * center_spacing_m;
    let upper_com_offset_from_spent_m = upper_origin_offset_from_spent_m
        + pre_separation.orientation
            * (upper_properties.center_of_mass_m - spent_properties.center_of_mass_m);

    let pre_center_of_mass_m =
        pre_separation.position_m + pre_separation.orientation * pre_separation.center_of_mass_m;
    let upper_center_of_mass_m = pre_center_of_mass_m
        + upper_com_offset_from_spent_m * spent_properties.mass_kg / total_mass_kg;
    let spent_center_of_mass_m = pre_center_of_mass_m
        - upper_com_offset_from_spent_m * upper_properties.mass_kg / total_mass_kg;

    let angular_velocity_world_radps =
        pre_separation.orientation * pre_separation.angular_velocity_radps;
    let pre_center_velocity_mps = pre_separation.velocity_mps
        + angular_velocity_world_radps
            .cross(pre_separation.orientation * pre_separation.center_of_mass_m);
    let impulses = separation_impulse(
        pre_center_velocity_mps,
        pre_separation.orientation,
        separation_axis_body,
        upper_properties.mass_kg,
        spent_properties.mass_kg,
        upper_dv_mps,
        spent_retro_dv_mps,
    );

    let mut upper = pre_separation;
    upper.position_m =
        upper_center_of_mass_m - pre_separation.orientation * upper_properties.center_of_mass_m;
    upper.velocity_mps = impulses.upper_velocity_mps
        - angular_velocity_world_radps
            .cross(pre_separation.orientation * upper_properties.center_of_mass_m);
    upper.angular_acceleration_radps2 = DVec3::ZERO;
    upper.mass_kg = upper_properties.mass_kg;
    upper.inertia_body = upper_properties.inertia_body;
    upper.center_of_mass_m = upper_properties.center_of_mass_m;

    let mut spent = pre_separation;
    spent.position_m =
        spent_center_of_mass_m - pre_separation.orientation * spent_properties.center_of_mass_m;
    spent.velocity_mps = impulses.spent_velocity_mps
        - angular_velocity_world_radps
            .cross(pre_separation.orientation * spent_properties.center_of_mass_m);
    spent.angular_acceleration_radps2 = DVec3::ZERO;
    spent.mass_kg = spent_properties.mass_kg;
    spent.inertia_body = spent_properties.inertia_body;
    spent.center_of_mass_m = spent_properties.center_of_mass_m;

    StageSeparationDynamics { upper, spent }
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
/// honoring per-engine ISP selection by ambient pressure. Only engines in
/// [`EngineState::Running`] contribute — every thrust consumer routes through
/// here so shutdown state is respected consistently everywhere.
pub fn stage_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    ambient_pressure_pa: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state != EngineState::Running {
            continue;
        }
        let point = EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
        force += engine.thrust_axis.as_dvec3() * point.thrust_n;
        mass_flow += point.mass_flow_kg_s;
    }
    (force, mass_flow)
}

/// Maximum thrust available to guidance before an engine is commanded to run.
/// Off engines with remaining ignition budget are startable; terminally
/// depleted engines are not. This is a planning capability only: physical
/// force, torque, and mass flow must continue to use [`stage_thrust_body`].
pub fn stage_available_thrust_body(
    engines: &[RocketEngine],
    throttle: f32,
    ambient_pressure_pa: f64,
) -> (DVec3, f64) {
    let throttle = throttle.clamp(0.0, 1.0);
    let mut force = DVec3::ZERO;
    let mut mass_flow = 0.0;
    for engine in engines {
        if engine.state == EngineState::Depleted {
            continue;
        }
        let point = EngineOperatingPoint::from_engine(engine, throttle, ambient_pressure_pa);
        force += engine.thrust_axis.as_dvec3() * point.thrust_n;
        mass_flow += point.mass_flow_kg_s;
    }
    (force, mass_flow)
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
    use crate::domain::entities::rocket::Rocket;
    use crate::domain::math::Vec3;
    use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};

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
        let consumption = consume_propellant(1_000.0, 250.0, 2.0);
        assert!((consumption.remaining_kg - 500.0).abs() < 1e-6);
        assert!((consumption.consumed_kg - 500.0).abs() < 1e-6);
        // Cannot consume beyond available propellant.
        let consumption = consume_propellant(100.0, 250.0, 2.0);
        assert_eq!(consumption.remaining_kg, 0.0);
        assert!((consumption.consumed_kg - 100.0).abs() < 1e-6);
    }

    #[test]
    fn final_partial_step_only_delivers_available_burn_impulse() {
        assert_eq!(burn_duration_s(0.0, 10.0, 1.0), 0.0);
        assert!((burn_duration_s(5.0, 10.0, 1.0) - 0.5).abs() < 1e-12);
        assert!((burn_duration_s(50.0, 10.0, 1.0) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn staging_sheds_stage_mass() {
        let rocket = Rocket::falcon9_test_fixture();
        let mut propellant = rocket
            .stages
            .iter()
            .map(|s| s.propellant_mass_kg)
            .collect::<Vec<_>>();
        let before = active_vehicle_mass(&rocket.stages, &propellant, 0);
        let (next, shed) = shed_stage(&rocket.stages, &propellant, 0).unwrap();
        propellant[0] = 0.0; // simulate residual
        let (_, shed_empty) = shed_stage(&rocket.stages, &propellant, 0).unwrap();
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
        let burn_time = propellant / mdot;
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
        let rocket = Rocket::falcon9_test_fixture();
        let thrust_n = stage_thrust_body(&rocket.stages[0].engines, 1.0, SEA_LEVEL_PRESSURE_PA)
            .0
            .length();
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
        let rocket = Rocket::falcon9_test_fixture();
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
            let (thrust_body, mdot) = stage_thrust_body(&stage.engines, 1.0, 0.0);
            let thrust_n = thrust_body.length();
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
        let rocket = Rocket::falcon9_test_fixture();
        let initial_total = rocket.total_mass_kg() as f64;
        let mut propellant = rocket
            .stages
            .iter()
            .map(|s| s.propellant_mass_kg)
            .collect::<Vec<_>>();

        // Burn partway into the active stage so a residual exists at
        // separation (realistic depletion).
        let mut burned_total = 0.0_f64;
        let consumption = consume_propellant(
            propellant[0],
            mass_flow_from_thrust(7_607_000.0, 282.0),
            30.0,
        );
        propellant[0] = consumption.remaining_kg;
        burned_total += consumption.consumed_kg as f64;

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
    fn isp_selection_blends_with_ambient_pressure() {
        assert_eq!(selected_isp(282.0, 311.0, SEA_LEVEL_PRESSURE_PA), 282.0);
        assert_eq!(selected_isp(282.0, 311.0, 0.0), 311.0); // vacuum
        let mid = selected_isp(282.0, 311.0, SEA_LEVEL_PRESSURE_PA * 0.5);
        assert!(mid > 282.0 && mid < 311.0);
    }

    #[test]
    fn rated_thrust_matches_declared_reference_with_fixed_mass_flow() {
        for (reference, reference_pressure_pa, other_pressure_pa) in [
            (ThrustReference::SeaLevel, SEA_LEVEL_PRESSURE_PA, 0.0),
            (ThrustReference::Vacuum, 0.0, SEA_LEVEL_PRESSURE_PA),
        ] {
            let mut engine = engine_with_throttle(0.0, 1.0);
            engine.thrust_reference = reference;
            let rated = EngineOperatingPoint::from_engine(&engine, 1.0, reference_pressure_pa);
            let intermediate =
                EngineOperatingPoint::from_engine(&engine, 1.0, SEA_LEVEL_PRESSURE_PA * 0.5);
            let other = EngineOperatingPoint::from_engine(&engine, 1.0, other_pressure_pa);

            assert!((rated.thrust_n - engine.rated_thrust_kn as f64 * 1000.0).abs() < 1e-6);
            assert!((intermediate.mass_flow_kg_s - rated.mass_flow_kg_s).abs() < 1e-12);
            assert!((other.mass_flow_kg_s - rated.mass_flow_kg_s).abs() < 1e-12);
            assert!(intermediate.specific_impulse_s > engine.isp_sea_level);
            assert!(intermediate.specific_impulse_s < engine.isp_vacuum);
        }
    }

    #[test]
    fn gimbaled_force_and_torque_share_the_same_deflected_axis() {
        let engine = RocketEngine {
            position_m: Vec3::new(0.0, -2.0, 0.0),
            gimbal_range_deg: 10.0,
            ..engine_with_throttle(0.0, 1.0)
        };
        let pitch = 0.1;
        let (force, _) =
            stage_gimbaled_thrust_body(std::slice::from_ref(&engine), 1.0, 0.0, pitch, 0.0);
        let direction = gimbaled_thrust_direction_body(DVec3::Y, pitch, 0.0);
        assert!(force.normalize().dot(direction) > 1.0 - 1e-12);
        let torque = gimbal_torque_body(
            engine.position_m.as_dvec3(),
            DVec3::ZERO,
            DVec3::Y,
            force.length(),
            pitch,
            0.0,
        );
        assert!(
            torque.length() > 0.0,
            "deflected thrust must produce torque"
        );
    }

    #[test]
    fn attached_stack_gimbal_translates_stage_local_engine_station() {
        let rocket = Rocket::falcon9_test_fixture();
        let stage_origin_in_stack_m =
            Rocket::stage_origin_in_stack_m(&rocket.stages, rocket.height_m, 0)
                .unwrap()
                .as_dvec3();
        let center_of_mass_m = DVec3::new(0.0, -8.0, 0.0);
        let engine = &rocket.stages[0].engines[0];
        let actual = stage_gimbal_torque_body(
            std::slice::from_ref(engine),
            stage_origin_in_stack_m,
            center_of_mass_m,
            1.0,
            0.0,
            clamp_gimbal(0.05, engine.gimbal_range_deg) as f64,
            0.0,
        );
        let expected = gimbal_torque_body(
            stage_origin_in_stack_m + engine.position_m.as_dvec3(),
            center_of_mass_m,
            engine.thrust_axis.as_dvec3(),
            engine_thrust_n(engine, 1.0, 0.0),
            clamp_gimbal(0.05, engine.gimbal_range_deg) as f64,
            0.0,
        );

        assert!((actual - expected).length() < 1e-9);
    }

    #[test]
    fn stage_thrust_ignores_shutdown_engines() {
        use crate::domain::entities::rocket::EngineState;
        let running = engine_with_throttle(0.0, 1.0);
        let mut shutdown = running.clone();
        shutdown.state = EngineState::Off;

        let (thrust_on, _) =
            stage_thrust_body(&[running.clone(), shutdown], 1.0, SEA_LEVEL_PRESSURE_PA);
        assert!(
            (thrust_on.y - 1_000_000.0).abs() < 1e-6,
            "only Running engines thrust"
        );
        assert_eq!(thrust_on.x, 0.0);

        let mut all_off = running;
        all_off.state = EngineState::Off;
        let (thrust_off, flow_off) = stage_thrust_body(&[all_off], 1.0, SEA_LEVEL_PRESSURE_PA);
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
            2_000.0,
            4_000.0,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
        );
        let axis_world = (orientation * DVec3::Y).normalize();
        let expected_upper = velocity + axis_world * SEPARATION_UPPER_DV_MPS;
        let expected_spent = velocity
            - axis_world * (SEPARATION_UPPER_DV_MPS * 2_000.0 / 4_000.0 + SPENT_STAGE_RETRO_DV_MPS);
        assert!((outcome.upper_velocity_mps - expected_upper).length() < 1e-12);
        assert!((outcome.spent_velocity_mps - expected_spent).length() < 1e-12);
        // The bodies move apart: relative velocity along the axis is positive.
        let relative = outcome.upper_velocity_mps - outcome.spent_velocity_mps;
        assert!(relative.dot(axis_world) > 0.0);
    }

    #[test]
    fn separation_with_zero_impulses_is_a_no_op() {
        let velocity = DVec3::new(1.0, 2.0, 3.0);
        let outcome = separation_impulse(
            velocity,
            DQuat::IDENTITY,
            DVec3::Y,
            1_000.0,
            2_000.0,
            0.0,
            0.0,
        );
        assert_eq!(outcome.upper_velocity_mps, velocity);
        assert_eq!(outcome.spent_velocity_mps, velocity);
    }

    #[test]
    fn retro_dv_can_be_disabled_independently() {
        let velocity = DVec3::ZERO;
        let outcome = separation_impulse(
            velocity,
            DQuat::IDENTITY,
            DVec3::Y,
            1_000.0,
            1_000.0,
            1.5,
            0.0,
        );
        assert_eq!(outcome.spent_velocity_mps, DVec3::new(0.0, -1.5, 0.0));
        assert_eq!(outcome.upper_velocity_mps, DVec3::new(0.0, 1.5, 0.0));
    }

    #[test]
    fn stage_separation_assigns_local_properties_and_clearance() {
        let rocket = Rocket::falcon9_test_fixture();
        let upper_properties = stage_mass_properties(&rocket.stages[1], 20_000.0, 1_000.0, 0.0);
        let spent_properties = stage_mass_properties(&rocket.stages[0], 15_000.0, 0.0, 0.0);
        assert_eq!(upper_properties.mass_kg, 25_200.0);
        assert_eq!(spent_properties.mass_kg, 33_000.0);
        let pre_mass_kg = upper_properties.mass_kg + spent_properties.mass_kg;
        let (pre_inertia, pre_com) = rocket_inertia_tensor(pre_mass_kg, 0.0, 1.85, 54.4);
        let orientation = DQuat::from_rotation_z(std::f64::consts::FRAC_PI_4);
        let pre = RocketDynamicsState::new(
            DVec3::new(6_500_000.0, 100.0, -50.0),
            DVec3::new(10.0, 2_000.0, -5.0),
            orientation,
            pre_mass_kg,
            pre_inertia,
            pre_com,
        );

        let separated = separate_stage_dynamics(
            pre,
            upper_properties,
            spent_properties,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            0.0,
            MIN_SEPARATION_CLEARANCE_M,
        );

        assert_eq!(separated.upper.mass_kg, upper_properties.mass_kg);
        assert_eq!(separated.upper.inertia_body, upper_properties.inertia_body);
        assert_eq!(
            separated.upper.center_of_mass_m,
            upper_properties.center_of_mass_m
        );
        assert_eq!(separated.spent.mass_kg, spent_properties.mass_kg);
        assert_eq!(separated.spent.inertia_body, spent_properties.inertia_body);
        assert_eq!(
            separated.spent.center_of_mass_m,
            spent_properties.center_of_mass_m
        );
        assert_ne!(separated.upper.inertia_body, separated.spent.inertia_body);
        assert_ne!(separated.upper.inertia_body, pre.inertia_body);
        assert_ne!(separated.spent.inertia_body, pre.inertia_body);
        assert_ne!(
            separated.upper.center_of_mass_m,
            separated.spent.center_of_mass_m
        );
        assert_ne!(separated.upper.center_of_mass_m, pre.center_of_mass_m);
        assert_ne!(separated.spent.center_of_mass_m, pre.center_of_mass_m);

        let axis_world = (orientation * DVec3::Y).normalize();
        let minimum_origin_distance_m = (upper_properties.height_m + spent_properties.height_m)
            * 0.5
            + MIN_SEPARATION_CLEARANCE_M;
        let origin_delta_m = separated.upper.position_m - separated.spent.position_m;
        assert!(
            (origin_delta_m - axis_world * minimum_origin_distance_m).length() < 1e-9,
            "stage origins overlap or are not separated on the body axis: {origin_delta_m}"
        );

        let world_com = |state: &RocketDynamicsState| {
            state.position_m + state.orientation * state.center_of_mass_m
        };
        let composite_com_m = (world_com(&separated.upper) * separated.upper.mass_kg
            + world_com(&separated.spent) * separated.spent.mass_kg)
            / pre_mass_kg;
        assert!(
            (composite_com_m - world_com(&pre)).length() < 1e-9,
            "separation moved the composite center of mass"
        );

        let pre_momentum = pre.velocity_mps * pre_mass_kg;
        let post_momentum = separated.upper.velocity_mps * separated.upper.mass_kg
            + separated.spent.velocity_mps * separated.spent.mass_kg;
        assert!(
            (post_momentum - pre_momentum).length() < 1e-8,
            "the pusher impulse must conserve linear momentum"
        );
    }

    #[test]
    fn stage_separation_is_deterministic() {
        let rocket = Rocket::falcon9_test_fixture();
        let upper_properties = stage_mass_properties(&rocket.stages[1], 30_000.0, 0.0, 0.0);
        let spent_properties = stage_mass_properties(&rocket.stages[0], 0.0, 0.0, 0.0);
        let (inertia, com) = rocket_inertia_tensor(
            upper_properties.mass_kg + spent_properties.mass_kg,
            0.0,
            1.85,
            54.4,
        );
        let pre = RocketDynamicsState::new(
            DVec3::new(6_500_000.0, 0.0, 0.0),
            DVec3::new(0.0, 2_000.0, 0.0),
            DQuat::from_rotation_x(0.2),
            upper_properties.mass_kg + spent_properties.mass_kg,
            inertia,
            com,
        );

        let first = separate_stage_dynamics(
            pre,
            upper_properties,
            spent_properties,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
            MIN_SEPARATION_CLEARANCE_M,
        );
        let second = separate_stage_dynamics(
            pre,
            upper_properties,
            spent_properties,
            DVec3::Y,
            SEPARATION_UPPER_DV_MPS,
            SPENT_STAGE_RETRO_DV_MPS,
            MIN_SEPARATION_CLEARANCE_M,
        );
        assert_eq!(first, second);
    }

    #[test]
    fn parallel_booster_jettison_is_symmetric_non_overlapping_and_momentum_neutral() {
        let boosters = ParallelBoosters::new(
            RocketStage {
                name: "Pair".into(),
                diameter_m: 2.0,
                height_m: 8.0,
                dry_mass_kg: 100.0,
                propellant_mass_kg: 10.0,
                recovery_propellant_reserve_kg: None,
                landing_gear: None,
                fairing_dry_mass_kg: None,
                engines: vec![engine_with_throttle(1.0, 1.0)],
            },
            vec![Vec3::new(-4.0, -1.0, 0.0), Vec3::new(4.0, -1.0, 0.0)],
        );
        let (inertia, com) = rocket_inertia_tensor(1_200.0, 0.0, 1.0, 12.0);
        let pre = RocketDynamicsState::new(
            DVec3::new(10.0, 20.0, 30.0),
            DVec3::new(40.0, 50.0, 60.0),
            DQuat::from_rotation_z(0.2),
            1_200.0,
            inertia,
            com,
        );
        let mut pre = pre;
        pre.angular_velocity_radps = DVec3::new(0.0, 0.0, 0.5);

        let first = separate_parallel_boosters_dynamics(
            pre,
            &boosters,
            &[0.0, 0.0],
            PARALLEL_BOOSTER_SEPARATION_DV_MPS,
        );
        let second = separate_parallel_boosters_dynamics(
            pre,
            &boosters,
            &[0.0, 0.0],
            PARALLEL_BOOSTER_SEPARATION_DV_MPS,
        );
        assert_eq!(first, second, "jettison must be deterministic");
        assert!(
            first[0].position_m.distance(first[1].position_m) > boosters.stage.diameter_m as f64,
            "booster cylinders overlap at jettison"
        );

        let omega_world = pre.orientation * pre.angular_velocity_radps;
        let mut separation_impulse_kg_mps = DVec3::ZERO;
        for (index, dynamics) in first.iter().enumerate() {
            let attachment_m = boosters
                .attachment_position_m(index)
                .expect("test index is bounded by the attachment inventory");
            let attachment_world = pre.orientation * attachment_m.as_dvec3();
            let rigid_point_velocity = pre.velocity_mps + omega_world.cross(attachment_world);
            let radial = (pre.orientation
                * DVec3::new(attachment_m.x as f64, 0.0, attachment_m.z as f64).normalize())
                * PARALLEL_BOOSTER_SEPARATION_DV_MPS;
            assert!((dynamics.velocity_mps - (rigid_point_velocity + radial)).length() < 1e-12);
            separation_impulse_kg_mps +=
                (dynamics.velocity_mps - rigid_point_velocity) * dynamics.mass_kg;
        }
        assert!(
            separation_impulse_kg_mps.length() < 1e-10,
            "symmetric booster impulses must not change core linear momentum"
        );
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
    fn throttle_range_clamps_into_engine_envelope() {
        // A zero/negative command is "engine off" and stays off even when the
        // engine has a positive minimum throttle.
        assert_eq!(clamp_throttle_range(0.0, 0.4, 0.9), 0.0);
        assert_eq!(clamp_throttle_range(-1.0, 0.4, 0.9), 0.0);
        // Command inside the range is untouched.
        assert_eq!(clamp_throttle_range(0.5, 0.4, 0.9), 0.5);
        // Commands outside the range are clamped to it.
        assert_eq!(clamp_throttle_range(0.1, 0.4, 0.9), 0.4);
        assert_eq!(clamp_throttle_range(0.95, 0.4, 0.9), 0.9);
        // Degenerate ranges stay within the physical 0..1 band.
        assert_eq!(clamp_throttle_range(1.5, 0.0, 2.5), 1.0);
        assert_eq!(clamp_throttle_range(0.3, -0.5, 0.5), 0.3);
        // Fixed-thrust engine: any positive command runs full throttle.
        assert_eq!(clamp_throttle_range(0.2, 1.0, 1.0), 1.0);
    }

    fn engine_with_throttle(min: f32, max: f32) -> RocketEngine {
        RocketEngine {
            position_m: Vec3::ZERO,
            thrust_axis: Vec3::Y,
            isp_sea_level: 282.0,
            isp_vacuum: 311.0,
            gimbal_range_deg: 5.0,
            rated_thrust_kn: 1000.0,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: min,
            throttle_max: max,
            max_ignitions: 2,
            ignition_count: 1,
            state: EngineState::Running,
        }
    }

    #[test]
    fn stage_throttle_envelope_is_the_intersection() {
        let engines = [
            engine_with_throttle(0.3, 0.9),
            engine_with_throttle(0.4, 1.0),
            engine_with_throttle(0.0, 0.7),
        ];
        let (min, max) = stage_throttle_envelope(&engines);
        assert_eq!(min, 0.4, "highest lower bound wins");
        assert_eq!(max, 0.7, "lowest upper bound wins");
        // No engines → full physical range.
        assert_eq!(stage_throttle_envelope(&[]), (0.0, 1.0));
    }

    #[test]
    fn engine_lifecycle_enforces_ignition_budget_and_terminal_cutoff() {
        let mut engine = engine_with_throttle(0.0, 1.0);
        engine.reset_lifecycle();
        engine.max_ignitions = 1;

        engine.command_lifecycle(true, true);
        assert_eq!(engine.state, EngineState::Running);
        assert_eq!(engine.ignition_count, 1);
        engine.command_lifecycle(false, true);
        assert_eq!(engine.state, EngineState::Depleted);
        engine.command_lifecycle(true, true);
        assert_eq!(engine.state, EngineState::Depleted);
        assert_eq!(engine.ignition_count, 1);
    }

    #[test]
    fn payload_mass_rides_with_active_vehicle_mass() {
        let rocket = Rocket::falcon9_test_fixture();
        let propellant = vec![90_000.0_f32, 30_000.0];
        assert_eq!(
            active_vehicle_mass_with_payload(&rocket.stages, &propellant, 0, 1_900.0),
            142_200.0 + 1_900.0
        );
        // Zero payload matches plain active_vehicle_mass.
        assert_eq!(
            active_vehicle_mass_with_payload(&rocket.stages, &propellant, 1, 0.0),
            active_vehicle_mass(&rocket.stages, &propellant, 1)
        );
    }

    #[test]
    fn serial_only_mass_and_inertia_are_unchanged_without_parallel_boosters() {
        let rocket = Rocket::falcon9_test_fixture();
        let propellant = vec![90_000.0_f32, 30_000.0];
        let serial_mass = active_vehicle_mass_with_payload(&rocket.stages, &propellant, 0, 100.0);
        let extended_mass = active_vehicle_mass_with_payload_and_boosters(
            &rocket.stages,
            &propellant,
            0,
            100.0,
            None,
            &[],
        );
        assert_eq!(serial_mass, extended_mass);
        let serial_inertia =
            active_vehicle_inertia(&rocket.stages, &propellant, 0, 100.0, 0.0, 1.85, 70.0);
        let properties = ActiveVehicleMassPropertiesInput {
            stages: &rocket.stages,
            propellant_remaining_kg: &propellant,
            active_stage: 0,
            attached_payload_kg: 100.0,
            ablation_mass_loss_kg: 0.0,
            radius_m: 1.85,
            height_m: 70.0,
            boosters: None,
            booster_propellant_remaining_kg: &[],
        }
        .calculate();
        assert_eq!(properties.mass_kg, serial_mass);
        assert_eq!(properties.inertia_body, serial_inertia.0);
        assert_eq!(properties.center_of_mass_m, serial_inertia.1);
    }

    #[test]
    fn gimbal_torque_direction_and_clamping() {
        // Engine offset toward -Y, thrust +Y → torque about a transverse axis.
        let engine = DVec3::new(0.0, -30.0, 0.0);
        let com = DVec3::ZERO;
        let _torque = gimbal_torque_body(engine, com, DVec3::Y, 1_000_000.0, 0.0, 0.0);
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
        let rocket = Rocket::falcon9_test_fixture();
        let engines = &rocket.stages[0].engines;
        // Stage-local booster engines sit below a representative fuel-loaded
        // COM, leaving enough gimbal lever arm to reproduce this command.
        let com = DVec3::new(0.0, -10.0, 0.0);
        let torque_cmd = DVec3::new(5.0e6, 0.0, -3.0e6);

        let (pitch, yaw) = allocate_gimbal_deflections(engines, com, torque_cmd, 1.0, 0.0);
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
                engine_thrust_n(engine, 1.0, 0.0),
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
            allocate_gimbal_deflections(&[], com, torque_cmd, 1.0, 0.0),
            (0.0, 0.0)
        );
    }

    #[test]
    fn consumption_updates_dynamics_mass() {
        let rocket = Rocket::falcon9_test_fixture();
        let (inertia, com) = active_vehicle_inertia(
            &rocket.stages,
            &[90_000.0, 30_000.0],
            0,
            0.0,
            0.0,
            1.85,
            70.0,
        );
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
        let consumption = consume_propellant(90_000.0, 2_500.0, 5.0);
        state.mass_kg -= consumption.consumed_kg as f64;
        assert!((state.mass_kg - (142_200.0 - 12_500.0)).abs() < 1.0);
    }
}
