//! Presentation-free execution of one deterministic rocket scenario.

use crate::application::plugins::RocketFixedSimulationPlugin;
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::application::rocket_spawning::spawn_rocket_physics;
use crate::domain::services::regression::RocketStateSample;
use crate::domain::services::simulation_artifact::{SimulationArtifact, SimulationTelemetryFrame};
use crate::domain::services::simulation_run::SimulationRunIdentity;
use crate::domain::services::simulation_time::SimulationTime;
#[cfg(feature = "dem")]
use crate::domain::services::terrain_source::DEFAULT_EARTH_DEM_PATH;
use crate::infrastructure::bevy_adapters::entity_components::{
    PlanetAtmosphere, PlanetComponent, PlanetTerrain,
};
use crate::infrastructure::bevy_adapters::ephemeris::{
    update_ephemeris_snapshot, EphemerisAuthority, EphemerisPlugin, EphemerisSnapshot,
};
use crate::infrastructure::bevy_adapters::rocket::components::{
    RocketFlightConditions, RocketMissionState, RocketPhysicsState, RocketPropulsion,
    TerrainCollisionState,
};
use bevy::math::DVec3;
use bevy::prelude::*;
#[cfg(feature = "dem")]
use sha2::{Digest, Sha256};

const FIXED_TIMESTEP_S: f64 = 1.0 / 64.0;

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

pub fn write_simulation_artifact(
    path: impl AsRef<std::path::Path>,
    artifact: &SimulationArtifact,
) -> Result<(), String> {
    std::fs::write(path.as_ref(), artifact.to_ron()?).map_err(|error| error.to_string())
}

/// Execute sequentially and write one deterministic RON artifact per success.
/// The file stem combines the scenario id and file-backed vehicle checksum so
/// wall-clock time is never part of artifact identity.
pub fn run_headless_batch_to_directory(
    scenarios: impl IntoIterator<Item = HeadlessScenario>,
    directory: impl AsRef<std::path::Path>,
) -> Vec<BatchScenarioResult> {
    let directory = directory.as_ref();
    let mut results = run_headless_batch(scenarios);
    for batch in &mut results {
        let Ok(artifact) = &batch.result else {
            continue;
        };
        let scenario = batch
            .scenario_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect::<String>();
        let vehicle = artifact
            .run_identity
            .vehicle_configuration_sha256
            .as_deref()
            .unwrap_or("in-memory");
        let path = directory.join(format!(
            "{scenario}-{}-v{}.ron",
            &vehicle[..vehicle.len().min(12)],
            artifact.schema_version
        ));
        let text = match artifact.to_ron() {
            Ok(text) => text,
            Err(error) => {
                batch.result = Err(format!("cannot serialize batch artifact: {error}"));
                continue;
            }
        };
        if let Err(error) =
            std::fs::create_dir_all(directory).and_then(|_| std::fs::write(path, text))
        {
            batch.result = Err(format!("cannot write batch artifact: {error}"));
        }
    }
    results
}

fn mission_code(mission: RocketMissionState) -> u8 {
    use crate::domain::entities::rocket::RocketMissionState as State;
    match mission.0 {
        State::PreLaunch => 0,
        State::Launch => 1,
        State::Ascent => 2,
        State::Orbit => 3,
        State::DeorbitBurn => 4,
        State::ReentryCorridor => 5,
        State::PoweredDescent => 6,
        State::UnpoweredDescent => 7,
        State::Landing => 8,
        State::Landed => 9,
        State::Crashed => 10,
    }
}

fn capture_telemetry_frame(app: &mut App) -> Result<SimulationTelemetryFrame, String> {
    let time = app.world().resource::<SimulationTime>();
    let epoch = time
        .tdb_epoch()
        .map_err(|error| error.to_string())?
        .seconds_since_j2000();
    let simulation_time_s = time.sim_time_s;
    let world = app.world_mut();
    let mut query = world.query::<(
        &RocketPhysicsState,
        &RocketMissionState,
        &RocketPropulsion,
        &RocketFlightConditions,
        &TerrainCollisionState,
    )>();
    let (physics, mission, propulsion, conditions, collision) =
        query.single(world).map_err(|error| error.to_string())?;
    let dynamics = &physics.dynamics;
    Ok(SimulationTelemetryFrame {
        simulation_time_s,
        epoch_tdb_seconds_since_j2000: epoch,
        state: RocketStateSample::new(
            [
                dynamics.position_m.x,
                dynamics.position_m.y,
                dynamics.position_m.z,
            ],
            [
                dynamics.velocity_mps.x,
                dynamics.velocity_mps.y,
                dynamics.velocity_mps.z,
            ],
            [
                dynamics.orientation.x,
                dynamics.orientation.y,
                dynamics.orientation.z,
                dynamics.orientation.w,
            ],
            [
                dynamics.angular_velocity_radps.x,
                dynamics.angular_velocity_radps.y,
                dynamics.angular_velocity_radps.z,
            ],
            dynamics.mass_kg,
            mission_code(*mission),
        ),
        active_stage: propulsion.active_stage as u32,
        propellant_remaining_kg: propulsion
            .propellant_remaining_kg
            .iter()
            .map(|value| f64::from(*value))
            .sum(),
        throttle_unit: f64::from(propulsion.throttle),
        mach_number: conditions.mach_number,
        dynamic_pressure_pa: conditions.dynamic_pressure_pa,
        atmospheric_density_kg_m3: conditions.density_kg_m3,
        terrain_altitude_m: collision.radar_altitude_m,
        ground_contact: !matches!(
            collision.ground_contact,
            crate::domain::services::terrain_collision::GroundContact::None
        ),
    })
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

/// Execute a fresh isolated simulation context and retain its fixed-tick output.
pub fn run_headless_scenario_artifact(
    scenario: HeadlessScenario,
) -> Result<SimulationArtifact, String> {
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
    app.insert_resource(SimulationTime::new(FIXED_TIMESTEP_S));
    app.insert_resource(catalog);
    app.insert_resource(selection);
    app.add_plugins((EphemerisPlugin, RocketFixedSimulationPlugin));
    app.add_systems(
        Startup,
        spawn_headless_scenario.after(update_ephemeris_snapshot),
    );
    app.update();

    let simulation_time = app.world().resource::<SimulationTime>();
    let ephemeris = app.world().resource::<EphemerisAuthority>();
    let identity = SimulationRunIdentity {
        scenario_id: scenario.scenario_id,
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
        fixed_timestep_s: FIXED_TIMESTEP_S,
        random_seed: None,
        software_revision: option_env!("GIT_HASH").unwrap_or("workspace").to_string(),
    };
    identity.validate()?;
    let mut artifact = SimulationArtifact::new(identity);
    artifact.telemetry.push(capture_telemetry_frame(&mut app)?);

    for _ in 0..scenario.fixed_steps {
        app.world_mut().run_schedule(FixedUpdate);
        artifact.telemetry.push(capture_telemetry_frame(&mut app)?);
    }
    artifact.validate()?;
    Ok(artifact)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
