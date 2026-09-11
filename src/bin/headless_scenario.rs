//! Run a bounded rocket scenario without initializing presentation systems.

use cosmic_systems_wasm::application::headless_scenario::{
    run_headless_scenario_artifact, HeadlessScenario,
};
use cosmic_systems_wasm::domain::services::simulation_analysis::analyze_simulation_artifact;
use cosmic_systems_wasm::domain::services::simulation_artifact::SimulationArtifact;

fn main() {
    let mut raw_arguments = std::env::args().skip(1).collect::<Vec<_>>().into_iter();
    let first = raw_arguments.next();
    if first.as_deref() == Some("analyze") {
        let input = raw_arguments
            .next()
            .unwrap_or_else(|| panic!("analyze requires an artifact path"));
        let mut output = None;
        while let Some(argument) = raw_arguments.next() {
            match argument.as_str() {
                "--output" => {
                    output = Some(
                        raw_arguments
                            .next()
                            .unwrap_or_else(|| panic!("--output requires a path")),
                    )
                }
                _ => panic!("unknown analysis argument '{argument}'"),
            }
        }
        let text = std::fs::read_to_string(&input)
            .unwrap_or_else(|error| panic!("cannot read {input}: {error}"));
        let artifact = SimulationArtifact::from_ron(&text)
            .unwrap_or_else(|error| panic!("invalid artifact: {error}"));
        let result = analyze_simulation_artifact(&artifact, &[])
            .unwrap_or_else(|error| panic!("analysis failed: {error}"));
        if let Some(path) = output {
            std::fs::write(&path, result.to_ron().unwrap())
                .unwrap_or_else(|error| panic!("cannot write {path}: {error}"));
        }
        print!("{}", result.engineering_summary());
        return;
    }
    let mut scenario = HeadlessScenario::default();
    let mut output = None;
    let mut arguments = first.into_iter().chain(raw_arguments);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--vehicle" => {
                scenario.vehicle_key = Some(
                    arguments
                        .next()
                        .unwrap_or_else(|| panic!("--vehicle requires a vehicle key")),
                );
            }
            "--steps" => {
                scenario.fixed_steps = arguments
                    .next()
                    .unwrap_or_else(|| panic!("--steps requires an integer"))
                    .parse()
                    .unwrap_or_else(|_| panic!("--steps requires an unsigned integer"));
            }
            "--scenario" => {
                scenario.scenario_id = arguments
                    .next()
                    .unwrap_or_else(|| panic!("--scenario requires an identifier"));
            }
            "--output" => {
                output = Some(
                    arguments
                        .next()
                        .unwrap_or_else(|| panic!("--output requires a path")),
                )
            }
            _ => panic!("unknown headless scenario argument '{argument}'"),
        }
    }

    let steps = scenario.fixed_steps;
    let artifact = run_headless_scenario_artifact(scenario)
        .unwrap_or_else(|error| panic!("headless scenario failed: {error}"));
    if let Some(path) = output {
        let text = artifact
            .to_ron()
            .unwrap_or_else(|error| panic!("cannot serialize artifact: {error}"));
        std::fs::write(&path, text).unwrap_or_else(|error| panic!("cannot write {path}: {error}"));
    }
    let final_frame = artifact
        .telemetry
        .last()
        .expect("headless run records a final frame");
    println!(
        "scenario={} steps={} mass_kg={:.3} position_m={:?} velocity_mps={:?}",
        artifact.run_identity.scenario_id,
        steps,
        final_frame.state.mass_kg,
        final_frame.state.position_m,
        final_frame.state.velocity_mps,
    );
    println!(
        "vehicle_sha256={} terrain={} ephemeris={}",
        artifact
            .run_identity
            .vehicle_configuration_sha256
            .as_deref()
            .expect("file-backed headless vehicle has an identity"),
        artifact.run_identity.terrain_source_id,
        artifact.run_identity.ephemeris_authority_id,
    );
}
