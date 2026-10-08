use crate::application::launch_tower::{
    diagonal_braces, perimeter_braces, service_arms, TowerLayout, TowerMember, PLATFORM_THICKNESS_M,
};
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::domain::entities::rocket::Rocket;
use crate::domain::services::body_orientation::BodyOrientation;
use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::{
    body_fixed_to_planet_inertial_rotation, enu_basis, geodetic_to_body_fixed,
    geodetic_to_terrain_lat_lon, surface_velocity_in_planet_inertial,
};
use crate::domain::services::rocket_dynamics::{
    orientation_from_up_and_heading, RocketDynamicsState,
};
use crate::domain::services::rocket_propulsion::{
    ActiveVehicleMassProperties, DEFAULT_ULLAGE_SETTLE_TIME_S,
};
use crate::domain::services::terrain_collision::sample_surface;
use crate::domain::services::terrain_source::TerrainSource;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::entity_components::Selectable;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::rocket::components::*;
use crate::infrastructure::bevy_adapters::rocket::telemetry::FlightRecorder;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

/// Flight-recorder ring capacity (entries).
const RECORDER_MAX_ENTRIES: usize = 2_048;
/// Flight-recorder sampling interval (s): ~10 physics ticks at 60 Hz.
const RECORDER_INTERVAL_S: f64 = 1.0 / 6.0;

/// Authoritative launch-frame and mass properties computed once for a vehicle
/// selection. Interactive presentation and headless execution both consume this
/// single launch model so the two spawns cannot drift (AGENTS.md sections 17,
/// 50, 51).
struct LaunchSetup {
    rocket: Rocket,
    final_stage_fairing_mass_kg: Option<f32>,
    propulsion: RocketPropulsion,
    geometry: RocketGeometry,
    dynamics: RocketDynamicsState,
    launch_site: LaunchSiteCoordinates,
    mass_properties: ActiveVehicleMassProperties,
    position_body_fixed_m: DVec3,
    terrain_normal_body_fixed: DVec3,
    pad_north_body_fixed: DVec3,
}

/// Resolve the selected vehicle and derive its authoritative prelaunch state in
/// the one planet-centered inertial frame. Presentation-only values (mesh,
/// material, pad geometry) are derived by the caller from this result.
fn build_launch_setup(
    catalog: &RocketCatalog,
    selection: &VehicleSelection,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) -> LaunchSetup {
    let requested_key = selection.selected_key();
    let Some((_, vehicle)) = catalog.resolve(selection) else {
        let available = catalog.keys().collect::<Vec<_>>().join(", ");
        panic!("Unknown vehicle '{requested_key}'. Available vehicles: {available}");
    };
    let rocket = vehicle.rocket.clone();
    // A final-stage fairing rides with the vehicle mass until jettison; one
    // authority is shared by consumption, serial staging, and jettison.
    let final_stage_fairing_mass_kg = rocket
        .stages
        .last()
        .and_then(|stage| stage.fairing_dry_mass_kg);
    let attached_payload_kg = final_stage_fairing_mass_kg.unwrap_or(0.0);
    let propulsion = RocketPropulsion::for_fresh_flight(
        rocket.clone(),
        attached_payload_kg,
        DEFAULT_ULLAGE_SETTLE_TIME_S,
    );

    // The launch site is defined in Earth body-fixed geodetic coordinates, then
    // converted once into the authoritative planet-centered inertial frame.
    // Collision and terrain convert back through the same reference-frame API.
    let papua = predefined_sites::papua_indonesia_coastal_lowland();
    let earth = PlanetFactory::create_by_id(&papua.planet_id).unwrap();
    let earth_radius_m = earth.radius_km as f64 * 1000.0;
    let (terrain_latitude_deg, terrain_longitude_deg) = geodetic_to_terrain_lat_lon(&papua, &earth);
    let terrain_sample = sample_surface(
        terrain_source,
        terrain_latitude_deg,
        terrain_longitude_deg,
        earth_radius_m,
    );
    let launch_site = LaunchSiteCoordinates::new(
        papua.planet_id.clone(),
        papua.latitude_deg,
        papua.longitude_deg,
        terrain_sample.height_m as f32,
    );
    // Terrain elevations are radial offsets from the catalog mean radius. Use
    // the WGS-84-derived radial direction, but let the shared terrain surface
    // define the authoritative launch radius.
    let position_body_fixed_m = geodetic_to_body_fixed(&launch_site, &earth).normalize()
        * (earth_radius_m + terrain_sample.height_m);
    let body_to_inertial = body_fixed_to_planet_inertial_rotation(earth_orientation);
    // Stand on the procedural pad normal while preserving a deterministic
    // northward heading. This is the held physical prelaunch attitude, not a
    // presentation rotation.
    let launch_up = body_to_inertial * terrain_sample.normal;
    let (_, pad_north_body_fixed, _) = enu_basis(papua.latitude_deg, papua.longitude_deg);
    let launch_attitude =
        orientation_from_up_and_heading(launch_up, body_to_inertial * pad_north_body_fixed)
            .expect("Papua coastal-lowland launch heading is finite");
    let geometry = RocketGeometry {
        radius_m: rocket.diameter_m / 2.0,
        height_m: rocket.height_m,
        lower_extent_y_m: rocket.lower_extent_in_stack_m(),
    };
    let mass_properties = propulsion.mass_properties(geometry, 0.0);
    // State position is the full cylindrical stack's geometric center; its
    // lower -Y extent, rather than that center, rests on the launch surface.
    let position_m =
        body_to_inertial * position_body_fixed_m + launch_up * (rocket.height_m as f64 * 0.5);
    let dynamics = RocketDynamicsState::new(
        position_m,
        surface_velocity_in_planet_inertial(position_m, earth_orientation),
        launch_attitude,
        mass_properties.mass_kg,
        mass_properties.inertia_body,
        mass_properties.center_of_mass_m,
    );
    LaunchSetup {
        rocket,
        final_stage_fairing_mass_kg,
        propulsion,
        geometry,
        dynamics,
        launch_site,
        mass_properties,
        position_body_fixed_m,
        terrain_normal_body_fixed: terrain_sample.normal,
        pad_north_body_fixed,
    }
}

/// Spawn the authoritative rocket entity: physical state, flight-support state,
/// recorder, fairing, and landing gear. Presentation components are added by
/// the interactive caller; headless stops here.
fn spawn_rocket_core(commands: &mut Commands, setup: &LaunchSetup) -> Entity {
    let entity = commands
        .spawn((
            RocketPhysicsState {
                dynamics: setup.dynamics,
            },
            setup.geometry,
            RocketMissionState::PreLaunch,
            MissionPhaseTracker::default(),
            setup.propulsion.clone(),
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            GravityAcceleration::default(),
            SpecificForceAcceleration::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
            setup.launch_site.clone(),
        ))
        .id();
    commands.entity(entity).insert((
        RocketFlightConditions::default(),
        AerodynamicForces::default(),
        MaxQTracker::default(),
        RocketCommands::default(),
        RocketAutopilot::for_launch_site(f64::from(setup.launch_site.latitude_deg)),
        TerrainCollisionState::default(),
        // The vehicle spawns standing on the pad: the resting-contact
        // constraint holds it there until thrust exceeds weight (real physics
        // instead of the old crash-exemption hack).
        GroundRest { active: true },
        // Required by update_orbital_elements and guidance_system; without it
        // neither system ever matches the entity.
        OrbitalElements::default(),
        ThermalState::default(),
        AblationState::default(),
        ParachuteState::default(),
        // Required by GroundContactAccess (resolve_ground_contact). Without
        // these, the contact query never matches the vehicle, GroundRest never
        // holds it, and the rocket falls freely through the terrain.
        TipOverState::default(),
        LandingScorecard::default(),
        CommsState::default(),
        RetroPropulsionEffect::default(),
    ));
    commands
        .entity(entity)
        .insert(AppliedPropulsionState::default());
    commands.entity(entity).insert(FlightRecorder::new(
        RECORDER_MAX_ENTRIES,
        RECORDER_INTERVAL_S,
    ));
    if let Some(dry_mass_kg) = setup.final_stage_fairing_mass_kg {
        commands.entity(entity).insert((
            PayloadFairing { dry_mass_kg },
            InitialPayloadFairing { dry_mass_kg },
        ));
    }
    // Contact follows only the active serial stage's attached hardware. Leg
    // meshes wait until a stage is independently recoverable, avoiding lower
    // stage visuals parented to an upper stage after separation.
    if let Some(spec) = setup
        .rocket
        .stages
        .first()
        .and_then(|stage| stage.landing_gear)
    {
        commands
            .entity(entity)
            .insert(LandingLegs::new(LandingGear::new(
                spec,
                setup.mass_properties.mass_kg,
            )));
    }
    entity
}

/// Spawn the authoritative rocket state without presentation components.
///
/// The graphical launcher still owns mesh, material, and pad creation. This
/// path deliberately uses the same launch-frame and mass-property calculations
/// so headless execution does not have a second physical spawn model.
pub fn spawn_rocket_physics(
    commands: &mut Commands,
    catalog: &RocketCatalog,
    selection: &VehicleSelection,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) -> Entity {
    let setup = build_launch_setup(catalog, selection, terrain_source, earth_orientation);
    spawn_rocket_core(commands, &setup)
}

pub(crate) fn spawn_rockets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    catalog: &RocketCatalog,
    selection: &VehicleSelection,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) {
    let setup = build_launch_setup(catalog, selection, terrain_source, earth_orientation);
    let entity = spawn_rocket_core(commands, &setup);

    // Create a proper multi-part rocket mesh from the vehicle configuration.
    let mesh_handle = build_rocket_mesh(meshes, &setup.rocket);

    // Create rocket material: white painted hull, lit by the sun so the body
    // shades correctly (cylinder silhouette reads as a rocket, not a ghost).
    // Slightly lower base value + higher roughness to prevent blowout under
    // 100 klx sun + sky ambient.
    let material = StandardMaterial {
        base_color: Color::srgb(0.9, 0.9, 0.92),
        metallic: 0.05,
        perceptual_roughness: 0.52,
        reflectance: 0.45,
        ..default()
    };
    let material_handle = materials.add(material);

    // Presentation primitives. The mesh and transform are non-authoritative;
    // `capture_render_state`/`interpolate_render_transform` derive them from the
    // authoritative dynamics.
    commands.entity(entity).insert((
        RocketRenderState::new(setup.dynamics),
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::default(),
        Selectable {
            name: setup.rocket.name.clone(),
            selected: false,
        },
    ));

    spawn_procedural_launch_pad(
        commands,
        meshes,
        materials,
        LaunchPadPresentation {
            planet_name: CelestialBodyId::earth(),
            position_body_fixed_m: setup.position_body_fixed_m,
            normal_body_fixed: setup.terrain_normal_body_fixed,
            heading_body_fixed: setup.pad_north_body_fixed,
        },
        setup.rocket.height_m,
        setup.rocket.diameter_m,
    );
}

/// Spawn a procedural service tower and supporting facilities at the exact
/// terrain launch point. Every child is presentation-only: it has no collision
/// authority, contributes no terrain height, and applies no forces. All
/// structure children are marked [`LaunchSiteStructure`] so they can be hidden
/// beyond the presentation structure draw distance.
fn spawn_procedural_launch_pad(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    anchor: LaunchPadPresentation,
    rocket_height_m: f32,
    rocket_diameter_m: f32,
) {
    let steel = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.31, 0.34),
        metallic: 0.35,
        perceptual_roughness: 0.48,
        reflectance: 0.5,
        // Pull pad steel slightly toward the camera so it wins depth ties with
        // terrain that interpolates to the same height at the anchor.
        depth_bias: 1.5,
        ..default()
    });
    let concrete = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.19, 0.18),
        perceptual_roughness: 0.88,
        depth_bias: 2.0,
        ..default()
    });
    let water = materials.add(StandardMaterial {
        base_color: Color::srgba(0.62, 0.74, 0.78, 0.55),
        metallic: 0.0,
        perceptual_roughness: 0.28,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let warning_light = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.17, 0.03),
        emissive: Color::srgb(1.0, 0.06, 0.01).to_linear() * 5.0,
        perceptual_roughness: 0.35,
        ..default()
    });
    let floodlight = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.94, 0.86),
        emissive: Color::srgb(1.0, 0.96, 0.82).to_linear() * 4.0,
        perceptual_roughness: 0.4,
        ..default()
    });
    let hazard = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.62, 0.05),
        metallic: 0.1,
        perceptual_roughness: 0.6,
        ..default()
    });
    let tower = TowerLayout::from_vehicle(rocket_height_m, rocket_diameter_m);
    let tower_height_m = tower.height_m;
    let tower_offset_m = tower.center_x_m;
    let mast_height_m = tower.mast_height_m;
    let root = commands
        .spawn((
            anchor,
            Transform::default(),
            GlobalTransform::default(),
            Visibility::default(),
        ))
        .id();
    commands.entity(root).with_children(|parent| {
        // A wide concrete apron grounds the facility visually; the deck sits on
        // top. Both are low slabs whose top face sits a few centimetres above
        // the sampled surface height, so streamed terrain that interpolates to
        // the same height cannot z-fight with or hide the pad, while the slab
        // still reads as a prepared pad rather than a raised platform.
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(64.0, 0.5, 64.0))),
            MeshMaterial3d(concrete.clone()),
            Transform::from_xyz(0.0, -0.20, 0.0),
            Name::new("LaunchPadApron"),
        ));
        // The deck is centered on the authoritative terrain anchor with its top
        // 0.06 m proud of the sampled surface; the vehicle rests on the surface,
        // so its base sits inside the slab, which reads as a launch mount rather
        // than a floating pad.
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(36.0, 0.5, 36.0))),
            MeshMaterial3d(concrete.clone()),
            Transform::from_xyz(0.0, -0.19, 0.0),
            Name::new("LaunchPadDeck"),
        ));
        // Local procedural facility details only. The terrain and pad anchor
        // remain the sole source of surface placement.
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(9.0, 5.0, 16.0))),
            MeshMaterial3d(concrete.clone()),
            Transform::from_xyz(0.0, -2.55, 0.0),
            Name::new("LaunchPadFlameTrench"),
        ));
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(10.0, 2.5, 1.0))),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(0.0, -1.15, -7.0),
            Name::new("LaunchPadFlameDeflector"),
        ));
        for x in [-11.0_f32, 11.0] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(0.35, 1.4, 26.0))),
                MeshMaterial3d(water.clone()),
                Transform::from_xyz(x, 0.45, 0.0),
                Name::new("LaunchPadDelugeRail"),
            ));
        }
        // Hold-down clamps around the vehicle base.
        for (x, z) in [(-3.0_f32, -3.0_f32), (-3.0, 3.0), (3.0, -3.0), (3.0, 3.0)] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(1.2, 0.8, 1.2))),
                MeshMaterial3d(hazard.clone()),
                Transform::from_xyz(x, 0.5, z),
                Name::new("LaunchPadHoldDownClamp"),
            ));
        }
        // Service tower legs. The braces below attach to these columns.
        for center in tower.column_centers() {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(0.55, tower_height_m, 0.55))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(center[0], tower_height_m * 0.5, center[2]),
                Name::new("LaunchPadTowerLeg"),
            ));
        }
        // Platforms span the full tower height so the topmost supports the
        // lightning mast, and they enclose the columns so levels brace every leg.
        for y in tower.platform_levels_y() {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(
                    tower.platform_half_span_m * 2.0,
                    PLATFORM_THICKNESS_M,
                    tower.platform_half_span_m * 2.0,
                ))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(tower_offset_m, y, 0.0),
                Name::new("LaunchPadTowerLevel"),
            ));
        }
        // Horizontal perimeter rings and face diagonals. Both are built from
        // explicit column/platform attachment points, so no member floats in the
        // tower interior between the legs.
        for member in perimeter_braces(&tower) {
            spawn_tower_member(parent, meshes, &steel, member, "LaunchPadTowerBrace");
        }
        for member in diagonal_braces(&tower) {
            spawn_tower_member(parent, meshes, &steel, member, "LaunchPadTowerDiagonal");
        }
        // Service and umbilical arms start on the platform edge nearest the
        // vehicle and reach its surface, so each arm is carried by the platform.
        for member in service_arms(&tower, rocket_diameter_m) {
            spawn_tower_member(parent, meshes, &steel, member, "LaunchPadServiceArm");
        }
        // Lightning mast rising above the tower.
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cylinder::new(0.22, mast_height_m - tower_height_m))),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(tower_offset_m, (tower_height_m + mast_height_m) * 0.5, 0.0),
            Name::new("LaunchPadLightningMast"),
        ));
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Sphere::new(0.5))),
            MeshMaterial3d(warning_light.clone()),
            Transform::from_xyz(tower_offset_m, mast_height_m, 0.0),
            Name::new("LaunchPadMastBeacon"),
        ));
        for z in [-4.0_f32, 4.0] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Sphere::new(0.28))),
                MeshMaterial3d(warning_light.clone()),
                Transform::from_xyz(tower_offset_m + 4.4, tower_height_m + 0.2, z),
                Name::new("LaunchPadTowerBeacon"),
            ));
        }
        // Supporting facilities: water tanks, a control blockhouse, and
        // perimeter floodlight poles.
        for (x, z, radius) in [(-20.0_f32, 16.0_f32, 3.2_f32), (-13.0, 20.0, 2.4)] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cylinder::new(radius, 10.0))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(x, 5.0, z),
                Name::new("LaunchPadWaterTank"),
            ));
        }
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(12.0, 4.0, 8.0))),
            MeshMaterial3d(concrete.clone()),
            Transform::from_xyz(-22.0, 2.0, -16.0),
            Name::new("LaunchPadBlockhouse"),
        ));
        for (x, z) in [
            (-16.0_f32, -16.0_f32),
            (-16.0, 16.0),
            (16.0, -16.0),
            (16.0, 16.0),
        ] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cylinder::new(0.18, 12.0))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(x, 6.0, z),
                Name::new("LaunchPadFloodlightPole"),
            ));
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(1.4, 0.5, 0.6))),
                MeshMaterial3d(floodlight.clone()),
                Transform::from_xyz(x, 12.2, z),
                Name::new("LaunchPadFloodlightHead"),
            ));
        }
    });
}

/// Spawn one straight tower member mesh between its explicit attachment points.
/// The cuboid is authored along +X and rotated onto the member direction, so the
/// endpoints land exactly where the geometry module placed them.
fn spawn_tower_member(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    steel: &Handle<StandardMaterial>,
    member: TowerMember,
    name: &'static str,
) {
    let start = Vec3::from_array(member.start);
    let end = Vec3::from_array(member.end);
    let delta = end - start;
    let length = delta.length().max(1e-3);
    let rotation = Quat::from_rotation_arc(Vec3::X, delta / length);
    parent.spawn((
        LaunchSiteStructure,
        Mesh3d(meshes.add(Cuboid::new(length, member.thickness, member.thickness))),
        MeshMaterial3d(steel.clone()),
        Transform::from_translation((start + end) * 0.5).with_rotation(rotation),
        Name::new(name),
    ));
}

/// Synchronize the facility from its body-fixed anchor through the same shared
/// ephemeris and render-origin boundary as the rocket and terrain patches.
pub fn sync_launch_pad_presentation(
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    render_origin: Res<RenderOrigin>,
    physical_scale: Res<PhysicalScale>,
    mut pads: Query<(&LaunchPadPresentation, &mut Transform)>,
) {
    for (pad, mut transform) in &mut pads {
        let Some(orientation) =
            ephemeris_snapshot.orientation_for_catalog_body(pad.planet_name.as_str())
        else {
            continue;
        };
        let body_to_inertial = body_fixed_to_planet_inertial_rotation(orientation);
        let Some(attitude) = orientation_from_up_and_heading(
            body_to_inertial * pad.normal_body_fixed,
            body_to_inertial * pad.heading_body_fixed,
        ) else {
            continue;
        };
        let local_m = body_to_inertial * pad.position_body_fixed_m - render_origin.origin;
        transform.translation = DVec3::new(
            physical_scale.flight_meters_to_units(local_m.x),
            physical_scale.flight_meters_to_units(local_m.y),
            physical_scale.flight_meters_to_units(local_m.z),
        )
        .as_vec3();
        transform.rotation = attitude.as_quat();
    }
}

/// Build a catalog-derived multi-part rocket mesh. Its origin is the full
/// cylindrical stack's geometric center, matching the physical assembly frame.
fn rocket_mesh_engine_stations(
    rocket: &crate::domain::entities::rocket::Rocket,
) -> Vec<(usize, Vec3)> {
    rocket
        .stages
        .iter()
        .enumerate()
        .flat_map(|(stage_index, stage)| {
            stage.engines.iter().map(move |engine| {
                (
                    stage_index,
                    crate::domain::entities::rocket::Rocket::engine_position_in_stack_m(
                        &rocket.stages,
                        rocket.height_m,
                        stage_index,
                        engine,
                    )
                    .expect("stage index comes from rocket.stages"),
                )
            })
        })
        .collect()
}

pub(crate) fn build_rocket_mesh(
    meshes: &mut Assets<Mesh>,
    rocket: &crate::domain::entities::rocket::Rocket,
) -> Handle<Mesh> {
    let total_height = rocket.height_m;

    // Build mesh manually with all required attributes to avoid merge issues
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Helper: add a cylinder section at base_y with given height and radius
    let add_cylinder = |center_x: f32,
                        center_z: f32,
                        base_y: f32,
                        height: f32,
                        radius: f32,
                        positions: &mut Vec<[f32; 3]>,
                        normals: &mut Vec<[f32; 3]>,
                        uvs: &mut Vec<[f32; 2]>,
                        indices: &mut Vec<u32>,
                        index_offset: &mut u32| {
        let rings = 16;
        let segments = 32;
        let section_start = *index_offset;
        for ring in 0..=rings {
            let y = base_y + height * (ring as f32 / rings as f32);
            let v = ring as f32 / rings as f32;
            for seg in 0..segments {
                let angle = seg as f32 * std::f32::consts::TAU / segments as f32;
                let x = center_x + radius * angle.cos();
                let z = center_z + radius * angle.sin();
                positions.push([x, y, z]);
                normals.push([angle.cos(), 0.0, angle.sin()]);
                uvs.push([seg as f32 / segments as f32, v]);
            }
        }
        for ring in 0..rings {
            for seg in 0..segments {
                let next_seg = (seg + 1) % segments;
                let a = section_start + ring * segments + seg;
                let b = section_start + ring * segments + next_seg;
                let c = section_start + (ring + 1) * segments + seg;
                let d = section_start + (ring + 1) * segments + next_seg;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        let base_cap = section_start + (rings + 1) * segments;
        positions.push([center_x, base_y, center_z]);
        normals.push([0.0, -1.0, 0.0]);
        uvs.push([0.5, 0.5]);
        let top_cap = base_cap + 1;
        positions.push([center_x, base_y + height, center_z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.5, 0.5]);
        for seg in 0..segments {
            let next_seg = (seg + 1) % segments;
            let base_a = section_start + seg;
            let base_b = section_start + next_seg;
            let top_a = section_start + rings * segments + seg;
            let top_b = section_start + rings * segments + next_seg;
            indices.extend_from_slice(&[base_cap, base_a, base_b]);
            indices.extend_from_slice(&[top_cap, top_b, top_a]);
        }
        *index_offset = top_cap + 1;
    };

    // Helper: add a cone at (center_x, center_z) with base at base_y, apex at base_y + height
    let add_cone = |center_x: f32,
                    center_z: f32,
                    base_y: f32,
                    height: f32,
                    radius: f32,
                    positions: &mut Vec<[f32; 3]>,
                    normals: &mut Vec<[f32; 3]>,
                    uvs: &mut Vec<[f32; 2]>,
                    indices: &mut Vec<u32>,
                    index_offset: &mut u32| {
        let segments = 32;
        // Apex
        positions.push([center_x, base_y + height, center_z]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([0.5, 1.0]);
        let apex_idx = *index_offset;
        *index_offset += 1;
        // Base ring
        for seg in 0..segments {
            let angle = seg as f32 * std::f32::consts::TAU / segments as f32;
            let x = center_x + radius * angle.cos();
            let z = center_z + radius * angle.sin();
            let nx = angle.cos();
            let nz = angle.sin();
            positions.push([x, base_y, z]);
            // Apex-up cone outward normal is radial plus up, matching the
            // triangle winding corrected below.
            let normal_len = (nx * nx + nz * nz + 0.25).sqrt();
            normals.push([nx / normal_len, 0.5 / normal_len, nz / normal_len]);
            uvs.push([seg as f32 / segments as f32, 0.0]);
        }
        for seg in 0..segments {
            let next_seg = (seg + 1) % segments;
            let a = apex_idx;
            let b = *index_offset + seg;
            let c = *index_offset + next_seg;
            // Outward-facing lateral triangle: (apex, next, seg) rather than
            // (apex, seg, next), which faced inward and was back-face culled.
            indices.extend_from_slice(&[a, c, b]);
        }
        *index_offset += segments;
        let base_cap = *index_offset;
        positions.push([center_x, base_y, center_z]);
        normals.push([0.0, -1.0, 0.0]);
        uvs.push([0.5, 0.5]);
        for seg in 0..segments {
            let next_seg = (seg + 1) % segments;
            let a = base_cap - segments + seg;
            let b = base_cap - segments + next_seg;
            indices.extend_from_slice(&[base_cap, a, b]);
        }
        *index_offset += 1;
    };

    let mut idx_offset = 0u32;

    let mut next_stage_base_y_m = -total_height * 0.5;
    for stage in &rocket.stages {
        add_cylinder(
            0.0,
            0.0,
            next_stage_base_y_m,
            stage.height_m,
            stage.diameter_m * 0.5,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            &mut idx_offset,
        );
        next_stage_base_y_m += stage.height_m;
    }

    if let Some(boosters) = &rocket.parallel_boosters {
        for attachment_m in boosters.attachment_positions() {
            add_cylinder(
                attachment_m.x,
                attachment_m.z,
                attachment_m.y - boosters.stage.height_m * 0.5,
                boosters.stage.height_m,
                boosters.stage.diameter_m * 0.5,
                &mut positions,
                &mut normals,
                &mut uvs,
                &mut indices,
                &mut idx_offset,
            );
        }
    }

    // Any declared stack height beyond the stage cylinders is the catalog's
    // upper adapter/fairing volume. The catalog does not claim its exact shape.
    let nose_height_m = (total_height * 0.5 - next_stage_base_y_m).max(0.0);
    if nose_height_m > 0.0 {
        add_cone(
            0.0,
            0.0,
            next_stage_base_y_m,
            nose_height_m,
            rocket.diameter_m * 0.5,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            &mut idx_offset,
        );
    }

    for (stage_index, engine_station_m) in rocket_mesh_engine_stations(rocket) {
        let stage = &rocket.stages[stage_index];
        // Bell proportions are generic presentation only. The station and
        // radial engine layout are exactly the catalog's stage-local data.
        let bell_height_m = (stage.diameter_m * 0.35).max(0.2);
        let bell_radius_m = (stage.diameter_m * 0.16).max(0.08);
        add_cone(
            engine_station_m.x,
            engine_station_m.z,
            engine_station_m.y - bell_height_m,
            bell_height_m,
            bell_radius_m,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            &mut idx_offset,
        );
    }
    if let Some(boosters) = &rocket.parallel_boosters {
        let bell_height_m = (boosters.stage.diameter_m * 0.35).max(0.2);
        let bell_radius_m = (boosters.stage.diameter_m * 0.16).max(0.08);
        for booster_index in 0..boosters.count() {
            for engine in &boosters.stage.engines {
                let engine_station_m = crate::domain::entities::rocket::Rocket::parallel_booster_engine_position_in_stack_m(
                    boosters,
                    booster_index,
                    engine,
                )
                .expect("booster index is bounded by its attachment inventory");
                add_cone(
                    engine_station_m.x,
                    engine_station_m.z,
                    engine_station_m.y - bell_height_m,
                    bell_height_m,
                    bell_radius_m,
                    &mut positions,
                    &mut normals,
                    &mut uvs,
                    &mut indices,
                    &mut idx_offset,
                );
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));

    meshes.add(mesh)
}

/// Build the presentation mesh for a surviving serial stage in its own
/// stage-centered body frame. Physics already changes to this same frame at
/// separation, so the rendered assembly cannot retain the discarded stack.
pub(crate) fn build_serial_stage_mesh(
    meshes: &mut Assets<Mesh>,
    stage: &crate::domain::entities::rocket::RocketStage,
    upper_envelope_height_m: f32,
) -> Handle<Mesh> {
    build_rocket_mesh(
        meshes,
        &crate::domain::entities::rocket::Rocket {
            name: stage.name.clone(),
            diameter_m: stage.diameter_m,
            // The physics stage remains its catalog height; this presentation
            // envelope preserves the upper adapter/fairing that stays attached
            // after a lower serial stage separates.
            height_m: stage.height_m + upper_envelope_height_m.max(0.0),
            stages: vec![stage.clone()],
            parallel_boosters: None,
        },
    )
}

#[cfg(test)]
#[expect(
    clippy::items_after_test_module,
    reason = "The spawn regressions stay beside the catalog-driven mesh builder."
)]
mod tests {
    use super::*;
    use crate::domain::entities::rocket::{
        EngineState, RocketEngine, RocketStage, ThrustReference,
    };
    use bevy::light::{NotShadowCaster, NotShadowReceiver};

    #[test]
    fn procedural_pad_is_geodetically_anchored_and_presentation_only() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.add_systems(
            Startup,
            |mut commands: Commands,
             mut meshes: ResMut<Assets<Mesh>>,
             mut materials: ResMut<Assets<StandardMaterial>>| {
                spawn_procedural_launch_pad(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    LaunchPadPresentation {
                        planet_name: CelestialBodyId::earth(),
                        position_body_fixed_m: DVec3::new(6_371_000.0, 0.0, 0.0),
                        normal_body_fixed: DVec3::X,
                        heading_body_fixed: DVec3::Z,
                    },
                    70.0,
                    3.7,
                );
            },
        );
        app.update();

        let world = app.world_mut();
        let mut anchors = world.query_filtered::<Entity, With<LaunchPadPresentation>>();
        assert_eq!(anchors.iter(world).count(), 1);

        let mut structures = world.query_filtered::<Entity, With<LaunchSiteStructure>>();
        assert!(
            structures.iter(world).count() > 20,
            "the pad should present detailed structures"
        );

        // No structure may carry collision or physics state.
        let mut physics = world.query_filtered::<Entity, (
            With<LaunchSiteStructure>,
            Or<(With<TerrainCollisionState>, With<RocketPhysicsState>)>,
        )>();
        assert_eq!(physics.iter(world).count(), 0);

        // Pad structures participate in shadows: none carries an exclusion marker.
        let mut excluded = world.query_filtered::<Entity, (
            With<LaunchSiteStructure>,
            Or<(With<NotShadowCaster>, With<NotShadowReceiver>)>,
        )>();
        assert_eq!(excluded.iter(world).count(), 0);
    }

    /// Integration test over the actually spawned meshes: expand every brace and
    /// diagonal beam from its spawned transform and assert both physical endpoints
    /// land on a spawned column or platform, not in the tower interior. This
    /// catches the original defect (braces centred on the tower axis at z = 0,
    /// meeting no column) even if the layout functions are untouched.
    #[test]
    fn spawned_tower_members_attach_to_columns_or_platforms() {
        let height_m = 70.0;
        let diameter_m = 3.7;
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<Assets<Mesh>>();
        app.init_resource::<Assets<StandardMaterial>>();
        app.add_systems(
            Startup,
            move |mut commands: Commands,
                  mut meshes: ResMut<Assets<Mesh>>,
                  mut materials: ResMut<Assets<StandardMaterial>>| {
                spawn_procedural_launch_pad(
                    &mut commands,
                    &mut meshes,
                    &mut materials,
                    LaunchPadPresentation {
                        planet_name: CelestialBodyId::earth(),
                        position_body_fixed_m: DVec3::new(6_371_000.0, 0.0, 0.0),
                        normal_body_fixed: DVec3::X,
                        heading_body_fixed: DVec3::Z,
                    },
                    height_m,
                    diameter_m,
                );
            },
        );
        app.update();

        let world = app.world_mut();
        let layout = TowerLayout::from_vehicle(height_m, diameter_m);
        let columns: Vec<[f32; 2]> = layout
            .column_centers()
            .iter()
            .map(|center| [center[0], center[2]])
            .collect();
        let platforms: Vec<f32> = layout.platform_levels_y().to_vec();
        // The parent root is spawned with an identity transform, so a child's
        // local translation equals its position in pad space.
        let mut query = world.query::<(&Name, &Transform, &Mesh3d)>();
        let meshes = world.resource::<Assets<Mesh>>();
        let mut checked = 0usize;
        for (name, transform, mesh3d) in query.iter(world) {
            let kind = name.as_str();
            if kind != "LaunchPadTowerBrace" && kind != "LaunchPadTowerDiagonal" {
                continue;
            }
            let mesh = meshes
                .get(&mesh3d.0)
                .expect("spawned tower member mesh must exist");
            let Some(bevy_mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("tower member mesh must carry Float32x3 positions");
            };
            let half_length = positions.iter().map(|p| p[0].abs()).fold(0.0f32, f32::max);
            let axis = transform.rotation * Vec3::X * half_length;
            for endpoint in [transform.translation - axis, transform.translation + axis] {
                let on_column = columns
                    .iter()
                    .any(|c| (endpoint.x - c[0]).hypot(endpoint.z - c[1]) < 0.35);
                let on_platform = platforms
                    .iter()
                    .any(|y| (endpoint.y - y).abs() < PLATFORM_THICKNESS_M);
                assert!(
                    on_column || on_platform,
                    "{kind} endpoint {endpoint:?} attaches to neither a column nor a platform"
                );
            }
            checked += 1;
        }
        assert!(
            checked >= 8,
            "expected the spawned tower to contain perimeter and diagonal braces, found {checked}"
        );
    }

    #[test]
    fn data_driven_mesh_layout_honors_non_falcon_dimensions_and_stations() {
        let engine = |position_m| RocketEngine {
            position_m,
            thrust_axis: Vec3::Y,
            isp_sea_level: 250.0,
            isp_vacuum: 300.0,
            gimbal_range_deg: 5.0,
            rated_thrust_kn: 100.0,
            thrust_reference: ThrustReference::SeaLevel,
            throttle_min: 0.0,
            throttle_max: 1.0,
            max_ignitions: 2,
            ignition_count: 1,
            state: EngineState::Running,
        };
        let rocket = crate::domain::entities::rocket::Rocket {
            name: "Narrow test vehicle".into(),
            diameter_m: 2.0,
            height_m: 30.0,
            stages: vec![
                RocketStage {
                    name: "Lower".into(),
                    diameter_m: 2.0,
                    height_m: 10.0,
                    dry_mass_kg: 100.0,
                    propellant_mass_kg: 900.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines: vec![engine(Vec3::new(0.75, -5.0, 0.0))],
                },
                RocketStage {
                    name: "Upper".into(),
                    diameter_m: 1.0,
                    height_m: 8.0,
                    dry_mass_kg: 50.0,
                    propellant_mass_kg: 250.0,
                    recovery_propellant_reserve_kg: None,
                    landing_gear: None,
                    fairing_dry_mass_kg: None,
                    engines: vec![engine(Vec3::new(0.0, -4.0, 0.0))],
                },
            ],
            parallel_boosters: None,
        };

        assert_eq!(
            crate::domain::entities::rocket::Rocket::stage_origin_in_stack_m(
                &rocket.stages,
                rocket.height_m,
                0,
            ),
            Some(Vec3::new(0.0, -10.0, 0.0))
        );
        assert_eq!(
            crate::domain::entities::rocket::Rocket::stage_origin_in_stack_m(
                &rocket.stages,
                rocket.height_m,
                1,
            ),
            Some(Vec3::new(0.0, -1.0, 0.0))
        );
        assert_eq!(
            rocket_mesh_engine_stations(&rocket),
            vec![
                (0, Vec3::new(0.75, -15.0, 0.0)),
                (1, Vec3::new(0.0, -5.0, 0.0))
            ]
        );
    }

    #[test]
    fn serial_stage_mesh_has_only_the_active_stage_geometry() {
        let stage = RocketStage {
            name: "Upper".into(),
            diameter_m: 1.0,
            height_m: 8.0,
            dry_mass_kg: 50.0,
            propellant_mass_kg: 250.0,
            recovery_propellant_reserve_kg: None,
            landing_gear: None,
            fairing_dry_mass_kg: None,
            engines: vec![RocketEngine {
                position_m: Vec3::new(0.0, -4.0, 0.0),
                thrust_axis: Vec3::Y,
                isp_sea_level: 250.0,
                isp_vacuum: 300.0,
                gimbal_range_deg: 5.0,
                rated_thrust_kn: 100.0,
                thrust_reference: ThrustReference::Vacuum,
                throttle_min: 0.0,
                throttle_max: 1.0,
                max_ignitions: 1,
                ignition_count: 0,
                state: EngineState::Off,
            }],
        };
        let mut meshes = Assets::<Mesh>::default();
        let mesh = build_serial_stage_mesh(&mut meshes, &stage, 0.0);

        assert!(meshes.get(&mesh).is_some());
    }

    /// Checkpoint 3 acceptance: the vehicle must rest on the same authoritative
    /// terrain surface that collision and rendering use. The held prelaunch
    /// state samples the procedural surface rather than a fixed altitude, so a
    /// regression that floats or sinks the pad would detach it from the ground
    /// it is drawn on.
    #[test]
    fn launch_vehicle_rests_on_authoritative_terrain() {
        use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
        use crate::domain::services::terrain_source::ProceduralTerrainSource;
        use bevy::math::DQuat;

        let catalog = RocketCatalog::from_dir()
            .expect("the shipped vehicle catalog must load for the launch regression");
        let selection = VehicleSelection::Default;
        let terrain_source = ProceduralTerrainSource::new(42, 2_500.0, 1_200.0, 0);
        let orientation = BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::ZERO,
        );

        // Independently reproduce the authoritative surface sample at the site.
        let papua = predefined_sites::papua_indonesia_coastal_lowland();
        let earth = PlanetFactory::create_by_id(&papua.planet_id).unwrap();
        let earth_radius_m = earth.radius_km as f64 * 1000.0;
        let (terrain_latitude_deg, terrain_longitude_deg) =
            geodetic_to_terrain_lat_lon(&papua, &earth);
        let sample = sample_surface(
            &terrain_source,
            terrain_latitude_deg,
            terrain_longitude_deg,
            earth_radius_m,
        );

        let setup = build_launch_setup(&catalog, &selection, &terrain_source, &orientation);

        // The stack centre is raised half its height above the resting base
        // along the launch up axis (+Y body).
        let up = (setup.dynamics.orientation * DVec3::Y).normalize();
        let base = setup.dynamics.position_m - up * (setup.rocket.height_m as f64 * 0.5);
        let expected_radius_m = earth_radius_m + sample.height_m;
        assert!(
            (base.length() - expected_radius_m).abs() < 1e-3,
            "vehicle base radius {} must match the terrain radius {expected_radius_m}",
            base.length()
        );

        // The held attitude stands on the terrain normal, not the ellipsoid radial.
        let expected_up = body_fixed_to_planet_inertial_rotation(&orientation) * sample.normal;
        assert!(
            up.dot(expected_up) > 0.999,
            "vehicle up {} must follow the terrain normal {expected_up}",
            up
        );
    }
}

/// Spawn one visual strut per configured landing leg as a child of the
/// rocket, angled from the lower hull out to the foot radius. The pose is
/// static (always shown deployed); collision/physics never read these
/// entities.
pub(crate) fn spawn_landing_leg_meshes(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    parent: Entity,
    height_m: f32,
    hull_radius_m: f32,
    spec: &LandingGearSpec,
) {
    let count = spec.count.max(1);
    let stroke_m = spec.stroke_m as f32;
    let base_radius_m = spec.base_radius_m as f32;
    // Mesh and physics share a center origin: legs attach at the lower
    // cylindrical extent and extend below it as presentation only.
    let root_y = -height_m * 0.5 + stroke_m * 0.25;
    let foot_y = -height_m * 0.5 - stroke_m * 0.6;

    for i in 0..count {
        let azimuth = i as f32 * std::f32::consts::TAU / count as f32;
        let (cos_a, sin_a) = (azimuth.cos(), azimuth.sin());
        let root = Vec3::new(hull_radius_m * cos_a, root_y, hull_radius_m * sin_a);
        let foot = Vec3::new(base_radius_m * cos_a, foot_y, base_radius_m * sin_a);
        let direction = (foot - root).normalize_or_zero();
        if !direction.is_finite() || direction == Vec3::ZERO {
            continue;
        }
        let length = root.distance(foot);
        commands.entity(parent).with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cylinder::new(0.15, length))),
                MeshMaterial3d(material.clone()),
                Transform {
                    translation: (root + foot) / 2.0,
                    rotation: Quat::from_rotation_arc(Vec3::Y, direction),
                    ..default()
                },
            ));
        });
    }
}
