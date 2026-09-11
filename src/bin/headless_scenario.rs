//! Run a bounded rocket scenario without initializing presentation systems.

use cosmic_systems_wasm::application::headless_scenario::{
    run_headless_scenario_artifact, HeadlessScenario,
};

fn main() {
    let mut scenario = HeadlessScenario::default();
    let mut output = None;
    let mut arguments = std::env::args().skip(1);
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
