//! Vehicle-definition validation and the typed config error.

use super::schema::{
    is_https_url, is_iso_publication_date, DataBasis, EngineDef, LandingLegsDef, StageDef,
    VehicleDef,
};
use super::MAX_GIMBAL_RANGE_DEG;
use crate::domain::entities::rocket::{ParallelBoosters, Rocket, RocketStage};
use crate::domain::services::landing_gear::LandingGearSpec;
use crate::domain::services::simulation_run::is_lowercase_sha256;
use bevy::math::Vec3;
use ron::error::SpannedError;
use std::fmt;
use std::path::PathBuf;

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

/// One vehicle ready for ECS spawning.
#[derive(Debug, Clone)]
pub struct LoadedVehicle {
    pub rocket: Rocket,
    /// SHA-256 of the exact local RON bytes parsed into `rocket`.
    pub configuration_sha256: String,
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
                    .map(EngineDef::to_domain)
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
                    ParallelBoosters::new(
                        RocketStage {
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
                                .map(EngineDef::to_domain)
                                .collect(),
                        },
                        boosters
                            .attachment_positions
                            .iter()
                            .map(|position| Vec3::from_array(*position))
                            .collect(),
                    )
                }),
            },
            configuration_sha256: String::new(),
        }
    }
}
