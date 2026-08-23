//! Data-driven rocket vehicle definitions (AGENTS.md sections 39-40).
//!
//! Vehicles are described in RON files under `assets/configs/rockets/*.ron`
//! and converted once at load into the domain [`Rocket`] model. This module
//! owns the file schema and validation; the domain structs stay free of any
//! serialization concerns. Loading fails fast with a clear error for invalid
//! configuration (AGENTS.md section 65).

use crate::domain::entities::rocket::{EngineState, Rocket, RocketEngine, RocketStage};
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

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RocketConfigFile {
    pub vehicles: Vec<VehicleDef>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VehicleDef {
    pub name: String,
    pub diameter_m: f32,
    pub height_m: f32,
    #[serde(default)]
    pub stages: Vec<StageDef>,
    #[serde(default)]
    pub fairing: Option<FairingDef>,
    /// Optional deployable landing gear. Vehicles without this field land
    /// gear-less via the point-contact model.
    #[serde(default)]
    pub landing_legs: Option<LandingLegsDef>,
}

/// Per-vehicle landing-gear definition. Additive schema: existing files
/// without a `landing_legs` block stay valid.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LandingLegsDef {
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

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StageDef {
    pub name: String,
    pub dry_mass_kg: f32,
    pub propellant_mass_kg: f32,
    #[serde(default)]
    pub engines: Vec<EngineDef>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineDef {
    /// Body-frame position, meters (+Y longitudinal, nose-up).
    pub position: [f32; 3],
    /// Body-frame thrust unit axis.
    pub thrust_axis: [f32; 3],
    pub isp_sl: f32,
    pub isp_vac: f32,
    pub gimbal_range_deg: f32,
    /// Vacuum-referenced maximum thrust in newtons (converted to kN at load).
    pub max_thrust_n: f32,
    #[serde(default)]
    pub throttle_min: f32,
    #[serde(default = "default_throttle_max")]
    pub throttle_max: f32,
    #[serde(default = "default_restartable")]
    pub restartable: bool,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FairingDef {
    pub dry_mass_kg: f32,
}

fn default_throttle_max() -> f32 {
    1.0
}

fn default_restartable() -> bool {
    true
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

/// One vehicle ready for ECS spawning: the domain model plus whether a
/// payload fairing rides on top and optional landing-gear configuration.
#[derive(Debug, Clone)]
pub struct LoadedVehicle {
    pub rocket: Rocket,
    pub fairing_dry_mass_kg: Option<f32>,
    pub landing_legs: Option<LandingGearSpec>,
}

impl VehicleDef {
    /// Fail-fast validation of physical plausibility (AGENTS.md section 65).
    pub fn validate(&self) -> Result<(), RocketConfigError> {
        if self.name.trim().is_empty() {
            return Err(self.invalid("vehicle name must not be empty"));
        }
        if self.diameter_m <= 0.0 || self.height_m <= 0.0 {
            return Err(self.invalid("needs positive diameter_m and height_m"));
        }
        if self.stages.is_empty() {
            return Err(self.invalid("needs at least one stage"));
        }
        for (i, stage) in self.stages.iter().enumerate() {
            if stage.name.trim().is_empty() {
                return Err(self.invalid(format!("stage {i}: name must not be empty")));
            }
            let at = self.stage_context(i, stage);
            if stage.dry_mass_kg <= 0.0 {
                return Err(self.invalid(format!("{at}: dry_mass_kg must be > 0")));
            }
            if !stage.engines.is_empty() && stage.propellant_mass_kg <= 0.0 {
                return Err(
                    self.invalid(format!("{at}: carries engines but propellant_mass_kg <= 0"))
                );
            }
            if stage.engines.is_empty() {
                return Err(self.invalid(format!("{at}: needs at least one engine")));
            }
            for (e, engine) in stage.engines.iter().enumerate() {
                self.validate_engine(&format!("{at} engine {e}"), engine)?;
            }
        }
        if let Some(fairing) = &self.fairing {
            if fairing.dry_mass_kg <= 0.0 {
                return Err(self.invalid("fairing dry_mass_kg must be > 0"));
            }
        }
        if let Some(legs) = &self.landing_legs {
            if legs.count < 3 {
                return Err(self.invalid("landing_legs count must be >= 3 for a stable stance"));
            }
            if legs.base_radius_m <= 0.0 {
                return Err(self.invalid("landing_legs base_radius_m must be > 0"));
            }
            if legs.stroke_m <= 0.0 {
                return Err(self.invalid("landing_legs stroke_m must be > 0"));
            }
            if legs.deploy_altitude_m <= 0.0 {
                return Err(self.invalid("landing_legs deploy_altitude_m must be > 0"));
            }
            if legs.max_landing_mass_kg.is_some_and(|m| m <= 0.0) {
                return Err(self.invalid("landing_legs max_landing_mass_kg must be > 0"));
            }
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

    fn validate_engine(&self, at: &str, engine: &EngineDef) -> Result<(), RocketConfigError> {
        if engine.isp_sl <= 0.0 || engine.isp_vac <= 0.0 {
            return Err(self.invalid(format!("{at}: isp_sl and isp_vac must be > 0")));
        }
        if !(0.0..=MAX_GIMBAL_RANGE_DEG).contains(&engine.gimbal_range_deg) {
            return Err(self.invalid(format!(
                "{at}: gimbal_range_deg must be within [0, {MAX_GIMBAL_RANGE_DEG}]"
            )));
        }
        if engine.max_thrust_n <= 0.0 {
            return Err(self.invalid(format!("{at}: max_thrust_n must be > 0")));
        }
        if !(0.0..=1.0).contains(&engine.throttle_min)
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
        if !Vec3::from_array(engine.position).is_finite() {
            return Err(self.invalid(format!("{at}: position must be finite")));
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
                dry_mass_kg: stage.dry_mass_kg,
                propellant_mass_kg: stage.propellant_mass_kg,
                engines: stage
                    .engines
                    .iter()
                    .map(|engine| RocketEngine {
                        position_m: Vec3::from_array(engine.position),
                        thrust_axis: Vec3::from_array(engine.thrust_axis).normalize_or_zero(),
                        isp_sea_level: engine.isp_sl,
                        isp_vacuum: engine.isp_vac,
                        gimbal_range_deg: engine.gimbal_range_deg,
                        max_thrust_kn: engine.max_thrust_n / NEWTONS_PER_KN,
                        throttle_min: engine.throttle_min,
                        throttle_max: engine.throttle_max,
                        restartable: engine.restartable,
                        state: EngineState::Running,
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
            },
            fairing_dry_mass_kg: self.fairing.as_ref().map(|f| f.dry_mass_kg),
            landing_legs: self.landing_legs.as_ref().map(|legs| LandingGearSpec {
                count: legs.count,
                base_radius_m: legs.base_radius_m as f64,
                stroke_m: legs.stroke_m as f64,
                max_landing_mass_kg: legs.max_landing_mass_kg.map(|m| m as f64),
                deploy_altitude_m: legs.deploy_altitude_m as f64,
            }),
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
    /// RON parse options: IMPLICIT_SOME lets files write `fairing: ( ... )`
    /// instead of the noisier `Some(( ... ))`.
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

    /// Parse a definition expected to be invalid; returns the user-facing
    /// error message for substring assertions.
    #[cfg(test)]
    fn parse_err(text: &str) -> String {
        RocketConfigFile::parse(text)
            .expect_err("definition should fail validation")
            .to_string()
    }

    /// Regression pin: the shipped falcon9.ron must stay equivalent to the
    /// hardcoded `Rocket::falcon9()` (field-by-field; float comparisons are
    /// tolerant only to N→kN division rounding).
    #[test]
    fn falcon9_ron_matches_hardcoded_domain_model() {
        let loaded = load_shipped(FALCON9_RON).rocket;
        let hardcoded = Rocket::falcon9();

        assert_eq!(loaded.name, hardcoded.name);
        assert_eq!(loaded.diameter_m, hardcoded.diameter_m);
        assert_eq!(loaded.height_m, hardcoded.height_m);
        assert_eq!(loaded.stages.len(), hardcoded.stages.len());

        for (loaded_stage, hard_stage) in loaded.stages.iter().zip(hardcoded.stages.iter()) {
            assert_eq!(loaded_stage.name, hard_stage.name);
            assert!((loaded_stage.dry_mass_kg - hard_stage.dry_mass_kg).abs() < 1e-3);
            assert!((loaded_stage.propellant_mass_kg - hard_stage.propellant_mass_kg).abs() < 1e-3);
            assert_eq!(loaded_stage.engines.len(), hard_stage.engines.len());
            for (le, he) in loaded_stage.engines.iter().zip(hard_stage.engines.iter()) {
                assert!((le.position_m - he.position_m).length() < 1e-4);
                assert!((le.thrust_axis - he.thrust_axis).length() < 1e-4);
                assert_eq!(le.isp_sea_level, he.isp_sea_level);
                assert_eq!(le.isp_vacuum, he.isp_vacuum);
                assert_eq!(le.gimbal_range_deg, he.gimbal_range_deg);
                assert!((le.max_thrust_kn - he.max_thrust_kn).abs() < 1e-2);
                assert_eq!(le.throttle_min, he.throttle_min);
                assert_eq!(le.throttle_max, he.throttle_max);
                assert_eq!(le.restartable, he.restartable);
                assert_eq!(le.state, EngineState::Running);
            }
        }

        // Aggregate pins (same assertions as the hardcoded entity tests):
        // 22.2 t dry, 120 t propellant, 142.2 t gross, 7 607 kN liftoff.
        assert!((loaded.total_dry_mass_kg() - 22_200.0).abs() < 1.0);
        assert!((loaded.total_propellant_mass_kg() - 120_000.0).abs() < 1.0);
        assert!((loaded.total_mass_kg() - 142_200.0).abs() < 1.0);
        assert!((loaded.max_thrust_kn() - 7_607.0).abs() < 1.0);

        // Stage geometry: nine engines on a 1.2 m ring at y = -32 m, one
        // vacuum engine at y = +12 m.
        assert_eq!(loaded.stages[0].engines.len(), 9);
        for engine in &loaded.stages[0].engines {
            assert!(
                (engine.position_m.x * engine.position_m.x
                    + engine.position_m.z * engine.position_m.z
                    - 1.44)
                    .abs()
                    < 1e-4
            );
            assert!((engine.position_m.y + 32.0).abs() < 1e-4);
        }
        assert_eq!(loaded.stages[1].engines.len(), 1);
        assert!((loaded.stages[1].engines[0].position_m.y - 12.0).abs() < 1e-4);

        // Landing gear pin (Phase 13): four legs on a 4.5 m base radius with
        // a 3.0 m stroke deploying at 100 m AGL, rated for 30 t.
        let legs = load_shipped(FALCON9_RON)
            .landing_legs
            .expect("falcon9 must declare landing_legs");
        assert_eq!(legs.count, 4);
        assert!((legs.base_radius_m - 4.5).abs() < 1e-6);
        assert!((legs.stroke_m - 3.0).abs() < 1e-6);
        assert_eq!(legs.max_landing_mass_kg, Some(30_000.0));
        assert!((legs.deploy_altitude_m - 100.0).abs() < 1e-6);
    }

    #[test]
    fn falcon9_ron_declares_a_fairing() {
        let loaded = load_shipped(FALCON9_RON);
        assert!(loaded.fairing_dry_mass_kg.is_some());
        assert!(loaded.fairing_dry_mass_kg.unwrap() > 0.0);
    }

    /// Both gear paths must be exercised by the shipped catalog: falcon9 and
    /// starship carry landing legs, electron and sls deliberately stay
    /// leg-less so the point-contact fallback keeps working.
    #[test]
    fn shipped_catalog_exercises_both_gear_paths() {
        for (file, expect_legs) in [
            ("falcon9.ron", true),
            ("starship.ron", true),
            ("electron.ron", false),
            ("sls.ron", false),
        ] {
            let loaded = load_shipped(file);
            assert_eq!(
                loaded.landing_legs.is_some(),
                expect_legs,
                "{file} landing_legs mismatch"
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
    fn minimal_definition_loads_with_defaults() {
        let text = r#"
            (
                vehicles: [(
                    name: "Test Rocket",
                    diameter_m: 1.0,
                    height_m: 10.0,
                    stages: [(
                        name: "S1",
                        dry_mass_kg: 100.0,
                        propellant_mass_kg: 900.0,
                        engines: [(
                            position: (0.0, -5.0, 0.0),
                            thrust_axis: (0.0, 1.0, 0.0),
                            isp_sl: 250.0,
                            isp_vac: 300.0,
                            gimbal_range_deg: 6.0,
                            max_thrust_n: 100_000.0,
                        )],
                    )],
                )]
            )
        "#;
        let vehicles = RocketConfigFile::parse(text).expect("valid minimal definition");
        assert_eq!(vehicles.len(), 1);
        let loaded = &vehicles[0];
        let engine = &loaded.rocket.stages[0].engines[0];
        assert_eq!(engine.max_thrust_kn, 100.0);
        assert_eq!(engine.throttle_min, 0.0);
        assert_eq!(engine.throttle_max, 1.0);
        assert!(engine.restartable);
        assert!(loaded.fairing_dry_mass_kg.is_none());
        assert!(loaded.landing_legs.is_none(), "no legs declared → none");
    }

    #[test]
    fn landing_legs_load_with_explicit_values() {
        let text = r#"
            (
                vehicles: [(
                    name: "Legged",
                    diameter_m: 3.7,
                    height_m: 70.0,
                    landing_legs: (
                        count: 4,
                        base_radius_m: 4.5,
                        stroke_m: 3.0,
                        max_landing_mass_kg: 30_000.0,
                        deploy_altitude_m: 100.0,
                    ),
                    stages: [(
                        name: "S1",
                        dry_mass_kg: 500.0,
                        propellant_mass_kg: 1_000.0,
                        engines: [(
                            position: (0.0, -5.0, 0.0),
                            thrust_axis: (0.0, 1.0, 0.0),
                            isp_sl: 250.0,
                            isp_vac: 300.0,
                            gimbal_range_deg: 6.0,
                            max_thrust_n: 200_000.0,
                        )],
                    )],
                )]
            )
        "#;
        let vehicles = RocketConfigFile::parse(text).expect("legged definition");
        let legs = vehicles[0].landing_legs.expect("legs must load");
        assert_eq!(legs.count, 4);
        assert!((legs.base_radius_m - 4.5).abs() < 1e-9);

        // Omitted max_landing_mass_kg stays None (= whole vehicle).
        let without_limit =
            RocketConfigFile::parse(&text.replace("max_landing_mass_kg: 30_000.0,", ""))
                .expect("definition without mass limit");
        assert_eq!(
            without_limit[0].landing_legs.unwrap().max_landing_mass_kg,
            None
        );
    }

    #[test]
    fn invalid_definitions_fail_with_clear_errors() {
        let base = |body: &str| {
            format!(
                "( vehicles: [( name: \"Bad\", diameter_m: 3.0, height_m: 30.0, stages: [{body}] )] )"
            )
        };
        // No stages.
        let text = "( vehicles: [( name: \"Bad\", diameter_m: 3.0, height_m: 30.0, stages: [] )] )";
        assert!(parse_err(text).contains("at least one stage"));

        // Negative mass.
        let err = parse_err(&base(
            "( name: \"S1\", dry_mass_kg: -1.0, propellant_mass_kg: 10.0, engines: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
             isp_vac: 250.0, gimbal_range_deg: 5.0, max_thrust_n: 1000.0 )] )",
        ));
        assert!(err.contains("dry_mass_kg"), "{err}");

        // Non-positive ISP.
        let err = parse_err(&base(
            "( name: \"S1\", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 0.0, \
             isp_vac: 250.0, gimbal_range_deg: 5.0, max_thrust_n: 1000.0 )] )",
        ));
        assert!(err.contains("isp"), "{err}");

        // Gimbal range above the sanity ceiling.
        let err = parse_err(&base(
            "( name: \"S1\", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
             isp_vac: 250.0, gimbal_range_deg: 45.0, max_thrust_n: 1000.0 )] )",
        ));
        assert!(err.contains("gimbal_range_deg"), "{err}");

        // No engines in a stage.
        let err = parse_err(&base(
            "( name: \"S1\", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [] )",
        ));
        assert!(err.contains("at least one engine"), "{err}");

        // Inverted throttle bounds.
        let err = parse_err(&base(
            "( name: \"S1\", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [( \
             position: (0.0, 0.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0, \
             isp_vac: 250.0, gimbal_range_deg: 5.0, max_thrust_n: 1000.0, \
             throttle_min: 0.9, throttle_max: 0.1 )] )",
        ));
        assert!(err.contains("throttle"), "{err}");

        // Unknown fields are rejected (typo protection).
        let err = parse_err(
            "( vehicles: [( nam: \"Typo\", diameter_m: 3.0, height_m: 30.0, stages: [] )] )",
        );
        assert!(err.contains("RON parse error"), "{err}");

        // Landing legs: too few for a stable stance.
        let err = parse_err(
            r#"( vehicles: [( name: "Bad", diameter_m: 3.7, height_m: 70.0,
                landing_legs: ( count: 2, base_radius_m: 4.5, stroke_m: 3.0, deploy_altitude_m: 100.0 ),
                stages: [( name: "S1", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0,
                    isp_vac: 250.0, gimbal_range_deg: 5.0, max_thrust_n: 1000.0 )] )] )] )"#,
        );
        assert!(err.contains("count"), "{err}");

        // Unknown field inside landing_legs is rejected (schema typo guard).
        let err = parse_err(
            r#"( vehicles: [( name: "Bad", diameter_m: 3.7, height_m: 70.0,
                landing_legs: ( count: 4, base_radius_m: -1.0, stroke_m: 3.0, deploy_altitude_m: 100.0 ),
                stages: [( name: "S1", dry_mass_kg: 1.0, propellant_mass_kg: 10.0, engines: [(
                    position: (0.0, -5.0, 0.0), thrust_axis: (0.0, 1.0, 0.0), isp_sl: 200.0,
                    isp_vac: 250.0, gimbal_range_deg: 5.0, max_thrust_n: 1000.0 )] )] )] )"#,
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
                },
                fairing_dry_mass_kg: None,
                landing_legs: None,
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
                },
                fairing_dry_mass_kg: None,
                landing_legs: None,
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
                },
                fairing_dry_mass_kg: None,
                landing_legs: None,
            },
        );
        let keys: Vec<&String> = catalog.keys().collect();
        assert_eq!(keys, ["electron", "falcon9", "sls"]);
        assert_eq!(catalog.first_key(), Some("electron"));
        assert!(catalog.get("falcon9").is_some());
        assert!(catalog.get("unknown").is_none());
    }
}
