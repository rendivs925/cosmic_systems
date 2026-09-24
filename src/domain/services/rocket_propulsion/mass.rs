//! Active-vehicle and per-stage mass, inertia, and center-of-mass properties.

use crate::domain::entities::rocket::{ParallelBoosters, RocketStage};
use crate::domain::math::{DMat3, DVec3};
use crate::domain::services::rocket_dynamics::rocket_inertia_tensor_with_mass_adjustments;

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
