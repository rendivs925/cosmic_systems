//! Data-driven rocket vehicle definitions (AGENTS.md sections 39-40).
//!
//! Vehicles are described in RON files under `assets/configs/rockets/*.ron`
//! and converted once at load into the domain [`Rocket`] model. This module
//! owns the file schema and validation; the domain structs stay free of any
//! serialization concerns. Loading fails fast with a clear error for invalid
//! configuration (AGENTS.md section 65).
//!
//! The implementation is split into cohesive submodules: [`schema`] (RON
//! definitions and serde helpers), [`validation`] (the typed error and the
//! definition validator), and [`catalog`] (directory discovery, keys, and
//! selection).

mod catalog;
mod schema;
mod validation;

pub use catalog::{RocketCatalog, VehicleKey, VehicleSelection, CONFIGS_RELATIVE_PATH};
pub use schema::{
    DataBasis, EngineDef, EngineGroupDef, FairingDef, LandingLegsDef, ParallelBoostersDef,
    PrimarySource, RocketConfigFile, StageDef, ThrustReferenceDef, VehicleDef, VehicleProvenance,
};
pub use validation::{LoadedVehicle, RocketConfigError};

/// Sanity ceiling for engine gimbal ranges, degrees.
pub const MAX_GIMBAL_RANGE_DEG: f32 = 15.0;

/// Newtons per kilonewton: the file format uses N, the domain uses kN.
const NEWTONS_PER_KN: f32 = 1000.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::rocket_config::catalog::DEFAULT_VEHICLE_KEY;
    use crate::domain::entities::rocket::{EngineState, Rocket, ThrustReference};
    use std::env;
    use std::fs;
    use std::path::Path;

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
        assert_eq!(boosters.count(), 2);
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
            * boosters.count() as f64;
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
            VehicleKey::from("sls"),
            LoadedVehicle {
                rocket: Rocket {
                    name: "SLS".into(),
                    diameter_m: 5.0,
                    height_m: 98.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
                configuration_sha256: String::new(),
            },
        );
        catalog.insert(
            VehicleKey::from("electron"),
            LoadedVehicle {
                rocket: Rocket {
                    name: "Electron".into(),
                    diameter_m: 1.2,
                    height_m: 18.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
                configuration_sha256: String::new(),
            },
        );
        catalog.insert(
            VehicleKey::from("falcon9"),
            LoadedVehicle {
                rocket: Rocket {
                    name: "Falcon 9".into(),
                    diameter_m: 3.7,
                    height_m: 70.0,
                    stages: vec![],
                    parallel_boosters: None,
                },
                configuration_sha256: String::new(),
            },
        );
        let keys: Vec<&str> = catalog.keys().collect();
        assert_eq!(keys, ["electron", "falcon9", "sls"]);
        assert!(catalog
            .resolve(&VehicleSelection::Default)
            .is_some_and(|(key, _)| key == DEFAULT_VEHICLE_KEY));
        assert!(catalog
            .resolve(&VehicleSelection::Requested(VehicleKey::from("falcon9")))
            .is_some());
        assert!(catalog
            .resolve(&VehicleSelection::Requested(VehicleKey::from("unknown")))
            .is_none());
    }
}
