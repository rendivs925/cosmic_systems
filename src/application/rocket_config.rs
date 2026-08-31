//! Data-driven rocket vehicle definitions (AGENTS.md sections 39-40).
//!
//! Vehicles are described in RON files under `assets/configs/rockets/*.ron`
//! and converted once at load into the domain [`Rocket`] model. This module
//! owns the file schema and validation; the domain structs stay free of any
//! serialization concerns. Loading fails fast with a clear error for invalid
//! configuration (AGENTS.md section 65).

use crate::domain::entities::rocket::{
    EngineState, ParallelBoosters, Rocket, RocketEngine, RocketStage, ThrustReference,
};
use crate::domain::services::landing_gear::LandingGearSpec;
use bevy::math::Vec3;
use bevy::prelude::*;
use ron::error::SpannedError;
use ron::extensions::Extensions;
use ron::Options;
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

/// Sanity ceiling for engine gimbal ranges, degrees.
pub const MAX_GIMBAL_RANGE_DEG: f32 = 15.0;

/// Newtons per kilonewton: the file format uses N, the domain uses kN.
const NEWTONS_PER_KN: f32 = 1000.0;

// ---------------------------------------------------------------------------
// File format (RON via serde; files may carry // comments)
// ---------------------------------------------------------------------------

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RocketConfigFile {
    pub vehicles: Vec<VehicleDef>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDef {
    pub name: String,
    /// Source status for vehicle-level geometry.
    pub basis: DataBasis,
    pub provenance: VehicleProvenance,
    pub diameter_m: f32,
    pub height_m: f32,
    #[serde(default)]
    pub stages: Vec<StageDef>,
    /// Optional identical boosters that burn concurrently with the core stage.
    #[serde(default)]
    pub parallel_boosters: Option<ParallelBoostersDef>,
}

/// Per-stage landing-gear definition. Stages without a `landing_legs` block
/// stay valid and use the point-contact model when independently landed.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct LandingLegsDef {
    pub basis: DataBasis,
    pub count: u32,
    pub base_radius_m: f32,
    pub stroke_m: f32,
    /// Maximum mass the gear can land; defaults to the whole vehicle when
    /// omitted.
    #[serde(default)]
    pub max_landing_mass_kg: Option<f32>,
    /// Radar altitude at which the legs auto-deploy during descent, meters.
    pub deploy_altitude_m: f32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct StageDef {
    pub name: String,
    pub basis: DataBasis,
    /// Outer cylindrical diameter used by the active-stage force and inertia
    /// model, meters.
    pub diameter_m: f32,
    /// Physical stage length used by the active-stage force and inertia model,
    /// meters.
    pub height_m: f32,
    pub dry_mass_kg: f32,
    pub propellant_mass_kg: f32,
    /// Propellant held back for a first-stage recovery burn sequence. Stages
    /// without a reserve remain expendable debris after separation.
    #[serde(default)]
    pub recovery_propellant_reserve_kg: Option<f32>,
    /// Optional deployable gear installed on this serial stage only.
    #[serde(default)]
    pub landing_legs: Option<LandingLegsDef>,
    /// Payload fairing physically attached to this serial stage. The current
    /// vehicle architecture permits it only on the final serial stage.
    #[serde(default)]
    pub fairing: Option<FairingDef>,
    pub engines: EngineGroupDef,
}

/// Parallel booster hardware. Attachment positions are booster cylinder origins
/// in the full vehicle stack frame, while engine stations stay stage-local.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelBoostersDef {
    pub basis: DataBasis,
    pub count: u32,
    pub stage: StageDef,
    pub attachment_positions: Vec<[f32; 3]>,
}

/// One source-status declaration covers an explicit set of engine values.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineGroupDef {
    pub basis: DataBasis,
    pub values: Vec<EngineDef>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct EngineDef {
    /// Stage-local position, meters, from the stage cylinder geometric center
    /// (+Y longitudinal, nose-up).
    pub position: [f32; 3],
    /// Body-frame thrust unit axis.
    pub thrust_axis: [f32; 3],
    pub isp_sl: f32,
    pub isp_vac: f32,
    pub gimbal_range_deg: f32,
    /// Full-throttle rated thrust, in newtons, at `thrust_reference`.
    pub rated_thrust_n: f32,
    /// The required pressure endpoint for `rated_thrust_n`.
    pub thrust_reference: ThrustReferenceDef,
    #[serde(default)]
    pub throttle_min: f32,
    #[serde(default = "default_throttle_max")]
    pub throttle_max: f32,
    /// Required catalogued lifetime start budget. It is intentionally explicit:
    /// unknown operational limits must not silently become unlimited restarts.
    pub max_ignitions: u32,
}

/// RON representation of the pressure endpoint for an engine's rated thrust.
/// `SeaLevel` means standard sea-level pressure; `Vacuum` means zero pressure.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub enum ThrustReferenceDef {
    SeaLevel,
    Vacuum,
}

impl From<ThrustReferenceDef> for ThrustReference {
    fn from(value: ThrustReferenceDef) -> Self {
        match value {
            ThrustReferenceDef::SeaLevel => Self::SeaLevel,
            ThrustReferenceDef::Vacuum => Self::Vacuum,
        }
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct FairingDef {
    pub basis: DataBasis,
    pub dry_mass_kg: f32,
}

/// Declares whether a numerical group is pinned to a verified source byte.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub enum DataBasis {
    SourceVerified,
    Representative,
}

/// Per-vehicle source record. Representative definitions may retain partial
/// source details while their exact source byte is not pinned.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleProvenance {
    #[serde(default)]
    pub manufacturer: Option<String>,
    #[serde(default)]
    pub primary_source: Option<PrimarySource>,
    #[serde(default)]
    pub representative_rationale: Option<String>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PrimarySource {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub publication_date: Option<String>,
    #[serde(default)]
    pub source_url: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
}

fn default_throttle_max() -> f32 {
    1.0
}

fn is_iso_publication_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if !(bytes.len() == 7 || bytes.len() == 10)
        || bytes.get(4) != Some(&b'-')
        || (bytes.len() == 10 && bytes.get(7) != Some(&b'-'))
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[..4].parse::<u16>().ok();
    let month = value[5..7].parse::<u8>().ok();
    let Some((year, month)) = year.zip(month) else {
        return false;
    };
    if year == 0 || !(1..=12).contains(&month) {
        return false;
    }
    if bytes.len() == 7 {
        return true;
    }
    let Some(day) = value[8..10].parse::<u8>().ok() else {
        return false;
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => return false,
    };
    (1..=max_day).contains(&day)
}

fn is_https_url(value: &str) -> bool {
    value
        .strip_prefix("https://")
        .is_some_and(|authority_and_path| {
            authority_and_path
                .split('/')
                .next()
                .is_some_and(|authority| !authority.is_empty())
                && !value.contains(char::is_whitespace)
        })
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Failure modes when loading vehicle configuration. Display strings are
/// user-facing: startup surfaces them verbatim so a broken definition points
/// straight at the offending file or vehicle (AGENTS.md section 65).
#[derive(Debug)]
pub enum RocketConfigError {
    /// The RON text did not deserialize into [`RocketConfigFile`].
    Parse(SpannedError),
    /// A config directory or file could not be read.
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    /// A vehicle definition failed [`VehicleDef::validate`].
    InvalidVehicle { name: String, reason: String },
    /// A config file name has no stem usable as a catalog key.
    MissingStem { path: PathBuf },
    /// Two files map onto the same catalog key.
    DuplicateKey { key: String, path: PathBuf },
    /// The config directory contained no `*.ron` vehicle definitions.
    NoVehicles { dir: PathBuf },
}

impl fmt::Display for RocketConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "RON parse error: {e}"),
            Self::Io { path, source } => write!(f, "cannot read {}: {source}", path.display()),
            Self::InvalidVehicle { name, reason } => {
                write!(f, "invalid vehicle definition '{name}': {reason}")
            }
            Self::MissingStem { path } => {
                write!(f, "config file {} has no usable stem", path.display())
            }
            Self::DuplicateKey { key, path } => {
                write!(f, "duplicate vehicle key '{key}' in {}", path.display())
            }
            Self::NoVehicles { dir } => {
                write!(f, "no vehicle definitions found in {}", dir.display())
            }
        }
    }
}

impl std::error::Error for RocketConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Loaded representation
// ---------------------------------------------------------------------------

/// One vehicle ready for ECS spawning.
#[derive(Debug, Clone)]
pub struct LoadedVehicle {
    pub rocket: Rocket,
}

impl VehicleDef {
    /// Fail-fast validation of physical plausibility (AGENTS.md section 65).
    pub fn validate(&self) -> Result<(), RocketConfigError> {
        if self.name.trim().is_empty() {
            return Err(self.invalid("vehicle name must not be empty"));
        }
        self.validate_basis("vehicle geometry", self.basis)?;
        if !self.diameter_m.is_finite()
            || !self.height_m.is_finite()
            || self.diameter_m <= 0.0
            || self.height_m <= 0.0
        {
            return Err(self.invalid("needs positive diameter_m and height_m"));
        }
        if self.stages.is_empty() {
            return Err(self.invalid("needs at least one stage"));
        }
        let fairing_count = self
            .stages
            .iter()
            .filter(|stage| stage.fairing.is_some())
            .count();
        if fairing_count > 1 {
            return Err(self.invalid("only one serial stage may declare a fairing"));
        }
        for (i, stage) in self.stages.iter().enumerate() {
            if stage.name.trim().is_empty() {
                return Err(self.invalid(format!("stage {i}: name must not be empty")));
            }
            let at = self.stage_context(i, stage);
            self.validate_basis(&at, stage.basis)?;
            if !stage.dry_mass_kg.is_finite() || stage.dry_mass_kg <= 0.0 {
                return Err(self.invalid(format!("{at}: dry_mass_kg must be > 0")));
            }
            if !stage.diameter_m.is_finite()
                || !stage.height_m.is_finite()
                || stage.diameter_m <= 0.0
                || stage.height_m <= 0.0
            {
                return Err(self.invalid(format!("{at}: diameter_m and height_m must be > 0")));
            }
            if !stage.propellant_mass_kg.is_finite() {
                return Err(self.invalid(format!("{at}: propellant_mass_kg must be finite")));
            }
            if !stage.engines.values.is_empty() && stage.propellant_mass_kg <= 0.0 {
                return Err(
                    self.invalid(format!("{at}: carries engines but propellant_mass_kg <= 0"))
                );
            }
            if i + 1 == self.stages.len() && stage.recovery_propellant_reserve_kg.is_some() {
                return Err(self.invalid(format!(
                    "{at}: final stage cannot declare recovery_propellant_reserve_kg"
                )));
            }
            if stage.recovery_propellant_reserve_kg.is_some_and(|reserve| {
                !reserve.is_finite() || reserve <= 0.0 || reserve >= stage.propellant_mass_kg
            }) {
                return Err(self.invalid(format!(
                    "{at}: recovery_propellant_reserve_kg must be > 0 and less than propellant_mass_kg"
                )));
            }
            if stage.fairing.is_some() && i + 1 != self.stages.len() {
                return Err(self.invalid(format!(
                    "{at}: fairing may be declared only on the final serial stage"
                )));
            }
            if let Some(fairing) = &stage.fairing {
                self.validate_basis(&format!("{at} fairing"), fairing.basis)?;
                if !fairing.dry_mass_kg.is_finite() || fairing.dry_mass_kg <= 0.0 {
                    return Err(self.invalid(format!("{at}: fairing dry_mass_kg must be > 0")));
                }
            }
            if let Some(legs) = &stage.landing_legs {
                self.validate_basis(&format!("{at} landing_legs"), legs.basis)?;
                self.validate_landing_legs(&at, legs)?;
            }
            self.validate_basis(&format!("{at} engines"), stage.engines.basis)?;
            if stage.engines.values.is_empty() {
                return Err(self.invalid(format!("{at}: needs at least one engine")));
            }
            for (e, engine) in stage.engines.values.iter().enumerate() {
                self.validate_engine(&format!("{at} engine {e}"), stage, engine)?;
            }
        }
        if let Some(boosters) = &self.parallel_boosters {
            let stage = &boosters.stage;
            let at = format!("vehicle {} parallel boosters ({})", self.name, stage.name);
            self.validate_basis("parallel_boosters", boosters.basis)?;
            self.validate_basis(&at, stage.basis)?;
            if boosters.count == 0 || boosters.count % 2 != 0 {
                return Err(self.invalid("parallel_boosters count must be a positive even number"));
            }
            if boosters.attachment_positions.len() != boosters.count as usize {
                return Err(
                    self.invalid("parallel_boosters attachment_positions length must equal count")
                );
            }
            if stage.name.trim().is_empty()
                || !stage.diameter_m.is_finite()
                || !stage.height_m.is_finite()
                || stage.diameter_m <= 0.0
                || stage.height_m <= 0.0
            {
                return Err(self.invalid(format!("{at}: needs a name and positive dimensions")));
            }
            if !stage.dry_mass_kg.is_finite()
                || stage.dry_mass_kg <= 0.0
                || !stage.propellant_mass_kg.is_finite()
                || stage.propellant_mass_kg <= 0.0
            {
                return Err(self.invalid(format!(
                    "{at}: dry_mass_kg and propellant_mass_kg must be > 0"
                )));
            }
            if stage.recovery_propellant_reserve_kg.is_some() {
                return Err(self.invalid(format!("{at}: recovery propellant is not supported")));
            }
            if stage.landing_legs.is_some() {
                return Err(self.invalid(format!(
                    "{at}: landing_legs are not supported; parallel boosters never inherit core landing gear"
                )));
            }
            if stage.fairing.is_some() {
                return Err(
                    self.invalid(format!("{at}: parallel boosters cannot declare a fairing"))
                );
            }
            self.validate_basis(&format!("{at} engines"), stage.engines.basis)?;
            if stage.engines.values.is_empty() {
                return Err(self.invalid(format!("{at}: needs at least one engine")));
            }
            for (engine_index, engine) in stage.engines.values.iter().enumerate() {
                self.validate_engine(&format!("{at} engine {engine_index}"), stage, engine)?;
            }
            for (index, position) in boosters.attachment_positions.iter().enumerate() {
                let position_m = Vec3::from_array(*position);
                if !position_m.is_finite() || position_m.x.hypot(position_m.z) <= f32::EPSILON {
                    return Err(self.invalid(format!(
                        "{at} attachment {index}: must be finite and radial"
                    )));
                }
                if position_m.y.abs() + stage.height_m * 0.5 > self.height_m * 0.5 {
                    return Err(self.invalid(format!(
                        "{at} attachment {index}: booster must fit within vehicle height_m"
                    )));
                }
                if position_m.x.hypot(position_m.z) < (self.diameter_m + stage.diameter_m) * 0.5 {
                    return Err(self.invalid(format!(
                        "{at} attachment {index}: overlaps the core cylinder"
                    )));
                }
            }
            let (attachment_pairs, remainder) = boosters.attachment_positions.as_chunks::<2>();
            debug_assert!(remainder.is_empty(), "the validated count is even");
            for [left, right] in attachment_pairs {
                let left = Vec3::from_array(*left);
                let right = Vec3::from_array(*right);
                if (left.x + right.x).abs() > 1e-4
                    || (left.z + right.z).abs() > 1e-4
                    || (left.y - right.y).abs() > 1e-4
                {
                    return Err(self.invalid(format!(
                        "{at}: attachment pairs must be mirrored across the stack axis"
                    )));
                }
            }
            for (left_index, left) in boosters.attachment_positions.iter().enumerate() {
                for right in boosters.attachment_positions.iter().skip(left_index + 1) {
                    if Vec3::from_array(*left).distance(Vec3::from_array(*right)) < stage.diameter_m
                    {
                        return Err(self.invalid(format!("{at}: attachment positions overlap")));
                    }
                }
            }
        }
        let stage_height_m: f32 = self.stages.iter().map(|stage| stage.height_m).sum();
        if !stage_height_m.is_finite() || stage_height_m > self.height_m {
            return Err(self.invalid("total stage height must fit within height_m"));
        }
        Ok(())
    }

    /// Typed wrapper attaching this vehicle's name to a validation reason.
    fn invalid(&self, reason: impl Into<String>) -> RocketConfigError {
        RocketConfigError::InvalidVehicle {
            name: self.name.clone(),
            reason: reason.into(),
        }
    }

    /// Shared error-context prefix naming vehicle and stage ("vehicle F9
    /// stage 0 (booster)"), so every per-stage/per-engine message is located
    /// and built DRY.
    fn stage_context(&self, index: usize, stage: &StageDef) -> String {
        format!("vehicle {} stage {index} ({})", self.name, stage.name)
    }

    fn validate_basis(&self, group: &str, basis: DataBasis) -> Result<(), RocketConfigError> {
        match basis {
            DataBasis::Representative => {
                if self
                    .provenance
                    .representative_rationale
                    .as_deref()
                    .is_none_or(|rationale| rationale.trim().is_empty())
                {
                    return Err(self.invalid(format!(
                        "{group}: Representative basis requires nonblank provenance.representative_rationale"
                    )));
                }
            }
            DataBasis::SourceVerified => self.validate_verified_provenance(group)?,
        }
        Ok(())
    }

    fn validate_verified_provenance(&self, group: &str) -> Result<(), RocketConfigError> {
        if self
            .provenance
            .manufacturer
            .as_deref()
            .is_none_or(|manufacturer| manufacturer.trim().is_empty())
        {
            return Err(self.invalid(format!(
                "{group}: SourceVerified basis requires nonblank provenance.manufacturer"
            )));
        }
        let Some(source) = &self.provenance.primary_source else {
            return Err(self.invalid(format!(
                "{group}: SourceVerified basis requires provenance.primary_source"
            )));
        };
        for (field, value) in [("title", &source.title), ("version", &source.version)] {
            if value.as_deref().is_none_or(|value| value.trim().is_empty()) {
                return Err(self.invalid(format!(
                    "{group}: SourceVerified basis requires nonblank provenance.primary_source.{field}"
                )));
            }
        }
        let date = source.publication_date.as_deref().unwrap_or_default();
        if !is_iso_publication_date(date) {
            return Err(self.invalid(format!(
                "{group}: SourceVerified basis requires provenance.primary_source.publication_date in YYYY-MM or YYYY-MM-DD"
            )));
        }
        let url = source.source_url.as_deref().unwrap_or_default();
        if !is_https_url(url) {
            return Err(self.invalid(format!(
                "{group}: SourceVerified basis requires an HTTPS provenance.primary_source.source_url"
            )));
        }
        let sha256 = source.sha256.as_deref().unwrap_or_default();
        if !is_lowercase_sha256(sha256) {
            return Err(self.invalid(format!(
                "{group}: SourceVerified basis requires a lowercase 64-hex provenance.primary_source.sha256"
            )));
        }
        Ok(())
    }

    /// Every serial stage may carry physical gear, including a final stage.
    /// Recovery propellant remains separately prohibited on final stages.
    fn validate_landing_legs(
        &self,
        at: &str,
        legs: &LandingLegsDef,
    ) -> Result<(), RocketConfigError> {
        if legs.count < 3 {
            return Err(self.invalid(format!(
                "{at}: landing_legs count must be >= 3 for a stable stance"
            )));
        }
        if !legs.base_radius_m.is_finite() || legs.base_radius_m <= 0.0 {
            return Err(self.invalid(format!("{at}: landing_legs base_radius_m must be > 0")));
        }
        if !legs.stroke_m.is_finite() || legs.stroke_m <= 0.0 {
            return Err(self.invalid(format!("{at}: landing_legs stroke_m must be > 0")));
        }
        if !legs.deploy_altitude_m.is_finite() || legs.deploy_altitude_m <= 0.0 {
            return Err(self.invalid(format!("{at}: landing_legs deploy_altitude_m must be > 0")));
        }
        if legs
            .max_landing_mass_kg
            .is_some_and(|mass_kg| !mass_kg.is_finite() || mass_kg <= 0.0)
        {
            return Err(self.invalid(format!(
                "{at}: landing_legs max_landing_mass_kg must be > 0"
            )));
        }
        Ok(())
    }

    fn validate_engine(
        &self,
        at: &str,
        stage: &StageDef,
        engine: &EngineDef,
    ) -> Result<(), RocketConfigError> {
        if !engine.isp_sl.is_finite()
            || !engine.isp_vac.is_finite()
            || engine.isp_sl <= 0.0
            || engine.isp_vac <= 0.0
        {
            return Err(self.invalid(format!("{at}: isp_sl and isp_vac must be > 0")));
        }
        if !engine.gimbal_range_deg.is_finite()
            || !(0.0..=MAX_GIMBAL_RANGE_DEG).contains(&engine.gimbal_range_deg)
        {
            return Err(self.invalid(format!(
                "{at}: gimbal_range_deg must be within [0, {MAX_GIMBAL_RANGE_DEG}]"
            )));
        }
        if !engine.rated_thrust_n.is_finite() || engine.rated_thrust_n <= 0.0 {
            return Err(self.invalid(format!("{at}: rated_thrust_n must be > 0")));
        }
        if engine.max_ignitions == 0 {
            return Err(self.invalid(format!("{at}: max_ignitions must be > 0")));
        }
        if !engine.throttle_min.is_finite()
            || !engine.throttle_max.is_finite()
            || !(0.0..=1.0).contains(&engine.throttle_min)
            || !(0.0..=1.0).contains(&engine.throttle_max)
            || engine.throttle_min > engine.throttle_max
        {
            return Err(self.invalid(format!(
                "{at}: throttle bounds must satisfy 0 <= min <= max <= 1"
            )));
        }
        let axis = Vec3::from_array(engine.thrust_axis);
        if !axis.is_finite() || axis.length_squared() < f32::EPSILON {
            return Err(self.invalid(format!(
                "{at}: thrust_axis must be a finite non-zero vector"
            )));
        }
        let position_m = Vec3::from_array(engine.position);
        if !position_m.is_finite() {
            return Err(self.invalid(format!("{at}: position must be finite")));
        }
        let radial_distance_m = position_m.x.hypot(position_m.z);
        if radial_distance_m > stage.diameter_m * 0.5 {
            return Err(self.invalid(format!(
                "{at}: position radial distance must be within the stage radius"
            )));
        }
        if position_m.y.abs() > stage.height_m * 0.5 {
            return Err(self.invalid(format!(
                "{at}: position y must be within the stage half-height"
            )));
        }
        Ok(())
    }

    /// Convert into the domain model. Call [`VehicleDef::validate`] first;
    /// this function assumes valid input.
    pub fn to_domain(&self) -> LoadedVehicle {
        let stages = self
            .stages
            .iter()
            .map(|stage| RocketStage {
                name: stage.name.clone(),
                diameter_m: stage.diameter_m,
                height_m: stage.height_m,
                dry_mass_kg: stage.dry_mass_kg,
                propellant_mass_kg: stage.propellant_mass_kg,
                recovery_propellant_reserve_kg: stage.recovery_propellant_reserve_kg,
                landing_gear: stage.landing_legs.as_ref().map(|legs| LandingGearSpec {
                    count: legs.count,
                    base_radius_m: legs.base_radius_m as f64,
                    stroke_m: legs.stroke_m as f64,
                    max_landing_mass_kg: legs.max_landing_mass_kg.map(|mass_kg| mass_kg as f64),
                    deploy_altitude_m: legs.deploy_altitude_m as f64,
                }),
                fairing_dry_mass_kg: stage.fairing.as_ref().map(|fairing| fairing.dry_mass_kg),
                engines: stage
                    .engines
                    .values
                    .iter()
                    .map(|engine| RocketEngine {
                        position_m: Vec3::from_array(engine.position),
                        thrust_axis: Vec3::from_array(engine.thrust_axis).normalize_or_zero(),
                        isp_sea_level: engine.isp_sl,
                        isp_vacuum: engine.isp_vac,
                        gimbal_range_deg: engine.gimbal_range_deg,
                        rated_thrust_kn: engine.rated_thrust_n / NEWTONS_PER_KN,
                        thrust_reference: engine.thrust_reference.into(),
                        throttle_min: engine.throttle_min,
                        throttle_max: engine.throttle_max,
                        max_ignitions: engine.max_ignitions,
                        ignition_count: 0,
                        state: EngineState::Off,
                    })
                    .collect(),
            })
            .collect();
        LoadedVehicle {
            rocket: Rocket {
                name: self.name.clone(),
                diameter_m: self.diameter_m,
                height_m: self.height_m,
                stages,
                parallel_boosters: self.parallel_boosters.as_ref().map(|boosters| {
                    ParallelBoosters {
                        count: boosters.count,
                        stage: RocketStage {
                            name: boosters.stage.name.clone(),
                            diameter_m: boosters.stage.diameter_m,
                            height_m: boosters.stage.height_m,
                            dry_mass_kg: boosters.stage.dry_mass_kg,
                            propellant_mass_kg: boosters.stage.propellant_mass_kg,
                            recovery_propellant_reserve_kg: None,
                            // Parallel boosters do not receive serial-stage gear.
                            landing_gear: None,
                            fairing_dry_mass_kg: None,
                            engines: boosters
                                .stage
                                .engines
                                .values
                                .iter()
                                .map(|engine| RocketEngine {
                                    position_m: Vec3::from_array(engine.position),
                                    thrust_axis: Vec3::from_array(engine.thrust_axis)
                                        .normalize_or_zero(),
                                    isp_sea_level: engine.isp_sl,
                                    isp_vacuum: engine.isp_vac,
                                    gimbal_range_deg: engine.gimbal_range_deg,
                                    rated_thrust_kn: engine.rated_thrust_n / NEWTONS_PER_KN,
                                    thrust_reference: engine.thrust_reference.into(),
                                    throttle_min: engine.throttle_min,
                                    throttle_max: engine.throttle_max,
                                    max_ignitions: engine.max_ignitions,
                                    ignition_count: 0,
                                    state: EngineState::Off,
                                })
                                .collect(),
                        },
                        attachment_positions_m: boosters
                            .attachment_positions
                            .iter()
                            .map(|position| Vec3::from_array(*position))
                            .collect(),
                    }
                }),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Catalog resource + file loading
// ---------------------------------------------------------------------------

/// All vehicles available for selection, keyed by config file stem
/// (`falcon9.ron` → key `falcon9`). A BTreeMap keeps listing deterministic.
#[derive(Resource, Debug, Default)]
pub struct RocketCatalog {
    vehicles: BTreeMap<String, LoadedVehicle>,
}

impl RocketCatalog {
    pub fn insert(&mut self, key: impl Into<String>, vehicle: LoadedVehicle) {
        self.vehicles.insert(key.into(), vehicle);
    }

    pub fn get(&self, key: &str) -> Option<&LoadedVehicle> {
        self.vehicles.get(key)
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.vehicles.keys()
    }

    /// Deterministic default selection (first key in sorted order).
    pub fn first_key(&self) -> Option<&str> {
        self.vehicles.keys().next().map(String::as_str)
    }

    fn contains_key(&self, key: &str) -> bool {
        self.vehicles.contains_key(key)
    }

    /// Load every `*.ron` vehicle definition from the config directory,
    /// keyed by config-file stem (`falcon9.ron` → key `falcon9`) so the CLI
    /// selection matches the shipped file names.
    pub fn from_dir() -> Result<RocketCatalog, RocketConfigError> {
        let dir = configs_root();
        let entries = fs::read_dir(&dir).map_err(|source| RocketConfigError::Io {
            path: dir.clone(),
            source,
        })?;

        let mut paths: Vec<PathBuf> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ron"))
            .collect();
        paths.sort();

        let mut catalog = Self::default();
        for path in paths {
            let Some(key) = path.file_stem().and_then(|stem| stem.to_str()) else {
                return Err(RocketConfigError::MissingStem { path });
            };
            let text = fs::read_to_string(&path).map_err(|source| RocketConfigError::Io {
                path: path.clone(),
                source,
            })?;
            if catalog.contains_key(key) {
                return Err(RocketConfigError::DuplicateKey {
                    key: key.to_string(),
                    path,
                });
            }
            for vehicle in RocketConfigFile::parse(&text)? {
                catalog.insert(key, vehicle);
            }
        }
        if catalog.first_key().is_none() {
            return Err(RocketConfigError::NoVehicles { dir });
        }
        Ok(catalog)
    }
}

/// Location of the shipped vehicle definitions, relative to the asset root.
pub const CONFIGS_RELATIVE_PATH: &str = "assets/configs/rockets";

/// Default vehicle when no `--vehicle` argument is given.
pub const DEFAULT_VEHICLE_KEY: &str = "falcon9";

/// CLI-selected vehicle key (`--vehicle <key>`); `None` means default.
#[derive(Resource, Debug, Default, Clone)]
pub struct VehicleSelection(pub Option<String>);

/// Resolve the config directory the same way bevy_asset resolves its asset
/// root: BEVY_ASSET_ROOT env, then CARGO_MANIFEST_DIR (set under cargo),
/// then the current directory.
fn configs_root() -> PathBuf {
    if let Ok(root) = env::var("BEVY_ASSET_ROOT") {
        return Path::new(&root).join(CONFIGS_RELATIVE_PATH);
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        return Path::new(&manifest_dir).join(CONFIGS_RELATIVE_PATH);
    }
    PathBuf::from(CONFIGS_RELATIVE_PATH)
}

impl RocketConfigFile {
    /// RON parse options: IMPLICIT_SOME lets final stage blocks write
    /// `fairing: ( ... )` instead of the noisier `Some(( ... ))`.
    fn ron_options() -> Options {
        Options::default().with_default_extension(Extensions::IMPLICIT_SOME)
    }

    /// Parse one RON vehicle-definition document into loaded vehicles,
    /// validating each definition before conversion. Exposed for tests and
    /// future loaders (network, embedded assets); selection keys come from
    /// config file stems ([`RocketCatalog::from_dir`]), not display names.
    pub fn parse(text: &str) -> Result<Vec<LoadedVehicle>, RocketConfigError> {
        let file: RocketConfigFile = Self::ron_options()
            .from_str(text)
            .map_err(RocketConfigError::Parse)?;
        let mut out = Vec::with_capacity(file.vehicles.len());
        for vehicle in &file.vehicles {
            vehicle.validate()?;
            out.push(vehicle.to_domain());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FALCON9_RON: &str = "falcon9.ron";

    fn load_shipped(file: &str) -> LoadedVehicle {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let path = Path::new(&manifest).join(CONFIGS_RELATIVE_PATH).join(file);
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        RocketConfigFile::parse(&text)
            .unwrap_or_else(|e| panic!("{file}: {e}"))
            .into_iter()
            .next()
            .expect("one vehicle")
    }

    fn source_verified_electron_definition() -> String {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let path = Path::new(&manifest)
            .join(CONFIGS_RELATIVE_PATH)
            .join("electron.ron");
        fs::read_to_string(path)
            .expect("electron config exists")
            .replace("Representative", "SourceVerified")
    }

    /// Parse a definition expected to be invalid; returns the user-facing
    /// error message for substring assertions.
    #[cfg(test)]
    fn parse_err(text: &str) -> String {
        RocketConfigFile::parse(text)
            .expect_err("definition should fail validation")
            .to_string()
    }

    #[test]
    fn basis_is_required_for_every_numerical_group() {
        let text = fs::read_to_string(
            Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
                .join(CONFIGS_RELATIVE_PATH)
                .join("electron.ron"),
        )
        .unwrap();
        let error = parse_err(&text.replacen("basis: Representative,", "", 1));
        assert!(error.contains("RON parse error"), "{error}");
    }

    #[test]
    fn representative_groups_require_a_nonblank_rationale() {
        let text = source_verified_electron_definition()
            .replace("SourceVerified", "Representative")
            .replace(
                "representative_rationale: \"The guide informs the public envelope, but stage mass splits, engine stations, ignition limits, and fairing mass remain simulator approximations.\",",
                "representative_rationale: \" \",",
            );
        let error = parse_err(&text);
        assert!(
            error.contains("Representative basis requires nonblank"),
            "{error}"
        );
    }

    #[test]
    fn representative_metadata_accepts_partial_source_details() {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let text = fs::read_to_string(
            Path::new(&manifest)
                .join(CONFIGS_RELATIVE_PATH)
                .join(FALCON9_RON),
        )
        .unwrap();
        assert!(!text.contains("sha256:"));
        assert!(RocketConfigFile::parse(&text).is_ok());
    }

    #[test]
    fn source_verified_groups_require_complete_valid_provenance() {
        let verified = source_verified_electron_definition();
        assert!(RocketConfigFile::parse(&verified).is_ok());

        for (invalid, expected) in [
            (
                verified.replace("manufacturer: \"Rocket Lab\"", "manufacturer: \" \""),
                "provenance.manufacturer",
            ),
            (
                verified.replace("title: \"Electron Payload User's Guide\"", "title: \"\""),
                "primary_source.title",
            ),
            (
                verified.replace("version: \"8.0\"", "version: \"\""),
                "primary_source.version",
            ),
            (
                verified.replace(
                    "publication_date: \"2025-09\"",
                    "publication_date: \"2025-13\"",
                ),
                "publication_date in YYYY-MM or YYYY-MM-DD",
            ),
            (
                verified.replace(
                    "https://rocketlabcorp.com/assets/Rocket-Lab-Electron-Payload-User-Guide-8.0.pdf",
                    "http://rocketlabcorp.com/assets/Rocket-Lab-Electron-Payload-User-Guide-8.0.pdf",
                ),
                "HTTPS",
            ),
            (
                verified.replace(
                    "a212a499a70d44f7bde5b92d163b520558379aa0e71655be72ce936b3eb840f7",
                    "a212a499a70d44f7bde5b92d163b520558379aa0e71655be72ce936b3eb840fF",
                ),
                "lowercase 64-hex",
            ),
        ] {
            let error = parse_err(&invalid);
            assert!(error.contains(expected), "expected {expected:?} in {error}");
        }
    }

    /// The hardcoded Falcon test fixture remains field-equivalent to the
    /// shipped runtime catalog definition (float comparisons tolerate only
    /// N-to-kN division rounding).
    #[test]
    fn falcon9_ron_matches_hardcoded_domain_model() {
        let loaded = load_shipped(FALCON9_RON).rocket;
        let hardcoded = Rocket::falcon9_test_fixture();

        assert_eq!(loaded.name, hardcoded.name);
        assert_eq!(loaded.diameter_m, hardcoded.diameter_m);
        assert_eq!(loaded.height_m, hardcoded.height_m);
        assert_eq!(loaded.stages.len(), hardcoded.stages.len());

        for (loaded_stage, hard_stage) in loaded.stages.iter().zip(hardcoded.stages.iter()) {
            assert_eq!(loaded_stage.name, hard_stage.name);
            assert_eq!(loaded_stage.diameter_m, hard_stage.diameter_m);
            assert_eq!(loaded_stage.height_m, hard_stage.height_m);
            assert!((loaded_stage.dry_mass_kg - hard_stage.dry_mass_kg).abs() < 1e-3);
            assert!((loaded_stage.propellant_mass_kg - hard_stage.propellant_mass_kg).abs() < 1e-3);
            assert_eq!(loaded_stage.engines.len(), hard_stage.engines.len());
            assert_eq!(loaded_stage.landing_gear, hard_stage.landing_gear);
            for (le, he) in loaded_stage.engines.iter().zip(hard_stage.engines.iter()) {
                assert!((le.position_m - he.position_m).length() < 1e-4);
                assert!((le.thrust_axis - he.thrust_axis).length() < 1e-4);
                assert_eq!(le.isp_sea_level, he.isp_sea_level);
                assert_eq!(le.isp_vacuum, he.isp_vacuum);
                assert_eq!(le.gimbal_range_deg, he.gimbal_range_deg);
                assert!((le.rated_thrust_kn - he.rated_thrust_kn).abs() < 1e-2);
                assert_eq!(le.thrust_reference, he.thrust_reference);
                assert_eq!(le.throttle_min, he.throttle_min);
                assert_eq!(le.throttle_max, he.throttle_max);
                assert_eq!(le.max_ignitions, he.max_ignitions);
                assert_eq!(le.ignition_count, 0);
                assert_eq!(le.state, EngineState::Off);
            }
        }

        // Aggregate pins (same assertions as the hardcoded entity tests):
        // 22.2 t dry, 120 t propellant, 142.2 t gross, 7 607 kN liftoff.
        assert!((loaded.total_dry_mass_kg() - 22_200.0).abs() < 1.0);
        assert!((loaded.total_propellant_mass_kg() - 120_000.0).abs() < 1.0);
        assert!((loaded.total_mass_kg() - 142_200.0).abs() < 1.0);
        let rated_thrust_kn: f32 = loaded.stages[0]
            .engines
            .iter()
            .map(|engine| engine.rated_thrust_kn)
            .sum();
        assert!((rated_thrust_kn - 7_607.0).abs() < 1.0);

        // Stage-local geometry: nine engines on a 1.2 m ring at the stage-1
        // lower end (y = -20.6 m), one vacuum engine at the stage-2 lower end
        // (y = -6.6 m).
        assert_eq!(loaded.stages[0].engines.len(), 9);
        for engine in &loaded.stages[0].engines {
            assert!(
                (engine.position_m.x * engine.position_m.x
                    + engine.position_m.z * engine.position_m.z
                    - 1.44)
                    .abs()
                    < 1e-4
            );
            assert!((engine.position_m.y + 20.6).abs() < 1e-4);
        }
        assert_eq!(loaded.stages[1].engines.len(), 1);
        assert!((loaded.stages[1].engines[0].position_m.y + 6.6).abs() < 1e-4);

        // Landing gear pin (Phase 13): four legs on a 4.5 m base radius with
        // a 3.0 m stroke deploying at 100 m AGL, rated for 30 t.
        let legs = load_shipped(FALCON9_RON).rocket.stages[0]
            .landing_gear
            .expect("falcon9 must declare landing_legs");
        assert_eq!(legs.count, 4);
        assert!((legs.base_radius_m - 4.5).abs() < 1e-6);
        assert!((legs.stroke_m - 3.0).abs() < 1e-6);
        assert_eq!(legs.max_landing_mass_kg, Some(30_000.0));
        assert!((legs.deploy_altitude_m - 100.0).abs() < 1e-6);
    }

    #[test]
    fn shipped_fairings_belong_only_to_final_serial_stages() {
        for (file, expected_mass_kg) in [(FALCON9_RON, 1_750.0), ("electron.ron", 50.0)] {
            let loaded = load_shipped(file);
            assert!(loaded
                .rocket
                .stages
                .iter()
                .take(loaded.rocket.stages.len() - 1)
                .all(|stage| stage.fairing_dry_mass_kg.is_none()));
            assert_eq!(
                loaded
                    .rocket
                    .stages
                    .last()
                    .and_then(|stage| stage.fairing_dry_mass_kg),
                Some(expected_mass_kg),
                "{file} fairing must be owned by its final serial stage"
            );
        }
    }

    /// Both gear paths must be exercised by the shipped catalog: falcon9 and
    /// starship carry landing legs, electron and sls deliberately stay
    /// leg-less so the point-contact fallback keeps working.
    #[test]
    fn shipped_catalog_exercises_both_gear_paths() {
        for (file, expected_stage_gear) in [
            ("falcon9.ron", &[true, false][..]),
            ("starship.ron", &[false, true][..]),
            ("electron.ron", &[false, false][..]),
            ("sls.ron", &[false, false][..]),
        ] {
            let loaded = load_shipped(file);
            assert_eq!(
                loaded
                    .rocket
                    .stages
                    .iter()
                    .map(|stage| stage.landing_gear.is_some())
                    .collect::<Vec<_>>(),
                expected_stage_gear,
                "{file} stage-local landing_legs mismatch"
            );
        }
    }

    /// Every file shipped in assets/configs/rockets must parse and validate;
    /// the startup catalog load panics otherwise (fail fast), so this pins
    /// each vehicle definition independently in CI.
    #[test]
    fn all_shipped_vehicle_files_parse_and_validate() {
        let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let dir = Path::new(&manifest).join(CONFIGS_RELATIVE_PATH);
        let entries = fs::read_dir(&dir).expect("shipped config dir exists");
        let mut parsed_files = 0;
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if !path.extension().is_some_and(|ext| ext == "ron") {
                continue;
            }
            let text =
                fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            let vehicles = RocketConfigFile::parse(&text)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert!(
                !vehicles.is_empty(),
                "{} defined no vehicles",
                path.display()
            );
            parsed_files += 1;
        }
        assert!(
            parsed_files >= 4,
            "expected the four shipped vehicle files, found {parsed_files}"
        );
    }

    #[test]
    fn shipped_catalog_pins_retrieved_primary_source_bytes() {
        for (file, expected_url, expected_sha256) in [
            (
                "sls.ron",
                "https://www.nasa.gov/wp-content/uploads/2026/01/sls-5558-artemis-ii-sls-reference-guide-final-review-508-012026.pdf",
                "2f15dbdc7015fab5fb0deb080f49f527f78cb26fbfb767529029dd71f1e34fa3",
            ),
            (
                "electron.ron",
                "https://rocketlabcorp.com/assets/Rocket-Lab-Electron-Payload-User-Guide-8.0.pdf",
                "a212a499a70d44f7bde5b92d163b520558379aa0e71655be72ce936b3eb840f7",
            ),
        ] {
            let manifest = env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
            let text = fs::read_to_string(
                Path::new(&manifest)
                    .join(CONFIGS_RELATIVE_PATH)
                    .join(file),
            )
            .unwrap_or_else(|error| panic!("cannot read {file}: {error}"));
            let vehicle = RocketConfigFile::ron_options()
                .from_str::<RocketConfigFile>(&text)
                .unwrap_or_else(|error| panic!("cannot parse {file}: {error}"))
                .vehicles
                .into_iter()
                .next()
                .expect("shipped file must define one vehicle");
            let source = vehicle
                .provenance
                .primary_source
                .expect("pinned vehicle must declare primary source");
            assert_eq!(source.source_url.as_deref(), Some(expected_url));
            assert_eq!(source.sha256.as_deref(), Some(expected_sha256));
        }
    }

    #[test]
    fn shipped_engine_stations_fit_their_stage_local_cylinders() {
        for file in ["falcon9.ron", "starship.ron", "electron.ron", "sls.ron"] {
            let rocket = load_shipped(file).rocket;
            for stage in &rocket.stages {
                for engine in &stage.engines {
                    assert!(
                        engine.position_m.is_finite(),
                        "{file} has a non-finite station"
                    );
                    assert!(
                        engine.position_m.x.hypot(engine.position_m.z) <= stage.diameter_m * 0.5,
                        "{file} engine outside {} radial envelope",
                        stage.name
                    );
                    assert!(
                        engine.position_m.y.abs() <= stage.height_m * 0.5,
                        "{file} engine outside {} axial envelope",
                        stage.name
                    );
                }
            }
        }
    }

    #[test]
    fn sls_catalog_models_lift_capable_parallel_srb_pair() {
        let mut rocket = load_shipped("sls.ron").rocket;
        for engine in &mut rocket.stages[0].engines {
            engine.state = EngineState::Running;
        }
        let boosters = rocket
            .parallel_boosters
            .as_mut()
            .expect("SLS Block 1 must define its two SRBs");
        for engine in &mut boosters.stage.engines {
            engine.state = EngineState::Running;
        }
        assert_eq!(boosters.count, 2);
        assert_eq!(boosters.attachment_positions_m.len(), 2);
        let core_thrust_n = crate::domain::services::rocket_propulsion::stage_thrust_body(
            &rocket.stages[0].engines,
            1.0,
            crate::domain::services::atmosphere::SEA_LEVEL_PRESSURE_PA,
        )
        .0
        .length();
        let booster_thrust_n = crate::domain::services::rocket_propulsion::stage_thrust_body(
            &boosters.stage.engines,
            1.0,
            crate::domain::services::atmosphere::SEA_LEVEL_PRESSURE_PA,
        )
        .0
        .length()
            * boosters.count as f64;
        let tw_ratio = (core_thrust_n + booster_thrust_n)
            / (rocket.total_mass_kg() as f64
                * crate::domain::services::rocket_propulsion::STANDARD_GRAVITY_MPS2);
        assert!(
            tw_ratio > 1.0,
            "SLS pad T/W must exceed one, got {tw_ratio}"
        );
    }

    #[test]
    fn validation_rejects_invalid_or_asymmetric_parallel_boosters() {
        let sls = fs::read_to_string(
            Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap())
                .join(CONFIGS_RELATIVE_PATH)
                .join("sls.ron"),
        )
        .unwrap();
        assert!(parse_err(&sls.replace("count: 2", "count: 3")).contains("positive even"));
        assert!(
            parse_err(&sls.replace("(6.1, -16.5, 0.0)", "(5.9, -16.5, 0.0)"))
                .contains("overlaps the core")
        );
        assert!(
            parse_err(&sls.replace("(6.1, -16.5, 0.0)", "(6.2, -16.5, 0.0)")).contains("mirrored")
        );
        assert!(parse_err(&sls.replace(
            "name: \"5-Segment SRB\",",
            "name: \"5-Segment SRB\", landing_legs: ( basis: Representative, count: 4, base_radius_m: 4.5, stroke_m: 3.0, deploy_altitude_m: 100.0 ),",
        ))
        .contains("parallel boosters never inherit"));
    }

    #[test]
    fn validation_rejects_nonfinite_stations_outside_envelopes_and_final_reserves() {
        let base = r#"
            ( vehicles: [( name: "Bad", basis: Representative,
                provenance: ( representative_rationale: "test representative values" ),
                diameter_m: 2.0, height_m: 10.0, stages: [(
                name: "S1", basis: Representative, diameter_m: 2.0, height_m: 10.0, dry_mass_kg: 1.0,
                propellant_mass_kg: 10.0, recovery_propellant_reserve_kg: Some(1.0), engines: ( basis: Representative, values: [(
                    position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                    isp_sl: 200.0, isp_vac: 250.0, gimbal_range_deg: 5.0,
                    rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1
                )] )
            )] )] )
        "#;
        assert!(parse_err(base).contains("final stage"));
        let without_reserve = base.replace("recovery_propellant_reserve_kg: Some(1.0), ", "");
        assert!(parse_err(
            &without_reserve.replace("position: (0.0, 0.0, 0.0)", "position: (1.1, 0.0, 0.0)")
        )
        .contains("radial"));
        assert!(parse_err(
            &without_reserve.replace("position: (0.0, 0.0, 0.0)", "position: (0.0, 5.1, 0.0)")
        )
        .contains("half-height"));
        assert!(parse_err(
            &without_reserve.replace("rated_thrust_n: 1000.0", "rated_thrust_n: NaN")
        )
        .contains("rated_thrust_n"));
    }

    #[test]
    fn minimal_definition_requires_an_explicit_ignition_budget() {
        let text = r#"
            (
                vehicles: [(
                    name: "Test Rocket",
                    basis: Representative,
                    provenance: ( representative_rationale: "test representative values" ),
                    diameter_m: 1.0,
                    height_m: 10.0,
                    stages: [(
                        name: "S1",
                        basis: Representative,
                        diameter_m: 1.0,
                        height_m: 10.0,
                        dry_mass_kg: 100.0,
                        propellant_mass_kg: 900.0,
                        engines: ( basis: Representative, values: [(
                            position: (0.0, -5.0, 0.0),
                            thrust_axis: (0.0, 1.0, 0.0),
                            isp_sl: 250.0,
                            isp_vac: 300.0,
                            gimbal_range_deg: 6.0,
                            rated_thrust_n: 100_000.0,
                            thrust_reference: SeaLevel,
                            max_ignitions: 1,
                        )] ),
                    )],
                )]
            )
        "#;
        let vehicles = RocketConfigFile::parse(text).expect("valid minimal definition");
        assert_eq!(vehicles.len(), 1);
        let loaded = &vehicles[0];
        let engine = &loaded.rocket.stages[0].engines[0];
        assert_eq!(engine.rated_thrust_kn, 100.0);
        assert_eq!(engine.thrust_reference, ThrustReference::SeaLevel);
        assert_eq!(engine.throttle_min, 0.0);
        assert_eq!(engine.throttle_max, 1.0);
        assert_eq!(engine.max_ignitions, 1);
        assert_eq!(engine.ignition_count, 0);
        assert_eq!(engine.state, EngineState::Off);
        assert!(loaded.rocket.stages[0].fairing_dry_mass_kg.is_none());
        assert!(
            loaded.rocket.stages[0].landing_gear.is_none(),
            "no legs declared → none"
        );
        assert!(parse_err(&text.replace("max_ignitions: 1,", "")).contains("RON parse error"));
    }

    #[test]
    fn engine_thrust_reference_is_required_and_legacy_field_is_rejected() {
        let complete = r#"
            ( vehicles: [( name: "Test", basis: Representative,
                provenance: ( representative_rationale: "test representative values" ),
                diameter_m: 1.0, height_m: 2.0, stages: [(
                name: "S1", basis: Representative, diameter_m: 1.0, height_m: 2.0, dry_mass_kg: 1.0,
                propellant_mass_kg: 1.0, engines: ( basis: Representative, values: [(
                    position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                    isp_sl: 200.0, isp_vac: 250.0, gimbal_range_deg: 0.0,
                    rated_thrust_n: 1_000.0, thrust_reference: Vacuum, max_ignitions: 1
                )] )
            )] )] )
        "#;
        assert!(RocketConfigFile::parse(complete).is_ok());
        assert!(
            parse_err(&complete.replace(", thrust_reference: Vacuum", ""))
                .contains("RON parse error")
        );
        assert!(
            parse_err(&complete.replace("rated_thrust_n", "max_thrust_n"))
                .contains("RON parse error")
        );
    }

    #[test]
    fn stage_landing_legs_load_with_explicit_values() {
        let text = r#"
            (
                vehicles: [(
                    name: "Legged",
                    basis: Representative,
                    provenance: ( representative_rationale: "test representative values" ),
                    diameter_m: 3.7,
                    height_m: 70.0,
                    stages: [(
                        name: "S1",
                        basis: Representative,
                        diameter_m: 3.7,
                        height_m: 70.0,
                        dry_mass_kg: 500.0,
                        propellant_mass_kg: 1_000.0,
                        landing_legs: (
                            basis: Representative,
                            count: 4,
                            base_radius_m: 4.5,
                            stroke_m: 3.0,
                            max_landing_mass_kg: 30_000.0,
                            deploy_altitude_m: 100.0,
                        ),
                        engines: ( basis: Representative, values: [(
                            position: (0.0, -5.0, 0.0),
                            thrust_axis: (0.0, 1.0, 0.0),
                            isp_sl: 250.0,
                            isp_vac: 300.0,
                            gimbal_range_deg: 6.0,
                            rated_thrust_n: 200_000.0,
                            thrust_reference: SeaLevel,
                            max_ignitions: 1,
                        )] ),
                    )],
                )]
            )
        "#;
        let vehicles = RocketConfigFile::parse(text).expect("legged definition");
        let legs = vehicles[0].rocket.stages[0]
            .landing_gear
            .expect("legs must load");
        assert_eq!(legs.count, 4);
        assert!((legs.base_radius_m - 4.5).abs() < 1e-9);

        // Omitted max_landing_mass_kg stays None (= whole vehicle).
        let without_limit =
            RocketConfigFile::parse(&text.replace("max_landing_mass_kg: 30_000.0,", ""))
                .expect("definition without mass limit");
        assert_eq!(
            without_limit[0].rocket.stages[0]
                .landing_gear
                .unwrap()
                .max_landing_mass_kg,
            None
        );
    }

    #[test]
    fn final_serial_stage_may_have_gear_but_never_a_recovery_reserve() {
        let text = r#"
            ( vehicles: [( name: "Final gear", basis: Representative,
                provenance: ( representative_rationale: "test representative values" ),
                diameter_m: 1.0, height_m: 10.0, stages: [(
                name: "S1", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 100.0,
                propellant_mass_kg: 900.0,
                landing_legs: ( basis: Representative, count: 4, base_radius_m: 1.5, stroke_m: 1.0, deploy_altitude_m: 100.0 ),
                engines: ( basis: Representative, values: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                    isp_sl: 250.0, isp_vac: 300.0, gimbal_range_deg: 5.0,
                    rated_thrust_n: 100_000.0, thrust_reference: SeaLevel, max_ignitions: 1
                )] )
            )] )] )
        "#;
        let loaded = RocketConfigFile::parse(text).expect("final-stage gear is valid");
        assert!(loaded[0].rocket.stages[0].landing_gear.is_some());
        assert!(parse_err(&text.replace(
            "landing_legs:",
            "recovery_propellant_reserve_kg: Some(10.0), landing_legs:",
        ))
        .contains("final stage cannot declare recovery_propellant_reserve_kg"));
    }

    #[test]
    fn fairing_must_be_unique_and_belong_to_the_final_serial_stage() {
        let text = r#"
            ( vehicles: [( name: "Fairing test", basis: Representative,
                provenance: ( representative_rationale: "test representative values" ),
                diameter_m: 1.0, height_m: 20.0, stages: [
                ( name: "S1", basis: Representative, diameter_m: 1.0, height_m: 10.0,
                  dry_mass_kg: 100.0, propellant_mass_kg: 900.0,
                  engines: ( basis: Representative, values: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                    isp_sl: 250.0, isp_vac: 300.0, gimbal_range_deg: 5.0,
                    rated_thrust_n: 100_000.0, thrust_reference: SeaLevel, max_ignitions: 1
                  )] ) ),
                ( name: "S2", basis: Representative, diameter_m: 1.0, height_m: 10.0,
                  dry_mass_kg: 100.0, propellant_mass_kg: 900.0,
                  fairing: ( basis: Representative, dry_mass_kg: 25.0 ),
                  engines: ( basis: Representative, values: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                    isp_sl: 250.0, isp_vac: 300.0, gimbal_range_deg: 5.0,
                    rated_thrust_n: 100_000.0, thrust_reference: Vacuum, max_ignitions: 1
                  )] ) )
                ] )] )
        "#;
        let loaded = RocketConfigFile::parse(text).expect("final fairing is valid");
        assert_eq!(loaded[0].rocket.stages[1].fairing_dry_mass_kg, Some(25.0));
        let multiple = text.replacen(
            "engines:",
            "fairing: ( basis: Representative, dry_mass_kg: 20.0 ), engines:",
            1,
        );
        assert!(parse_err(&multiple).contains("only one serial stage"));
        let misplaced = text
            .replace("fairing: ( basis: Representative, dry_mass_kg: 25.0 ),", "")
            .replacen(
                "engines:",
                "fairing: ( basis: Representative, dry_mass_kg: 20.0 ), engines:",
                1,
            );
        assert!(parse_err(&misplaced).contains("final serial stage"));
    }

    #[test]
    fn vehicle_level_landing_legs_are_rejected_without_an_alias() {
        let text = r#"
            ( vehicles: [( name: "Bad", diameter_m: 1.0, height_m: 10.0,
                landing_legs: ( count: 4, base_radius_m: 1.5, stroke_m: 1.0, deploy_altitude_m: 100.0 ),
                stages: [( name: "S1", diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 100.0,
                    propellant_mass_kg: 900.0, engines: [(
                        position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0),
                        isp_sl: 250.0, isp_vac: 300.0, gimbal_range_deg: 5.0,
                        rated_thrust_n: 100_000.0, thrust_reference: SeaLevel, max_ignitions: 1
                    )] )] )] )
        "#;
        assert!(parse_err(text).contains("RON parse error"));
    }

    #[test]
    fn invalid_definitions_fail_with_clear_errors() {
        let base = |body: &str| {
            format!(
                "( vehicles: [( name: \"Bad\", basis: Representative, provenance: ( representative_rationale: \"test representative values\" ), diameter_m: 3.0, height_m: 30.0, stages: [{body}] )] )"
            )
        };
        // No stages.
        let text = "( vehicles: [( name: \"Bad\", basis: Representative, provenance: ( representative_rationale: \"test representative values\" ), diameter_m: 3.0, height_m: 30.0, stages: [] )] )";
        assert!(parse_err(text).contains("at least one stage"));

        // Negative mass.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: -1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
               isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )",
        ));
        assert!(err.contains("dry_mass_kg"), "{err}");

        // Stage exterior dimensions feed active-stage aero/inertia, so they
        // must be physical rather than inferred from a whole-stack fallback.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 0.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
               isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )",
        ));
        assert!(err.contains("diameter_m and height_m"), "{err}");

        // Non-positive ISP.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 0.0, \
               isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )",
        ));
        assert!(err.contains("isp"), "{err}");

        // Gimbal range above the sanity ceiling.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
               isp_vac: 250.0, gimbal_range_deg: 45.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )",
        ));
        assert!(err.contains("gimbal_range_deg"), "{err}");

        // No engines in a stage.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [] ) )",
        ));
        assert!(err.contains("at least one engine"), "{err}");

        // Inverted throttle bounds.
        let err = parse_err(&base(
            "( name: \"S1\", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: ( basis: Representative, values: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
               isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1, \
              throttle_min: 0.9, throttle_max: 0.1 )] ) )",
        ));
        assert!(err.contains("throttle"), "{err}");

        // Unknown fields are rejected (typo protection).
        let err = parse_err(
            "( vehicles: [( nam: \"Typo\", diameter_m: 3.0, height_m: 30.0, stages: [] )] )",
        );
        assert!(err.contains("RON parse error"), "{err}");

        // Landing legs: too few for a stable stance.
        let err = parse_err(
            r#"( vehicles: [( name: "Bad", basis: Representative, provenance: ( representative_rationale: "test representative values" ), diameter_m: 3.7, height_m: 70.0,
                stages: [( name: "S1", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0,
                    landing_legs: ( basis: Representative, count: 2, base_radius_m: 4.5, stroke_m: 3.0, deploy_altitude_m: 100.0 ), engines: ( basis: Representative, values: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0,
                    isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )] )] )"#,
        );
        assert!(err.contains("count"), "{err}");

        // Invalid fields inside stage-local landing_legs retain schema guards.
        let err = parse_err(
            r#"( vehicles: [( name: "Bad", basis: Representative, provenance: ( representative_rationale: "test representative values" ), diameter_m: 3.7, height_m: 70.0,
                stages: [( name: "S1", basis: Representative, diameter_m: 1.0, height_m: 10.0, dry_mass_kg: 1.0, propellant_mass_kg: 10.0,
                    landing_legs: ( basis: Representative, count: 4, base_radius_m: -1.0, stroke_m: 3.0, deploy_altitude_m: 100.0 ), engines: ( basis: Representative, values: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0,
                    isp_vac: 250.0, gimbal_range_deg: 5.0, rated_thrust_n: 1000.0, thrust_reference: SeaLevel, max_ignitions: 1 )] ) )] )] )"#,
        );
        assert!(err.contains("base_radius_m"), "{err}");
    }

    #[test]
    fn catalog_keys_are_deterministic_and_sorted() {
        let mut catalog = RocketCatalog::default();
        catalog.insert(
            "sls",
            LoadedVehicle {
                rocket: Rocket {
                    name: "SLS".into(),
                    diameter_m: 5.0,
                    height_m: 98.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
            },
        );
        catalog.insert(
            "electron",
            LoadedVehicle {
                rocket: Rocket {
                    name: "Electron".into(),
                    diameter_m: 1.2,
                    height_m: 18.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
            },
        );
        catalog.insert(
            "falcon9",
            LoadedVehicle {
                rocket: Rocket {
                    name: "Falcon 9".into(),
                    diameter_m: 3.7,
                    height_m: 70.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
            },
        );
        let keys: Vec<&String> = catalog.keys().collect();
        assert_eq!(keys, ["electron", "falcon9", "sls"]);
        assert_eq!(catalog.first_key(), Some("electron"));
        assert!(catalog.get("falcon9").is_some());
        assert!(catalog.get("unknown").is_none());
    }
}
