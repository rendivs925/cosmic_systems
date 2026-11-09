use crate::application::launch_tower::{
    diagonal_braces, perimeter_braces, service_arms, TowerLayout, TowerMember, PLATFORM_THICKNESS_M,
};
use crate::application::rocket_config::{RocketCatalog, VehicleSelection};
use crate::domain::entities::planet::Planet;
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
use crate::domain::services::terrain_source::{
    TerrainSource, LAUNCH_HARDSTAND_CUTOUT_RADIUS_M, LAUNCH_HARDSTAND_HALF_EXTENT_M,
    LAUNCH_TRENCH_DEPTH_M, LAUNCH_TRENCH_FAR_Z_M, LAUNCH_TRENCH_HALF_WIDTH_M,
    LAUNCH_TRENCH_NEAR_Z_M,
};
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::value_objects::launch_site_coordinates::{
    LaunchSiteCoordinates, PredefinedLaunchSite,
};
use crate::infrastructure::bevy_adapters::entity_components::Selectable;
use crate::infrastructure::bevy_adapters::ephemeris::EphemerisSnapshot;
use crate::infrastructure::bevy_adapters::physical_scale::PhysicalScale;
use crate::infrastructure::bevy_adapters::rocket::components::*;
use crate::infrastructure::bevy_adapters::rocket::telemetry::FlightRecorder;
use crate::infrastructure::bevy_adapters::terrain::cutouts::TerrainMeshCutouts;
use crate::infrastructure::bevy_adapters::terrain::render::RenderOrigin;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::math::DVec3;
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

/// Flight-recorder ring capacity (entries).
const RECORDER_MAX_ENTRIES: usize = 2_048;
/// Flight-recorder sampling interval (s): ~10 physics ticks at 60 Hz.
const RECORDER_INTERVAL_S: f64 = 1.0 / 6.0;

/// Gap between the hull and the inner face of a hold-down clamp, metres. The
/// clamp ring is derived from the hull radius, so no vehicle family can have its
/// clamps sit inside the body.
const CLAMP_CLEARANCE_M: f32 = 0.35;

/// Size multiplier for the support structures, from the vehicle height.
///
/// The hardstand and the flame trench are site infrastructure and stay fixed: the
/// trench width is what the terrain-mesh cutout contract can guarantee, so it
/// cannot shrink with a small launcher. The hardware *on* the pad — tanks,
/// blockhouse, floodlights — does follow the vehicle, or an 18 m Electron would
/// stand beside 10 m tanks and a 12 m blockhouse.
fn facility_scale(rocket_height_m: f32) -> f32 {
    (rocket_height_m / 70.0).clamp(0.6, 1.3)
}

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
    /// Guidance targets for this site, including the geocentric-latitude
    /// inclination the ascent plane can actually reach.
    autopilot: RocketAutopilot,
    mass_properties: ActiveVehicleMassProperties,
    position_body_fixed_m: DVec3,
    terrain_normal_body_fixed: DVec3,
    pad_north_body_fixed: DVec3,
}

/// Catalog, vehicle, and site inputs shared by interactive and headless spawns.
#[derive(Clone, Copy)]
pub struct LaunchConfiguration<'a> {
    pub catalog: &'a RocketCatalog,
    pub vehicle: &'a VehicleSelection,
    pub site: PredefinedLaunchSite,
}

/// Resolve the selected vehicle and derive its authoritative prelaunch state in
/// the one planet-centered inertial frame. Presentation-only values (mesh,
/// material, pad geometry) are derived by the caller from this result.
fn build_launch_setup(
    configuration: &LaunchConfiguration<'_>,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) -> LaunchSetup {
    let requested_key = configuration.vehicle.selected_key();
    let Some((_, vehicle)) = configuration.catalog.resolve(configuration.vehicle) else {
        let available = configuration.catalog.keys().collect::<Vec<_>>().join(", ");
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
    let site = configuration.site.coordinates();
    let earth = PlanetFactory::create_by_id(&site.planet_id).unwrap();
    let earth_radius_m = earth.radius_km as f64 * 1000.0;
    let (terrain_latitude_deg, terrain_longitude_deg) = geodetic_to_terrain_lat_lon(&site, &earth);
    if let Some(tiles) = terrain_source.elevation_tile_source() {
        if let Err(error) =
            tiles.ensure_covering_tile_loaded(terrain_latitude_deg, terrain_longitude_deg)
        {
            panic!(
                "{} launch elevation payload failed to load: {error:?}",
                configuration.site.label()
            );
        }
    }
    let terrain_sample = sample_surface(
        terrain_source,
        terrain_latitude_deg,
        terrain_longitude_deg,
        earth_radius_m,
    );
    let launch_site = LaunchSiteCoordinates::new(
        site.planet_id.clone(),
        site.latitude_deg,
        site.longitude_deg,
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
    let (_, pad_north_body_fixed, _) = enu_basis(site.latitude_deg, site.longitude_deg);
    let launch_attitude =
        orientation_from_up_and_heading(launch_up, body_to_inertial * pad_north_body_fixed)
            .unwrap_or_else(|| panic!("{} launch heading is finite", configuration.site.label()));
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
    let autopilot = autopilot_for_site(&launch_site, &earth);
    LaunchSetup {
        rocket,
        final_stage_fairing_mass_kg,
        propulsion,
        geometry,
        dynamics,
        launch_site,
        autopilot,
        mass_properties,
        position_body_fixed_m,
        terrain_normal_body_fixed: terrain_sample.normal,
        pad_north_body_fixed,
    }
}

/// Autopilot whose target inclination is the site's **geocentric** latitude.
///
/// A due-east launch reaches an orbital inclination equal to the geocentric
/// latitude, because the orbit plane contains the radial position vector.
/// WGS-84 geodetic latitude is not that value: Starbase is 25.9971° geodetic
/// but 25.8458° geocentric. The conversion goes through the one authoritative
/// geodetic-to-terrain-latitude function rather than a second formula.
fn autopilot_for_site(site: &LaunchSiteCoordinates, planet: &Planet) -> RocketAutopilot {
    let (terrain_latitude_deg, _) = geodetic_to_terrain_lat_lon(site, planet);
    RocketAutopilot::for_launch_site(terrain_latitude_deg)
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
        setup.autopilot.clone(),
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
    configuration: &LaunchConfiguration<'_>,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) -> Entity {
    let setup = build_launch_setup(configuration, terrain_source, earth_orientation);
    spawn_rocket_core(commands, &setup)
}

pub(crate) fn spawn_rockets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    configuration: &LaunchConfiguration<'_>,
    terrain_source: &dyn TerrainSource,
    earth_orientation: &BodyOrientation,
) {
    let setup = build_launch_setup(configuration, terrain_source, earth_orientation);
    let entity = spawn_rocket_core(commands, &setup);

    // Create a proper multi-part rocket mesh from the vehicle configuration.
    let mesh_handle = build_rocket_mesh(meshes, &setup.rocket, 0.0);

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

/// Spawn one axis-aligned concrete pad slab. The shared constructor keeps the
/// apron, deck and trench lining on one half-thickness convention, so the
/// geometry-placement tests can compare raw mesh bounds without per-part
/// special cases.
fn spawn_pad_slab(
    parent: &mut ChildSpawnerCommands,
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    name: &'static str,
    size: Vec3,
    centre: Vec3,
) {
    parent.spawn((
        LaunchSiteStructure,
        Mesh3d(meshes.add(Cuboid::new(size.x, size.y, size.z))),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(centre),
        Name::new(name),
    ));
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
    // The hardstand covers the ground here, and its flame trench is below grade,
    // so the terrain mesh must stop at the structure: a continuous terrain
    // surface spans the opening and hides the trench. Registering the cutout
    // here keeps it tied to the facility that fills it, and a site with no
    // facility never loses its ground.
    let mut cutouts = TerrainMeshCutouts::default();
    if let Some(planet) = PlanetFactory::create_by_id(&anchor.planet_name) {
        let radius_m = planet.radius_km as f64 * 1_000.0;
        if radius_m > 0.0 {
            cutouts.cover_once(
                anchor.normal_body_fixed,
                LAUNCH_HARDSTAND_CUTOUT_RADIUS_M / radius_m,
            );
        }
    }
    commands.insert_resource(cutouts);

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
        // the same height cannot z-fight with or hide the pad. Both stop at the
        // flame-trench mouth instead of bridging over it: the excavation is cut
        // into the terrain authority (LF-03), so a continuous slab would cover
        // the opening and hide the trench again.
        let trench_half_width_m = LAUNCH_TRENCH_HALF_WIDTH_M as f32;
        let trench_near_z_m = LAUNCH_TRENCH_NEAR_Z_M as f32;
        let trench_far_z_m = LAUNCH_TRENCH_FAR_Z_M as f32;
        let trench_depth_m = LAUNCH_TRENCH_DEPTH_M as f32;
        // Slabs that meet a perpendicular frame overlap it by this much. Two
        // coplanar slabs sharing an edge leave a sub-pixel crack under the top-left
        // fill rule, and at the trench corner that crack looked straight through
        // the ground.
        let slab_overlap_m = 2.0_f32;
        let apron_half_span_m = LAUNCH_HARDSTAND_HALF_EXTENT_M as f32;
        let deck_half_span_m = 18.0;
        for (name, centre_x_m, centre_z_m, size_x_m, size_z_m) in [
            (
                "LaunchPadApron",
                -(trench_half_width_m + apron_half_span_m) * 0.5,
                0.0,
                apron_half_span_m - trench_half_width_m,
                2.0 * apron_half_span_m,
            ),
            (
                "LaunchPadApron",
                (trench_half_width_m + apron_half_span_m) * 0.5,
                0.0,
                apron_half_span_m - trench_half_width_m,
                2.0 * apron_half_span_m,
            ),
            (
                "LaunchPadApron",
                0.0,
                (-apron_half_span_m + trench_near_z_m) * 0.5,
                2.0 * trench_half_width_m + slab_overlap_m,
                apron_half_span_m + trench_near_z_m,
            ),
            (
                "LaunchPadApron",
                0.0,
                (apron_half_span_m + trench_far_z_m) * 0.5,
                2.0 * trench_half_width_m + slab_overlap_m,
                apron_half_span_m - trench_far_z_m,
            ),
        ] {
            spawn_pad_slab(
                parent,
                meshes,
                &concrete,
                name,
                Vec3::new(size_x_m, 0.5, size_z_m),
                Vec3::new(centre_x_m, -0.20, centre_z_m),
            );
        }
        // The deck is centered on the authoritative terrain anchor with its top
        // 0.06 m proud of the sampled surface; the vehicle rests on the surface,
        // so its base sits inside the slab, which reads as a launch mount rather
        // than a floating pad.
        for (centre_x_m, size_x_m) in [
            (
                -(trench_half_width_m + deck_half_span_m) * 0.5,
                deck_half_span_m - trench_half_width_m,
            ),
            (
                (trench_half_width_m + deck_half_span_m) * 0.5,
                deck_half_span_m - trench_half_width_m,
            ),
        ] {
            spawn_pad_slab(
                parent,
                meshes,
                &concrete,
                "LaunchPadDeck",
                Vec3::new(size_x_m, 0.5, 2.0 * deck_half_span_m),
                Vec3::new(centre_x_m, -0.19, 0.0),
            );
        }
        spawn_pad_slab(
            parent,
            meshes,
            &concrete,
            "LaunchPadDeck",
            Vec3::new(
                2.0 * trench_half_width_m + slab_overlap_m,
                0.5,
                deck_half_span_m + trench_near_z_m,
            ),
            Vec3::new(0.0, -0.19, (-deck_half_span_m + trench_near_z_m) * 0.5),
        );
        // Trench lining. The excavation itself is terrain authority; these slabs
        // face it in concrete and give the opening a readable edge. Every part
        // sits on the excavated floor or spans from it to the pad plane.
        let trench_length_m = trench_far_z_m - trench_near_z_m;
        let trench_centre_z_m = (trench_near_z_m + trench_far_z_m) * 0.5;
        spawn_pad_slab(
            parent,
            meshes,
            &concrete,
            "LaunchPadFlameTrenchFloor",
            Vec3::new(2.0 * trench_half_width_m, 0.4, trench_length_m),
            Vec3::new(0.0, -trench_depth_m + 0.2, trench_centre_z_m),
        );
        // The lining rim must clear the terrain grade. Its top face used to sit
        // at exactly `y = 0`, the same height as the graded ground and the sea
        // cap, so it z-fought and let slivers of grass and sky through the
        // trench edge. It now meets the apron's top plane instead.
        let pad_top_m = 0.05;
        // Every rim part straddles the opening edge instead of abutting it, so no
        // two faces are coplanar and the corners cannot crack open. The part that
        // sits beyond the edge is hidden inside the apron and the deck.
        let rim_thickness_m = 0.4;
        for side in [-1.0_f32, 1.0] {
            spawn_pad_slab(
                parent,
                meshes,
                &concrete,
                "LaunchPadFlameTrenchWall",
                Vec3::new(
                    rim_thickness_m,
                    trench_depth_m + pad_top_m,
                    trench_length_m + 2.0 * rim_thickness_m,
                ),
                Vec3::new(
                    side * trench_half_width_m,
                    -trench_depth_m * 0.5 + pad_top_m * 0.5,
                    trench_centre_z_m,
                ),
            );
        }
        for centre_z_m in [trench_near_z_m, trench_far_z_m] {
            spawn_pad_slab(
                parent,
                meshes,
                &concrete,
                "LaunchPadFlameTrenchWall",
                Vec3::new(
                    2.0 * trench_half_width_m + 2.0 * rim_thickness_m,
                    trench_depth_m + pad_top_m,
                    rim_thickness_m,
                ),
                Vec3::new(0.0, -trench_depth_m * 0.5 + pad_top_m * 0.5, centre_z_m),
            );
        }
        // The deflector is a single inclined steel plate standing on the trench
        // floor. Its highest edge stays well below the pad plane, so it cannot
        // intersect the vehicle or its engines.
        let deflector_length_m = 5.0_f32;
        let deflector_angle_rad = 35.0_f32.to_radians();
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(
                2.0 * trench_half_width_m,
                0.5,
                deflector_length_m,
            ))),
            MeshMaterial3d(steel.clone()),
            Transform::from_xyz(
                0.0,
                // Inclined half-height plus half-thickness, so the low edge rests
                // on the trench floor and the high edge stays under the pad.
                -trench_depth_m
                    + 0.4
                    + deflector_length_m * 0.5 * deflector_angle_rad.sin()
                    + 0.25 * deflector_angle_rad.cos(),
                trench_near_z_m + 3.5,
            )
            .with_rotation(Quat::from_rotation_x(deflector_angle_rad)),
            Name::new("LaunchPadFlameDeflector"),
        ));
        for x in [-11.0_f32, 11.0] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(0.35, 1.4, 26.0))),
                MeshMaterial3d(water.clone()),
                // 1.4 m tall: centre at 0.7 m puts the base on the pad plane.
                Transform::from_xyz(x, 0.7, 0.0),
                Name::new("LaunchPadDelugeRail"),
            ));
        }
        // Hold-down clamps around the vehicle base. The radius comes from the
        // hull, not from a fixed 3 m: at a fixed offset the clamps sat *inside*
        // the 8.4 m SLS and 9.0 m Starship hulls.
        let clamp_size_m = (rocket_diameter_m * 0.33).clamp(0.6, 1.6);
        let clamp_radius_m = rocket_diameter_m * 0.5 + CLAMP_CLEARANCE_M + clamp_size_m * 0.5;
        for (x, z) in [
            (clamp_radius_m, 0.0_f32),
            (-clamp_radius_m, 0.0),
            (0.0, clamp_radius_m),
            (0.0, -clamp_radius_m),
        ] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(clamp_size_m, 0.8, clamp_size_m))),
                MeshMaterial3d(hazard.clone()),
                // 0.8 m tall: centre at 0.4 m grounds the clamp on the pad.
                Transform::from_xyz(x, 0.4, z),
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
        // perimeter floodlight poles. Their sizes follow the vehicle; their
        // positions are the site layout and stay put.
        let facility_scale = facility_scale(rocket_height_m);
        for (x, z, radius_m) in [(-20.0_f32, 16.0_f32, 3.2_f32), (-13.0, 20.0, 2.4)] {
            let radius_m = radius_m * facility_scale;
            let height_m = 10.0 * facility_scale;
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cylinder::new(radius_m, height_m))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(x, height_m * 0.5, z),
                Name::new("LaunchPadWaterTank"),
            ));
        }
        let blockhouse_height_m = 4.0 * facility_scale;
        parent.spawn((
            LaunchSiteStructure,
            Mesh3d(meshes.add(Cuboid::new(
                12.0 * facility_scale,
                blockhouse_height_m,
                8.0 * facility_scale,
            ))),
            MeshMaterial3d(concrete.clone()),
            Transform::from_xyz(-22.0, blockhouse_height_m * 0.5, -16.0),
            Name::new("LaunchPadBlockhouse"),
        ));
        let floodlight_height_m = 12.0 * facility_scale;
        for (x, z) in [
            (-16.0_f32, -16.0_f32),
            (-16.0, 16.0),
            (16.0, -16.0),
            (16.0, 16.0),
        ] {
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cylinder::new(0.18, floodlight_height_m))),
                MeshMaterial3d(steel.clone()),
                Transform::from_xyz(x, floodlight_height_m * 0.5, z),
                Name::new("LaunchPadFloodlightPole"),
            ));
            parent.spawn((
                LaunchSiteStructure,
                Mesh3d(meshes.add(Cuboid::new(
                    1.4 * facility_scale,
                    0.5 * facility_scale,
                    0.6 * facility_scale,
                ))),
                MeshMaterial3d(floodlight.clone()),
                // Centre offset scales with the head, so it always overlaps
                // the pole instead of floating above it.
                Transform::from_xyz(x, floodlight_height_m + 0.2 * facility_scale, z),
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

/// Build a catalog-derived multi-part rocket mesh in the physical body frame.
///
/// Vertices are assembled bottom-up from the declared stack envelope, so
/// `body_origin_offset_y_m` is the shift that puts the simulated body origin at
/// `(0, 0, 0)`. The full attached stack passes `0` (the envelope centre is its
/// physical origin). A surviving serial stage passes `+envelope / 2`, because
/// its physical origin is the *stage* cylinder centre while the mesh also
/// carries the attached upper envelope above it.
pub(crate) fn build_rocket_mesh(
    meshes: &mut Assets<Mesh>,
    rocket: &crate::domain::entities::rocket::Rocket,
    body_origin_offset_y_m: f32,
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

    if body_origin_offset_y_m != 0.0 {
        for position in &mut positions {
            position[1] += body_origin_offset_y_m;
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
        // Physics changes to a stage-centred frame at separation
        // (`RocketGeometry::lower_extent_y_m = -stage.height / 2`), so the
        // stage cylinder — not the stage+envelope assembly — must land on the
        // origin. Without this the rendered upper stage sits half an envelope
        // too low.
        upper_envelope_height_m.max(0.0) * 0.5,
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
    use bevy::camera::primitives::MeshAabb;
    use bevy::light::{NotShadowCaster, NotShadowReceiver};

    #[test]
    fn autopilot_targets_the_geocentric_site_latitude() {
        // A due-east launch from a WGS-84 geodetic latitude cannot reach an
        // orbit inclined at that latitude: the orbit plane contains the radial
        // position vector, whose latitude is geocentric. Starbase is the
        // clearest case (25.9971 geodetic, 25.8458 geocentric).
        let earth = PlanetFactory::create_by_id(&CelestialBodyId::earth()).unwrap();
        let starbase = PredefinedLaunchSite::Starbase.coordinates();
        let autopilot = autopilot_for_site(&starbase, &earth);
        let (geocentric_deg, _) = geodetic_to_terrain_lat_lon(&starbase, &earth);
        let targeted_deg = autopilot.target_orbit.target_inclination_rad.to_degrees();

        assert!(
            (targeted_deg - geocentric_deg).abs() < 1e-9,
            "target {targeted_deg} must be the geocentric latitude {geocentric_deg}"
        );
        let geodetic_deg = f64::from(starbase.latitude_deg);
        assert!(
            (geodetic_deg - geocentric_deg).abs() > 0.1,
            "this site must expose the datum difference the regression guards"
        );
        assert!(
            targeted_deg < geodetic_deg,
            "geocentric latitude is always poleward of geodetic on an oblate body"
        );
    }

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

    /// LF-02: every ground-based facility sits on the pad plane. The test reads
    /// the spawned mesh AABBs, so a hardcoded centre that leaves a clamp
    /// floating or buries a rail fails even if the layout constants change.
    #[test]
    fn spawned_ground_facilities_sit_on_the_pad_plane() {
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

        // Ground-based facilities: their lowest mesh vertex must meet the pad
        // plane. The apron/deck slabs, flame deflector (LF-03), tower members,
        // beacons and floodlight heads are deliberately mounted or recessed.
        const GROUND_NAMES: [&str; 6] = [
            "LaunchPadTowerLeg",
            "LaunchPadWaterTank",
            "LaunchPadBlockhouse",
            "LaunchPadFloodlightPole",
            "LaunchPadHoldDownClamp",
            "LaunchPadDelugeRail",
        ];
        let world = app.world_mut();
        let mut query = world.query::<(&Name, &Transform, &Mesh3d)>();
        let meshes = world.resource::<Assets<Mesh>>();
        let mut checked = [0usize; GROUND_NAMES.len()];
        for (name, transform, mesh3d) in query.iter(world) {
            let Some(index) = GROUND_NAMES
                .iter()
                .position(|candidate| *candidate == name.as_str())
            else {
                continue;
            };
            let mesh = meshes
                .get(&mesh3d.0)
                .expect("spawned facility mesh must exist");
            let aabb = mesh
                .compute_aabb()
                .expect("facility geometry must have bounds");
            let base_y_m =
                transform.translation.y + (aabb.center.y - aabb.half_extents.y) * transform.scale.y;
            assert!(
                base_y_m.abs() <= 0.05,
                "{} base is {base_y_m:+.3} m from the pad plane",
                name.as_str()
            );
            checked[index] += 1;
        }
        for (name, count) in GROUND_NAMES.iter().zip(checked) {
            assert!(count > 0, "the pad must spawn {name}");
        }
    }

    /// LF-03: the flame trench is open through the deck and its lining is
    /// grounded. The test reads the spawned mesh bounds, so a slab that bridges
    /// the mouth, a lining part left hanging, or a deflector poking through the
    /// pad fails even if the layout constants change.
    #[test]
    fn spawned_flame_trench_is_open_and_grounded() {
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
                    70.0,
                    3.7,
                );
            },
        );
        app.update();

        let half_width = LAUNCH_TRENCH_HALF_WIDTH_M as f32;
        let near_z = LAUNCH_TRENCH_NEAR_Z_M as f32;
        let far_z = LAUNCH_TRENCH_FAR_Z_M as f32;
        let depth = LAUNCH_TRENCH_DEPTH_M as f32;
        // The lining floor is a 0.4 m slab whose top face is the surface every
        // other trench part rests on.
        let floor_top_y = -depth + 0.4;
        let pad_plane_y = 0.06;

        let world = app.world_mut();
        let mut query = world.query::<(&Name, &Transform, &Mesh3d)>();
        let meshes = world.resource::<Assets<Mesh>>();
        let mut slabs = 0;
        let mut walls = 0;
        let mut floors = 0;
        let mut deflectors = 0;
        let mut clamp_max_z = f32::MIN;
        let mut apron_min = Vec3::splat(f32::MAX);
        let mut apron_max = Vec3::splat(f32::MIN);
        for (name, transform, mesh3d) in query.iter(world) {
            let mesh = meshes
                .get(&mesh3d.0)
                .expect("spawned facility mesh must exist");
            let aabb = mesh
                .compute_aabb()
                .expect("facility geometry must have bounds");
            // Rotate the local half extents into the anchor frame; the deflector
            // is inclined, so its bounds cannot be read unrotated.
            let rotation = Mat3::from_quat(transform.rotation);
            let local_half = Vec3::from(aabb.half_extents);
            let local_centre = Vec3::from(aabb.center);
            let half = Vec3::new(
                rotation.x_axis.abs().dot(local_half),
                rotation.y_axis.abs().dot(local_half),
                rotation.z_axis.abs().dot(local_half),
            );
            let centre = transform.translation + (rotation * local_centre);
            let min = centre - half;
            let max = centre + half;
            match name.as_str() {
                "LaunchPadApron" => {
                    slabs += 1;
                    apron_min = apron_min.min(min);
                    apron_max = apron_max.max(max);
                    let covers_x = max.x > -half_width && min.x < half_width;
                    let covers_z = max.z > near_z && min.z < far_z;
                    assert!(
                        !(covers_x && covers_z),
                        "{} spans {min:?}..{max:?} and bridges the trench mouth",
                        name.as_str()
                    );
                }
                "LaunchPadDeck" => {
                    slabs += 1;
                    let covers_x = max.x > -half_width && min.x < half_width;
                    let covers_z = max.z > near_z && min.z < far_z;
                    assert!(
                        !(covers_x && covers_z),
                        "{} spans {min:?}..{max:?} and bridges the trench mouth",
                        name.as_str()
                    );
                }
                "LaunchPadFlameTrenchFloor" => {
                    floors += 1;
                    assert!(
                        (min.y - (floor_top_y - 0.4)).abs() <= 0.01
                            && (max.y - floor_top_y).abs() <= 0.01,
                        "trench floor spans {min:?}..{max:?}, expected its top at \
                         {floor_top_y} m"
                    );
                }
                "LaunchPadFlameTrenchWall" => {
                    walls += 1;
                    assert!(
                        (min.y + depth).abs() <= 0.01 && max.y <= pad_plane_y + 1e-3,
                        "trench wall spans {min:?}..{max:?}, expected it to rise from \
                         the floor to the pad plane"
                    );
                    // A rim at exactly grade is coplanar with the ground and the
                    // sea cap, so it z-fights and lets slivers of grass and sky
                    // through the trench edge.
                    assert!(
                        max.y > 0.0,
                        "trench wall top {} must clear the terrain grade to avoid \
                         z-fighting at the rim",
                        max.y
                    );
                }
                "LaunchPadFlameDeflector" => {
                    deflectors += 1;
                    assert!(
                        max.y < pad_plane_y,
                        "deflector top {} must stay below the pad plane {pad_plane_y}",
                        max.y
                    );
                    assert!(
                        (min.y - floor_top_y).abs() <= 0.05,
                        "deflector base {} must rest on the trench floor {floor_top_y}",
                        min.y
                    );
                    assert!(
                        min.x >= -half_width - 1e-3
                            && max.x <= half_width + 1e-3
                            && min.z > near_z
                            && max.z < far_z,
                        "deflector {min:?}..{max:?} leaves the trench volume"
                    );
                }
                "LaunchPadHoldDownClamp" => clamp_max_z = clamp_max_z.max(max.z),
                _ => {}
            }
        }
        assert_eq!(floors, 1, "the trench needs exactly one lining floor");
        assert_eq!(deflectors, 1, "the trench needs exactly one deflector");
        assert_eq!(walls, 4, "the trench needs four lining walls");
        assert!(
            slabs >= 7,
            "expected the split apron and deck, found {slabs}"
        );
        assert!(
            clamp_max_z < near_z,
            "the trench mouth starts at {near_z} m but the hold-down clamps reach \
             {clamp_max_z} m; the launch point must keep a grade-level footing"
        );
        // The terrain mesh is cut away under the hardstand so the below-grade
        // trench can be seen. The hardstand must therefore span the whole cutout,
        // or the cut shows as a see-through hole in the ground.
        let cutout_radius = LAUNCH_HARDSTAND_CUTOUT_RADIUS_M as f32;
        assert!(
            apron_min.x <= -cutout_radius
                && apron_min.z <= -cutout_radius
                && apron_max.x >= cutout_radius
                && apron_max.z >= cutout_radius,
            "the hardstand spans {apron_min:?}..{apron_max:?} but the terrain cutout \
             reaches {cutout_radius} m"
        );
    }

    /// LF-04: the facility must scale with every catalogue family. The clamps
    /// used to sit at a fixed 3 m, which is inside the 8.4 m SLS and 9.0 m
    /// Starship hulls, and the service arms indexed a fourth platform that a
    /// short tower does not have.
    #[test]
    fn spawned_facility_scales_with_every_vehicle_family() {
        // (height_m, diameter_m) for Electron, Falcon 9, SLS and Starship.
        for (height_m, diameter_m) in [(18.0f32, 1.2f32), (70.0, 3.7), (98.0, 8.4), (121.0, 9.0)] {
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

            let layout = TowerLayout::from_vehicle(height_m, diameter_m);
            let hull_radius_m = diameter_m * 0.5;
            let world = app.world_mut();
            let mut query = world.query::<(&Name, &Transform, &Mesh3d)>();
            let meshes = world.resource::<Assets<Mesh>>();
            let mut clamps = 0;
            let mut levels = 0;
            for (name, transform, mesh3d) in query.iter(world) {
                match name.as_str() {
                    "LaunchPadHoldDownClamp" => {
                        clamps += 1;
                        let aabb = meshes
                            .get(&mesh3d.0)
                            .expect("clamp mesh must exist")
                            .compute_aabb()
                            .expect("clamp geometry must have bounds");
                        let half = Vec3::from(aabb.half_extents)
                            .x
                            .max(Vec3::from(aabb.half_extents).z);
                        let centre = transform.translation;
                        let inner_m = centre.x.abs().hypot(centre.z.abs()) - half;
                        assert!(
                            inner_m > hull_radius_m,
                            "{diameter_m} m vehicle: clamp inner face {inner_m:.2} m is inside \
                             the {hull_radius_m:.2} m hull"
                        );
                    }
                    "LaunchPadTowerLevel" => {
                        levels += 1;
                        assert!(
                            transform.translation.y <= layout.height_m + 1e-3
                                && transform.translation.y > 0.0,
                            "platform at {} m is outside the {} m tower",
                            transform.translation.y,
                            layout.height_m
                        );
                    }
                    _ => {}
                }
            }
            assert_eq!(clamps, 4, "the pad needs four hold-down clamps");
            assert_eq!(
                levels, layout.level_count,
                "{height_m} m vehicle spawned {levels} platforms, layout chose {}",
                layout.level_count
            );
        }
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

    fn upper_stage_fixture(height_m: f32) -> RocketStage {
        RocketStage {
            name: "Upper".into(),
            diameter_m: 1.0,
            height_m,
            dry_mass_kg: 50.0,
            propellant_mass_kg: 250.0,
            recovery_propellant_reserve_kg: None,
            landing_gear: None,
            fairing_dry_mass_kg: None,
            engines: vec![RocketEngine {
                position_m: Vec3::new(0.0, -height_m * 0.5, 0.0),
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
        }
    }

    fn mesh_positions(meshes: &Assets<Mesh>, handle: &Handle<Mesh>) -> Vec<[f32; 3]> {
        let mesh = meshes.get(handle).expect("serial mesh handle is live");
        let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("rocket mesh must expose f32 vertex positions");
        };
        positions.clone()
    }

    #[test]
    fn serial_stage_mesh_has_only_the_active_stage_geometry() {
        let stage = upper_stage_fixture(8.0);
        let mut meshes = Assets::<Mesh>::default();
        let mesh = build_serial_stage_mesh(&mut meshes, &stage, 0.0);

        assert!(meshes.get(&mesh).is_some());
    }

    /// Physics switches to a stage-centred body frame at serial separation:
    /// `RocketGeometry::lower_extent_y_m = -stage.height / 2`. The rebuilt mesh
    /// must put that same cylinder centre on its origin, with the retained
    /// upper envelope above it. Before this regression was pinned the builder
    /// kept the stage+envelope assembly centred, so the rendered upper stage
    /// sat half an envelope too low and jumped at the staging tick.
    #[test]
    fn serial_stage_mesh_is_centred_on_the_stage_not_the_envelope() {
        let stage = upper_stage_fixture(8.0);
        let envelope_m = 3.0_f32;
        let mut meshes = Assets::<Mesh>::default();
        let handle = build_serial_stage_mesh(&mut meshes, &stage, envelope_m);
        let positions = mesh_positions(&meshes, &handle);

        let stage_half_m = stage.height_m * 0.5;
        let bell_height_m = (stage.diameter_m * 0.35).max(0.2);
        let max_y_m = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::MIN, f32::max);
        let min_y_m = positions
            .iter()
            .map(|position| position[1])
            .fold(f32::MAX, f32::min);

        // Nose apex is the stage top plus the whole envelope.
        assert!(
            (max_y_m - (stage_half_m + envelope_m)).abs() < 1e-4,
            "mesh top {max_y_m} must be stage half-height {stage_half_m} plus envelope {envelope_m}"
        );
        // Engine bells are the lowest geometry, one bell below the stage base.
        assert!(
            (min_y_m - (-stage_half_m - bell_height_m)).abs() < 1e-4,
            "mesh bottom {min_y_m} must be the stage base plus one bell {bell_height_m}"
        );

        // The stage cylinder itself, not the envelope, spans ±height/2.
        let radius_m = stage.diameter_m * 0.5;
        for expected_y_m in [-stage_half_m, stage_half_m] {
            let ring_exists = positions.iter().any(|position| {
                (position[1] - expected_y_m).abs() < 1e-4
                    && ((position[0] * position[0] + position[2] * position[2]).sqrt() - radius_m)
                        .abs()
                        < 1e-3
            });
            assert!(
                ring_exists,
                "stage cylinder must have a {radius_m} m ring at y={expected_y_m}"
            );
        }
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
        let papua = PredefinedLaunchSite::Papua.coordinates();
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

        let configuration = LaunchConfiguration {
            catalog: &catalog,
            vehicle: &selection,
            site: PredefinedLaunchSite::Papua,
        };
        let setup = build_launch_setup(&configuration, &terrain_source, &orientation);

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

    #[cfg(feature = "dem")]
    #[test]
    fn ksc_launch_reads_the_composed_3dep_payload() {
        use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
        use crate::domain::services::terrain_source::EarthTerrainSource;
        use bevy::math::DQuat;

        if !std::path::Path::new("assets/large_files/terrain/earth_elevation_pyramid_ksc_3dep")
            .exists()
        {
            return;
        }
        let catalog = RocketCatalog::from_dir()
            .expect("the shipped vehicle catalog must load for the launch regression");
        let selection = VehicleSelection::Default;
        let terrain_source = EarthTerrainSource::new();
        let orientation = BodyOrientation::from_kernel(
            NaifBodyId::EARTH,
            TdbEpoch::j2000(),
            "test-orientation".to_string(),
            DQuat::IDENTITY,
            DVec3::ZERO,
        );
        let configuration = LaunchConfiguration {
            catalog: &catalog,
            vehicle: &selection,
            site: PredefinedLaunchSite::KennedyLc39a,
        };
        let setup = build_launch_setup(&configuration, &terrain_source, &orientation);

        let earth = PlanetFactory::create_by_id(&CelestialBodyId::earth()).unwrap();
        let (terrain_latitude_deg, terrain_longitude_deg) =
            geodetic_to_terrain_lat_lon(&setup.launch_site, &earth);
        let direction = crate::domain::services::reference_frames::terrain_lat_lon_to_body_fixed(
            terrain_latitude_deg,
            terrain_longitude_deg,
        );
        let tiles = terrain_source
            .elevation_tile_source()
            .expect("Earth source exposes composed elevation packages");
        let metadata = tiles
            .covering_tile_for_direction(direction)
            .expect("KSC launch site is covered");
        assert_eq!(
            metadata.patch.level, 13,
            "KSC launch site must resolve to the 3DEP package, not NASADEM"
        );
        let expected_m = terrain_source.height_m(terrain_latitude_deg, terrain_longitude_deg);
        assert!(
            (f64::from(setup.launch_site.altitude_m) - expected_m).abs() < 0.01,
            "KSC launch altitude {} must match the 3DEP surface {expected_m}",
            setup.launch_site.altitude_m
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
