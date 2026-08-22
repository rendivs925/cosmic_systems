use crate::components::rocket::*;
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial, geodetic_to_body_fixed,
};
use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
use crate::domain::services::rocket_propulsion::DEFAULT_ULLAGE_SETTLE_TIME_S;
use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
use crate::infrastructure::bevy_adapters::components::Selectable;
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;

/// Flight-recorder ring capacity (entries).
const RECORDER_MAX_ENTRIES: usize = 2_048;
/// Flight-recorder sampling interval (s): ~10 physics ticks at 60 Hz.
const RECORDER_INTERVAL_S: f64 = 1.0 / 6.0;
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;

pub fn spawn_rockets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let rocket = Rocket::falcon9();

    // Create simple rocket mesh (cylinder)
    let mesh = Mesh::from(Cylinder::new(1.85, 70.0)); // Falcon 9 dimensions
    let mesh_handle = meshes.add(mesh);

    // Create rocket material
    let material = StandardMaterial {
        base_color: Color::srgb(0.8, 0.8, 0.8),
        metallic: 0.9,
        perceptual_roughness: 0.2,
        ..default()
    };
    let material_handle = materials.add(material);

    // Place the rocket on the Kennedy Space Center pad in planet-centered
    // inertial meters (authoritative 6-DOF frame).
    let earth = PlanetFactory::create_by_name("Earth").unwrap();
    let ksc = predefined_sites::kennedy_space_center();
    let body_fixed = geodetic_to_body_fixed(&ksc, &earth);
    let position_m = body_fixed_to_planet_inertial(body_fixed, &earth, 0.0);

    // Stand vertical on the pad: body +Y aligned with the local up direction
    // (radial). Guidance's launch target is the same attitude, so the
    // closed-loop ascent starts from zero attitude error.
    let launch_attitude = DQuat::from_rotation_arc(DVec3::Y, position_m.normalize());

    let total_mass_kg = rocket.total_mass_kg() as f64;
    let radius_m = (rocket.diameter_m / 2.0) as f64;
    let (inertia, com) = rocket_inertia_tensor(
        rocket.total_dry_mass_kg() as f64,
        rocket.total_propellant_mass_kg() as f64,
        radius_m,
        rocket.height_m as f64,
    );
    let dynamics = RocketDynamicsState::new(
        position_m,
        DVec3::ZERO,
        launch_attitude,
        total_mass_kg,
        inertia,
        com,
    );

    let propellant_remaining_kg = rocket
        .stages
        .iter()
        .map(|stage| stage.propellant_mass_kg)
        .collect();

    // Phase 1: Core physics components (fits in bundle limit)
    let entity = commands
        .spawn((
            RocketPhysicsState { dynamics },
            RocketGeometry {
                radius_m: radius_m as f32,
                height_m: rocket.height_m,
            },
            RocketMass(total_mass_kg),
            RocketMissionState::PreLaunch,
            RocketPropulsion {
                vehicle: rocket,
                active_stage: 0,
                propellant_remaining_kg,
                throttle: 0.0,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                // Gate starts open: the first (pad) ignition needs no ullage.
                time_since_separation_s: DEFAULT_ULLAGE_SETTLE_TIME_S,
                ullage_settle_time_s: DEFAULT_ULLAGE_SETTLE_TIME_S,
            },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            GravityAcceleration::default(),
            RocketPlanetBinding {
                planet_name: "Earth".to_string(),
            },
        ))
        .id();

    // Phase 2: Facade + render components.
    // Two inserts because Bevy bundle tuples cap at 15 items.
    commands.entity(entity).insert((
        RocketFacade::default(),
        AtmosphereState::default(),
        AerodynamicForces::default(),
        MaxQTracker::default(),
        RocketCommands::default(),
        RocketAutopilot::default(),
        TerrainCollisionState::default(),
        ThermalState::default(),
        AblationState::default(),
        ParachuteState::default(),
    ));

    // Phase 3: Entry/comms state + render primitives.
    commands.entity(entity).insert((
        CommsState::default(),
        RetroPropulsionEffect::default(),
        FlightRecorder::new(RECORDER_MAX_ENTRIES, RECORDER_INTERVAL_S),
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        Selectable {
            name: "Falcon 9".to_string(),
            selected: false,
        },
    ));
}
