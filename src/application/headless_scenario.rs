//! Presentation-free execution of one deterministic rocket scenario.

use crate::application::plugins::RocketFixedSimulationPlugin;
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::application::rocket_spawning::spawn_rocket_physics;
use crate::domain::services::simulation_analysis::{
    analyze_simulation_artifact, EngineeringConstraint, SimulationAnalysisResult,
};
use crate::domain::services::simulation_artifact::{SimulationArtifact, SimulationTelemetryFrame};
use crate::domain::services::simulation_run::SimulationRunIdentity;
use crate::domain::services::simulation_time::{SimulationTime, DEFAULT_FIXED_TIMESTEP_S};
#[cfg(feature = "dem")]
use crate::domain::services::terrain_source::DEFAULT_EARTH_DEM_PATH;
use crate::infrastructure::bevy_adapters::entity_components::{
    PlanetAtmosphere, PlanetComponent, PlanetTerrain,
};
use crate::infrastructure::bevy_adapters::ephemeris::{
    update_ephemeris_snapshot, EphemerisAuthority, EphemerisPlugin, EphemerisSnapshot,
};
use crate::infrastructure::bevy_adapters::rocket::components::{RocketMissionState, SpentStage};
use crate::infrastructure::bevy_adapters::rocket::sets::RocketSet;
use crate::infrastructure::bevy_adapters::rocket::telemetry::{
    build_simulation_telemetry_frame, record_simulation_telemetry_system,
    SimulationTelemetryAccess, SimulationTelemetryRecorder,
};
use bevy::math::DVec3;
use bevy::prelude::*;
#[cfg(feature = "dem")]
use sha2::{Digest, Sha256};

#[cfg(feature = "dem")]
fn terrain_source_identity() -> Result<String, String> {
    let terrain_bytes = std::fs::read(DEFAULT_EARTH_DEM_PATH)
        .map_err(|error| format!("cannot read terrain input {DEFAULT_EARTH_DEM_PATH}: {error}"))?;
    Ok(format!(
        "earth-csdem-sha256:{:x}",
        Sha256::digest(terrain_bytes)
    ))
}

#[cfg(not(feature = "dem"))]
fn terrain_source_identity() -> Result<String, String> {
    Ok("earth-procedural-default".to_string())
}

/// A bounded, deterministic non-interactive flight execution request.
#[derive(Debug, Clone)]
pub struct HeadlessScenario {
    pub scenario_id: String,
    pub vehicle_key: Option<String>,
    pub fixed_steps: u64,
}

impl Default for HeadlessScenario {
    fn default() -> Self {
        Self {
            scenario_id: "papua-ascent".to_string(),
            vehicle_key: None,
            fixed_steps: 64 * 60,
        }
    }
}

/// Result data is simulation state only; no render transform is observed.
#[derive(Debug, Clone)]
pub struct HeadlessScenarioResult {
    pub identity: SimulationRunIdentity,
    pub executed_steps: u64,
    pub final_position_m: DVec3,
    pub final_velocity_mps: DVec3,
    pub final_mass_kg: f64,
}

/// Sequential batch result. Every scenario creates a fresh Bevy world through
/// `run_headless_scenario_artifact`, so no physics or clock resource is shared.
#[derive(Debug)]
pub struct BatchScenarioResult {
    pub scenario_id: String,
    pub result: Result<SimulationArtifact, String>,
}

/// Per-scenario analysis preserves batch isolation: artifacts are analyzed
/// after their fresh simulation worlds have completed.
#[derive(Debug)]
pub struct BatchScenarioAnalysisResult {
    pub scenario_id: String,
    pub result: Result<SimulationAnalysisResult, String>,
}

pub fn run_headless_batch(
    scenarios: impl IntoIterator<Item = HeadlessScenario>,
) -> Vec<BatchScenarioResult> {
    scenarios
        .into_iter()
        .map(|scenario| {
            let scenario_id = scenario.scenario_id.clone();
            let result = run_headless_scenario_artifact(scenario);
            BatchScenarioResult {
                scenario_id,
                result,
            }
        })
        .collect()
}

pub fn analyze_headless_batch(
    batch: &[BatchScenarioResult],
    constraints: &[EngineeringConstraint],
) -> Vec<BatchScenarioAnalysisResult> {
    batch
        .iter()
        .map(|scenario| BatchScenarioAnalysisResult {
            scenario_id: scenario.scenario_id.clone(),
            result: scenario
                .result
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|artifact| analyze_simulation_artifact(artifact, constraints)),
        })
        .collect()
}

/// Capture the initial primary-vehicle frame before the first fixed tick. The
/// shared fixed pipeline records every completed tick into the
/// [`SimulationTelemetryRecorder`] through the same frame model.
fn capture_telemetry_frame(app: &mut App) -> Result<SimulationTelemetryFrame, String> {
    let time = app.world().resource::<SimulationTime>();
    let epoch = time
        .tdb_epoch()
        .map_err(|error| error.to_string())?
        .seconds_since_j2000();
    let simulation_time_s = time.sim_time_s;
    let world = app.world_mut();
    let mut query = world.query_filtered::<SimulationTelemetryAccess, Without<SpentStage>>();
    let access = query.single(world).map_err(|error| error.to_string())?;
    let gravitational_parameter_m3_s2 = world
        .resource::<EphemerisSnapshot>()
        .gravitational_parameter_for_catalog_body(access.binding.planet_name.as_str());
    Ok(build_simulation_telemetry_frame(
        simulation_time_s,
        epoch,
        gravitational_parameter_m3_s2,
        &access,
    ))
}

fn spawn_headless_scenario(
    mut commands: Commands,
    catalog: Res<RocketCatalog>,
    selection: Res<VehicleSelection>,
    snapshot: Res<EphemerisSnapshot>,
) {
    let earth_orientation = snapshot
        .orientation_for_catalog_body("Earth")
        .expect("ephemeris startup must provide Earth orientation");
    let terrain = PlanetTerrain::earth();
    let earth = crate::domain::services::planet_factory::PlanetFactory::create_by_name("Earth")
        .expect("Earth must exist in the celestial catalog");
    commands.spawn((
        PlanetComponent {
            domain_planet: earth,
            material: Handle::default(),
            has_texture: false,
            base_reflectance: 1.0,
            base_roughness: 1.0,
        },
        terrain.clone(),
        PlanetAtmosphere::default_for("Earth"),
    ));
    let rocket = spawn_rocket_physics(
        &mut commands,
        &catalog,
        &selection,
        terrain.source.as_ref(),
        earth_orientation,
    );
    commands.entity(rocket).insert(RocketMissionState::Launch);
}

/// Execute existing fixed rocket systems without constructing an interactive
/// world. Scientific kernels and the selected vehicle are validated while the
/// app is composed, before a single simulation tick runs.
pub fn run_headless_scenario(scenario: HeadlessScenario) -> Result<HeadlessScenarioResult, String> {
    let artifact = run_headless_scenario_artifact(scenario.clone())?;
    let final_frame = artifact
        .telemetry
        .last()
        .ok_or_else(|| "headless scenario did not record a rocket state".to_string())?;
    Ok(HeadlessScenarioResult {
        identity: artifact.run_identity,
        executed_steps: scenario.fixed_steps,
        final_position_m: DVec3::from_array(final_frame.state.position_m),
        final_velocity_mps: DVec3::from_array(final_frame.state.velocity_mps),
        final_mass_kg: final_frame.state.mass_kg,
    })
}

/// Compose and start a fresh isolated rocket-run world, returning the app and
/// the run identity derived from its validated kernel and vehicle selection.
fn build_headless_app(scenario: &HeadlessScenario) -> Result<(App, SimulationRunIdentity), String> {
    let catalog = RocketCatalog::from_dir().map_err(|error| error.to_string())?;
    let selection = VehicleSelection::from(scenario.vehicle_key.clone());
    let (_, vehicle) = catalog
        .resolve(&selection)
        .ok_or_else(|| format!("unknown vehicle '{}'", selection.selected_key()))?;
    let vehicle_model_id = vehicle.rocket.name.clone();
    let vehicle_configuration_sha256 = vehicle.configuration_sha256.clone();

    let terrain_source_id = terrain_source_identity()?;

    let mut app = App::new();
    app.add_plugins(MinimalPlugins);
    app.insert_resource(SimulationTime::new(DEFAULT_FIXED_TIMESTEP_S));
    app.insert_resource(catalog);
    app.insert_resource(selection);
    app.add_plugins((EphemerisPlugin, RocketFixedSimulationPlugin));
    // Frame capture is consumed only by the artifact, so the headless
    // composition registers it instead of the interactive mode.
    app.add_systems(
        FixedUpdate,
        record_simulation_telemetry_system.in_set(RocketSet::Telemetry),
    );
    app.add_systems(
        Startup,
        spawn_headless_scenario.after(update_ephemeris_snapshot),
    );
    app.update();

    let simulation_time = app.world().resource::<SimulationTime>();
    let ephemeris = app.world().resource::<EphemerisAuthority>();
    let identity = SimulationRunIdentity {
        scenario_id: scenario.scenario_id.clone(),
        vehicle_model_id,
        vehicle_configuration_sha256: Some(vehicle_configuration_sha256),
        launch_site_id: "papua-indonesia-coastal-lowland".to_string(),
        environment_model_id: "earth-standard-atmosphere-v1".to_string(),
        terrain_source_id,
        ephemeris_authority_id: ephemeris.0.provenance().run_identity(),
        start_epoch_tdb_seconds_since_j2000: simulation_time
            .tdb_epoch()
            .map_err(|error| error.to_string())?
            .seconds_since_j2000(),
        state_reference_frame: "planet-inertial-meters".to_string(),
        numerical_integrator: "semi-implicit-euler".to_string(),
        fixed_timestep_s: DEFAULT_FIXED_TIMESTEP_S,
        random_seed: None,
        software_revision: option_env!("GIT_HASH").unwrap_or("workspace").to_string(),
    };
    identity.validate()?;
    Ok((app, identity))
}

/// Execute a fresh isolated simulation context and retain its fixed-tick output.
pub fn run_headless_scenario_artifact(
    scenario: HeadlessScenario,
) -> Result<SimulationArtifact, String> {
    let (mut app, identity) = build_headless_app(&scenario)?;
    let mut artifact = SimulationArtifact::new(identity);
    artifact.telemetry.push(capture_telemetry_frame(&mut app)?);
    // The shared fixed pipeline records each completed tick through the same
    // frame model. Discard any frame a stray startup fixed step may have
    // produced, then drain per tick so the artifact owns every frame without
    // ever hitting the interactive bound.
    app.world_mut()
        .resource_mut::<SimulationTelemetryRecorder>()
        .frames
        .clear();

    for _ in 0..scenario.fixed_steps {
        app.world_mut().run_schedule(FixedUpdate);
        artifact.telemetry.append(
            &mut app
                .world_mut()
                .resource_mut::<SimulationTelemetryRecorder>()
                .frames,
        );
    }
    artifact.events = std::mem::take(
        &mut app
            .world_mut()
            .resource_mut::<SimulationTelemetryRecorder>()
            .events,
    );
    artifact.validate()?;
    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::services::simulation_artifact::SimulationEventType;
    use crate::infrastructure::bevy_adapters::rocket::components::RocketTelemetry;

    #[test]
    fn repeated_headless_runs_are_bitwise_deterministic_and_identified() {
        let scenario = HeadlessScenario {
            fixed_steps: 4,
            ..default()
        };
        let first = run_headless_scenario(scenario.clone()).expect("first headless run succeeds");
        let second = run_headless_scenario(scenario).expect("second headless run succeeds");

        assert_eq!(first.identity, second.identity);
        assert_eq!(
            first.final_position_m.to_array(),
            second.final_position_m.to_array()
        );
        assert_eq!(
            first.final_velocity_mps.to_array(),
            second.final_velocity_mps.to_array()
        );
        assert_eq!(
            first.final_mass_kg.to_bits(),
            second.final_mass_kg.to_bits()
        );
        assert!(first
            .identity
            .vehicle_configuration_sha256
            .as_deref()
            .is_some_and(|checksum| checksum.len() == 64));
        let recorded = run_headless_scenario_artifact(HeadlessScenario {
            fixed_steps: 2,
            ..default()
        })
        .unwrap();
        let replay = crate::domain::services::simulation_artifact::ReplayTimeline::new(
            SimulationArtifact::from_ron(&recorded.to_ron().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(replay.artifact().telemetry.len(), 3);
        assert_eq!(replay.current().unwrap().state, recorded.telemetry[0].state);
        #[cfg(feature = "dem")]
        assert!(first.identity.terrain_source_id.contains("sha256:"));
    }

    #[test]
    fn batch_runs_are_isolated_and_report_their_scenarios() {
        let runs = run_headless_batch([
            HeadlessScenario {
                scenario_id: "one".into(),
                fixed_steps: 1,
                vehicle_key: None,
            },
            HeadlessScenario {
                scenario_id: "two".into(),
                fixed_steps: 1,
                vehicle_key: None,
            },
        ]);
        assert_eq!(runs.len(), 2);
        assert!(
            runs.iter().all(|run| run.result.is_ok()),
            "batch failures: {:?}",
            runs.iter()
                .map(|run| (&run.scenario_id, run.result.as_ref().err()))
                .collect::<Vec<_>>()
        );
        assert_ne!(
            runs[0].result.as_ref().unwrap().run_identity.scenario_id,
            runs[1].result.as_ref().unwrap().run_identity.scenario_id
        );
    }

    #[test]
    fn live_telemetry_propellant_fraction_falls_during_burn() {
        let scenario = HeadlessScenario {
            fixed_steps: 300,
            ..default()
        };
        let (mut app, _) = build_headless_app(&scenario).expect("headless app builds");

        for _ in 0..300 {
            app.world_mut().run_schedule(FixedUpdate);
        }

        let telemetry = app.world().resource::<RocketTelemetry>();
        assert!(
            telemetry.active_stage_propellant_fraction < 0.99,
            "active-stage fuel must fall below 99% after 4.7 s of burn, got {}",
            telemetry.active_stage_propellant_fraction
        );
        assert!(
            telemetry.active_stage_propellant_kg < telemetry.active_stage_initial_propellant_kg,
            "active-stage propellant must drop below its {} kg load, got {}",
            telemetry.active_stage_initial_propellant_kg,
            telemetry.active_stage_propellant_kg
        );
        assert!(
            telemetry.total_thrust_n > 0.0,
            "engines must be producing thrust, got {}",
            telemetry.total_thrust_n
        );
    }

    #[test]
    fn recorded_event_timeline_is_deterministic_and_ordered() {
        let scenario = HeadlessScenario {
            fixed_steps: 64 * 10,
            ..default()
        };
        let first = run_headless_scenario_artifact(scenario.clone()).expect("first run");
        let second = run_headless_scenario_artifact(scenario).expect("second run");

        assert_eq!(first.events, second.events);
        assert!(
            !first.events.is_empty(),
            "an ascent must record authoritative lifecycle events"
        );
        assert_eq!(
            first.events[0].event_type,
            Some(SimulationEventType::Ignition),
            "the first recorded event must be the engine ignition"
        );
        for window in first.events.windows(2) {
            assert!(window[1].simulation_time_s >= window[0].simulation_time_s);
        }
    }

    #[test]
    fn batch_analysis_uses_each_completed_artifact() {
        let batch = run_headless_batch([HeadlessScenario {
            fixed_steps: 1,
            ..default()
        }]);
        let analysis = analyze_headless_batch(&batch, &[]);
        assert_eq!(analysis.len(), 1);
        assert!(analysis[0].result.is_ok());
    }
}
