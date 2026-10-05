//! Compiles a validated player [`VehicleDraft`] into the authoritative
//! [`VehicleDef`] used by spawned rocket flight.
//!
//! `VehicleDef` remains the single flight-configuration authority; a draft is
//! only an authoring input. Compilation is pure and deterministic so the same
//! draft and catalog always yield the same vehicle (AGENTS.md sections 26/44).

use crate::application::rocket_config::{
    DataBasis, EngineDef, EngineGroupDef, FairingDef, LandingLegsDef, RocketConfigError, StageDef,
    ThrustReferenceDef, VehicleDef, VehicleProvenance,
};
use crate::domain::game::parts::{PartCatalog, PartCategory, PartDef, PartId};
use crate::domain::game::vehicle_draft::{DraftValidationError, VehicleDraft};
use std::collections::BTreeSet;
use std::fmt;

/// Rationale attached to every compiled player vehicle. It is deliberately
/// nonblank because the compiled definition uses `Representative` data.
const PLAYER_VEHICLE_RATIONALE: &str =
    "Player-assembled MVP vehicle compiled from the part catalog; masses, dimensions, and engine stations are simulator-derived.";

/// Engine station height above the stage's lower face, meters.
const ENGINE_STATION_MARGIN_M: f32 = 0.5;

/// Compiles a valid draft into the authoritative vehicle definition.
pub fn compile_draft(
    draft: &VehicleDraft,
    catalog: &PartCatalog,
    unlocked: &BTreeSet<PartId>,
) -> Result<VehicleDef, VehicleAssemblyError> {
    draft
        .validate(catalog, unlocked)
        .map_err(VehicleAssemblyError::InvalidDraft)?;

    let groups = draft.stage_groups(catalog);
    let mut stages = Vec::with_capacity(groups.len());
    for (index, group) in groups.iter().enumerate() {
        stages.push(compile_stage(index, group, catalog)?);
    }

    let diameter_m = stages
        .iter()
        .map(|stage| stage.diameter_m)
        .fold(0.0_f32, f32::max);
    let height_m = stages.iter().map(|stage| stage.height_m).sum();

    let vehicle = VehicleDef {
        name: draft.name.trim().to_string(),
        basis: DataBasis::Representative,
        provenance: VehicleProvenance {
            manufacturer: Some("Player Assembly".to_string()),
            primary_source: None,
            representative_rationale: Some(PLAYER_VEHICLE_RATIONALE.to_string()),
        },
        diameter_m,
        height_m,
        stages,
        parallel_boosters: None,
    };
    vehicle.validate().map_err(VehicleAssemblyError::Config)?;
    Ok(vehicle)
}

fn compile_stage(
    index: usize,
    group: &[PartId],
    catalog: &PartCatalog,
) -> Result<StageDef, VehicleAssemblyError> {
    let mut defs: Vec<&PartDef> = Vec::with_capacity(group.len());
    for id in group {
        let def = catalog
            .get(id)
            .ok_or_else(|| VehicleAssemblyError::MissingPart(id.clone()))?;
        defs.push(def);
    }

    let dry_mass_kg: f32 = defs.iter().map(|def| def.dry_mass_kg as f32).sum();
    let propellant_mass_kg: f32 = defs.iter().map(|def| def.propellant_mass_kg as f32).sum();
    let height_m: f32 = defs.iter().map(|def| def.height_m as f32).sum();
    let diameter_m = defs
        .iter()
        .map(|def| def.diameter_m as f32)
        .fold(0.0_f32, f32::max);

    let engines = compile_engines(&defs, diameter_m, height_m);
    let landing_legs = defs
        .iter()
        .find(|def| def.category == PartCategory::LandingLegs)
        .and_then(|def| def.landing_legs)
        .map(|spec| LandingLegsDef {
            basis: DataBasis::Representative,
            count: spec.count,
            base_radius_m: spec.base_radius_m as f32,
            stroke_m: spec.stroke_m as f32,
            max_landing_mass_kg: None,
            deploy_altitude_m: spec.deploy_altitude_m as f32,
        });
    let fairing = defs
        .iter()
        .find(|def| def.category == PartCategory::Fairing)
        .map(|def| FairingDef {
            basis: DataBasis::Representative,
            dry_mass_kg: def.dry_mass_kg as f32,
        });

    Ok(StageDef {
        name: format!("Stage {}", index + 1),
        basis: DataBasis::Representative,
        diameter_m,
        height_m,
        dry_mass_kg,
        propellant_mass_kg,
        recovery_propellant_reserve_kg: None,
        landing_legs,
        fairing,
        engines,
    })
}

/// Places engine parts on a bounded ring near the stage's lower face. One
/// engine sits on the axis; multiple engines are evenly distributed.
fn compile_engines(defs: &[&PartDef], diameter_m: f32, height_m: f32) -> EngineGroupDef {
    let engine_parts: Vec<&PartDef> = defs
        .iter()
        .copied()
        .filter(|def| def.engine.is_some())
        .collect();
    let count = engine_parts.len();
    let ring_radius_m = if count <= 1 {
        0.0
    } else {
        (diameter_m * 0.5 - 0.4).max(0.3)
    };
    let station_y_m = -(height_m * 0.5) + ENGINE_STATION_MARGIN_M;

    let values = engine_parts
        .iter()
        .enumerate()
        .filter_map(|(index, def)| def.engine.map(|spec| (index, spec)))
        .map(|(index, spec)| {
            let angle = if count <= 1 {
                0.0
            } else {
                std::f32::consts::TAU * index as f32 / count as f32
            };
            EngineDef {
                position: [
                    ring_radius_m * angle.cos(),
                    station_y_m,
                    ring_radius_m * angle.sin(),
                ],
                thrust_axis: [0.0, 1.0, 0.0],
                isp_sl: spec.isp_sea_level_s as f32,
                isp_vac: spec.isp_vacuum_s as f32,
                gimbal_range_deg: spec.gimbal_range_deg as f32,
                rated_thrust_n: spec.rated_thrust_n as f32,
                thrust_reference: ThrustReferenceDef::SeaLevel,
                throttle_min: spec.throttle_min as f32,
                throttle_max: spec.throttle_max as f32,
                max_ignitions: spec.max_ignitions,
            }
        })
        .collect();

    EngineGroupDef {
        basis: DataBasis::Representative,
        values,
    }
}

#[derive(Debug)]
pub enum VehicleAssemblyError {
    InvalidDraft(Vec<DraftValidationError>),
    MissingPart(PartId),
    Config(RocketConfigError),
}

impl fmt::Display for VehicleAssemblyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDraft(errors) => {
                write!(f, "vehicle draft is invalid: ")?;
                for (index, error) in errors.iter().enumerate() {
                    if index > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{error}")?;
                }
                Ok(())
            }
            Self::MissingPart(id) => write!(f, "part catalog is missing part {id}"),
            Self::Config(error) => write!(f, "compiled vehicle failed validation: {error}"),
        }
    }
}

impl std::error::Error for VehicleAssemblyError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::part_catalog::load_default_part_catalog;
    use crate::domain::game::parts::PartId;

    fn catalog() -> PartCatalog {
        load_default_part_catalog().expect("shipped part catalog loads")
    }

    fn unlocked(catalog: &PartCatalog) -> BTreeSet<PartId> {
        catalog.iter().map(|part| part.id.clone()).collect()
    }

    fn id(value: &str) -> PartId {
        PartId::new(value).unwrap()
    }

    fn single_stage() -> VehicleDraft {
        VehicleDraft::new(
            "Hopper",
            vec![id("engine_kestrel"), id("tank_small"), id("capsule_1")],
        )
    }

    fn two_stage() -> VehicleDraft {
        VehicleDraft::new(
            "Ares",
            vec![
                id("engine_kestrel"),
                id("tank_small"),
                id("legs_1"),
                id("separator_1"),
                id("engine_kestrel"),
                id("tank_small"),
                id("capsule_1"),
            ],
        )
    }

    #[test]
    fn single_stage_derives_mass_and_geometry() {
        let catalog = catalog();
        let vehicle = compile_draft(&single_stage(), &catalog, &unlocked(&catalog)).unwrap();
        assert_eq!(vehicle.stages.len(), 1);
        let stage = &vehicle.stages[0];
        assert!((stage.dry_mass_kg - 2_900.0).abs() < 1e-3);
        assert!((stage.propellant_mass_kg - 9_000.0).abs() < 1e-3);
        assert!((stage.height_m - 10.3).abs() < 1e-3);
        assert!((vehicle.height_m - 10.3).abs() < 1e-3);
        assert_eq!(stage.engines.values.len(), 1);
    }

    #[test]
    fn two_stage_preserves_bottom_to_top_order_and_gear() {
        let catalog = catalog();
        let vehicle = compile_draft(&two_stage(), &catalog, &unlocked(&catalog)).unwrap();
        assert_eq!(vehicle.stages.len(), 2);
        assert_eq!(vehicle.stages[0].name, "Stage 1");
        assert_eq!(vehicle.stages[1].name, "Stage 2");
        assert!(vehicle.stages[0].landing_legs.is_some());
        assert!(vehicle.stages[1].landing_legs.is_none());
        assert!(vehicle
            .stages
            .iter()
            .all(|stage| stage.engines.values.len() == 1));
    }

    #[test]
    fn compiled_vehicle_converts_to_domain_with_matching_stages() {
        let catalog = catalog();
        let vehicle = compile_draft(&two_stage(), &catalog, &unlocked(&catalog)).unwrap();
        let loaded = vehicle.to_domain();
        assert_eq!(loaded.rocket.stages.len(), 2);
        let expected_mass = vehicle
            .stages
            .iter()
            .map(|stage| stage.dry_mass_kg + stage.propellant_mass_kg)
            .sum::<f32>();
        assert!((loaded.rocket.total_mass_kg() - expected_mass).abs() < 1.0);
    }

    #[test]
    fn invalid_draft_is_rejected_before_compilation() {
        let catalog = catalog();
        let draft = VehicleDraft::new("NoCapsule", vec![id("engine_kestrel"), id("tank_small")]);
        let error = compile_draft(&draft, &catalog, &unlocked(&catalog)).unwrap_err();
        match error {
            VehicleAssemblyError::InvalidDraft(errors) => {
                assert!(errors.contains(&DraftValidationError::MissingControlPath));
            }
            other => panic!("expected InvalidDraft, got {other:?}"),
        }
    }

    #[test]
    fn locked_part_blocks_compilation() {
        let catalog = catalog();
        let mut unlocked = unlocked(&catalog);
        unlocked.remove(&id("tank_small"));
        let error = compile_draft(&single_stage(), &catalog, &unlocked).unwrap_err();
        assert!(matches!(error, VehicleAssemblyError::InvalidDraft(_)));
    }
}
