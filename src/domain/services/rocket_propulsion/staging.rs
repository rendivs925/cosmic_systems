//! Stage separation impulses and independent post-separation dynamics.

use super::mass::{stage_mass_properties, StageMassProperties};
use crate::domain::entities::rocket::{ParallelBoosters, RocketStage};
use crate::domain::math::{DQuat, DVec3};

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
