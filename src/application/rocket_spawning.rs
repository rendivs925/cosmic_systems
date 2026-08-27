use crate::application::rocket_config::{RocketCatalog, DEFAULT_VEHICLE_KEY};
use crate::components::rocket::*;
use crate::domain::value_objects::celestial_body_id::CelestialBodyId;
use crate::domain::services::landing_gear::{LandingGear, LandingGearSpec};
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::services::reference_frames::{
    body_fixed_to_inertial_rotation, geodetic_to_body_fixed, surface_velocity_in_planet_inertial,
};
use crate::domain::services::rocket_dynamics::{rocket_inertia_tensor, RocketDynamicsState};
use crate::domain::services::rocket_propulsion::DEFAULT_ULLAGE_SETTLE_TIME_S;
use crate::domain::services::terrain_collision::sample_surface;
use crate::domain::services::terrain_source::TerrainSource;
use crate::domain::value_objects::launch_site_coordinates::predefined_sites;
use crate::domain::value_objects::launch_site_coordinates::LaunchSiteCoordinates;
use crate::infrastructure::bevy_adapters::components::Selectable;
use crate::infrastructure::bevy_adapters::rocket_telemetry::FlightRecorder;
use bevy::asset::{Assets, RenderAssetUsages};
use bevy::math::{DQuat, DVec3};
use bevy::prelude::*;
use bevy_mesh::{Indices, PrimitiveTopology};

/// Flight-recorder ring capacity (entries).
const RECORDER_MAX_ENTRIES: usize = 2_048;
/// Flight-recorder sampling interval (s): ~10 physics ticks at 60 Hz.
const RECORDER_INTERVAL_S: f64 = 1.0 / 6.0;

pub fn spawn_rockets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    catalog: &RocketCatalog,
    selected_key: Option<&str>,
    terrain_source: Option<&dyn TerrainSource>,
) {
    let requested_key = selected_key.unwrap_or(DEFAULT_VEHICLE_KEY);
    let Some(vehicle) = catalog.get(requested_key) else {
        let available = catalog.keys().cloned().collect::<Vec<_>>().join(", ");
        panic!("Unknown vehicle '{requested_key}'. Available vehicles: {available}");
    };
    let rocket = vehicle.rocket.clone();
    // Attached payload hardware (fairing) rides with the vehicle mass until
    // jettison; one authority shared by consumption/staging/jettison.
    let attached_payload_kg = vehicle.fairing_dry_mass_kg.unwrap_or(0.0);

    // Create a proper multi-part rocket mesh from the vehicle configuration.
    let mesh_handle = build_rocket_mesh(meshes, &rocket);

    // Create rocket material: white painted hull, lit by the sun so the body
    // shades correctly (cylinder silhouette reads as a rocket, not a ghost).
    // Slightly lower base value + higher roughness to prevent blowout under
    // 100 klx sun + sky ambient.
    let material = StandardMaterial {
        base_color: Color::srgb(0.78, 0.78, 0.8),
        metallic: 0.05,
        perceptual_roughness: 0.6,
        ..default()
    };
    let material_handle = materials.add(material);

    // The launch site is defined in Earth body-fixed geodetic coordinates, then
    // converted once into the authoritative planet-centered inertial frame.
    // Collision and terrain convert back through the same reference-frame API.
    let earth = PlanetFactory::create_by_name("Earth").unwrap();
    let ksc = predefined_sites::kennedy_space_center();
    let earth_radius_m = earth.radius_km as f64 * 1000.0;
    let terrain_sample = terrain_source.map(|source| {
        sample_surface(
            source,
            ksc.latitude_deg as f64,
            ksc.longitude_deg as f64,
            earth_radius_m,
        )
    });
    let terrain_elevation_m = terrain_sample
        .map(|sample| sample.height_m)
        .unwrap_or(ksc.altitude_m as f64);
    let launch_site = LaunchSiteCoordinates::new(
        ksc.planet_name.clone(),
        ksc.latitude_deg,
        ksc.longitude_deg,
        terrain_elevation_m as f32,
    );
    let position_bf = geodetic_to_body_fixed(&launch_site, &earth).normalize()
        * (earth_radius_m + terrain_elevation_m);
    let body_to_inertial = body_fixed_to_inertial_rotation(&earth, 0.0);
    let position_m = body_to_inertial * position_bf;

    // Stand vertical on the pad: body +Y aligned with the local up direction
    // (radial). Guidance's launch target is the same attitude, so the
    // closed-loop ascent starts from zero attitude error.
    let launch_up = terrain_sample
        .map(|sample| body_to_inertial * sample.normal)
        .unwrap_or_else(|| position_m.normalize());
    let launch_attitude = DQuat::from_rotation_arc(DVec3::Y, launch_up);
    let surface_velocity_mps = surface_velocity_in_planet_inertial(position_m, &earth);

    // The fairing rides as structure until jettison, so it joins the dry
    // input of the geometric inertia model (documented approximation).
    let total_mass_kg = (rocket.total_mass_kg() + attached_payload_kg) as f64;
    let radius_m = (rocket.diameter_m / 2.0) as f64;
    let (inertia, com) = rocket_inertia_tensor(
        (rocket.total_dry_mass_kg() + attached_payload_kg) as f64,
        rocket.total_propellant_mass_kg() as f64,
        radius_m,
        rocket.height_m as f64,
    );
    let dynamics = RocketDynamicsState::new(
        position_m,
        surface_velocity_mps,
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
                vehicle: rocket.clone(),
                active_stage: 0,
                propellant_remaining_kg,
                throttle: 0.0,
                gimbal_pitch_rad: 0.0,
                gimbal_yaw_rad: 0.0,
                // Gate starts open: the first (pad) ignition needs no ullage.
                time_since_separation_s: DEFAULT_ULLAGE_SETTLE_TIME_S,
                ullage_settle_time_s: DEFAULT_ULLAGE_SETTLE_TIME_S,
                separations_count: 0,
                attached_payload_kg,
            },
            ForceAccumulator::default(),
            TorqueAccumulator::default(),
            GravityAcceleration::default(),
            RocketPlanetBinding {
                planet_name: CelestialBodyId::earth(),
            },
            launch_site,
        ))
        .id();

    // Phase 2: Facade + render components.
    // Two inserts because Bevy bundle tuples cap at 15 items.
    commands.entity(entity).insert((
        RocketFacade::default(),
        RocketRenderState::new(dynamics),
        AtmosphereState::default(),
        AerodynamicForces::default(),
        MaxQTracker::default(),
        RocketCommands::default(),
        RocketAutopilot::default(),
        TerrainCollisionState::default(),
        // The vehicle spawns standing on the pad: the resting-contact
        // constraint holds it there until thrust exceeds weight (real
        // physics instead of the old crash-exemption hack).
        GroundRest { active: true },
        // Required by update_orbital_elements and guidance_system; without
        // it neither system ever matches the entity.
        OrbitalElements::default(),
        ThermalState::default(),
        AblationState::default(),
        ParachuteState::default(),
        // Required by GroundContactAccess (resolve_ground_contact). Without
        // these, the contact query never matches the vehicle, GroundRest never
        // holds it, and the rocket falls freely through the terrain.
        TipOverState::default(),
        LandingScorecard::default(),
    ));

    // Phase 3: Entry/comms state + render primitives. Vehicles that define a
    // fairing carry one at spawn; `check_fairing_separation` jettisons it.
    commands.entity(entity).insert((
        CommsState::default(),
        RetroPropulsionEffect::default(),
        FlightRecorder::new(RECORDER_MAX_ENTRIES, RECORDER_INTERVAL_S),
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle.clone()),
        Transform::default(),
        Selectable {
            name: rocket.name.clone(),
            selected: false,
        },
    ));
    if let Some(fairing_dry_mass_kg) = vehicle.fairing_dry_mass_kg {
        commands.entity(entity).insert(PayloadFairing {
            dry_mass_kg: fairing_dry_mass_kg,
        });
    }

    // Landing gear: domain assembly from the catalog spec (struts sized
    // against the gross vehicle mass unless the config sets a limit), plus a
    // simple fixed-pose leg strut per configured leg (presentation only —
    // never authoritative for contact).
    if let Some(spec) = vehicle.landing_legs {
        spawn_landing_leg_meshes(
            commands,
            meshes,
            &material_handle,
            entity,
            rocket.height_m,
            rocket.diameter_m / 2.0,
            &spec,
        );
        commands
            .entity(entity)
            .insert(LandingLegs::new(LandingGear::new(spec, total_mass_kg)));
    }
}

/// Build a proper multi-part rocket mesh from the vehicle configuration.
/// The visual frame has y=0 at the base (pad), y=height at the nose.
fn build_rocket_mesh(
    meshes: &mut ResMut<Assets<Mesh>>,
    rocket: &crate::domain::entities::rocket::Rocket,
) -> Handle<Mesh> {
    let hull_radius = rocket.diameter_m / 2.0;
    let total_height = rocket.height_m;

    // Falcon 9 proportions (approximate from config):
    // Stage 1: ~47m (engines at y=3m, top at y~50m)
    // Interstage: ~3m (y=50-53m)
    // Stage 2: ~12m (y=53-65m)
    // Fairing/nose: ~5m (y=65-70m)
    let stage1_height = 47.0;
    let interstage_height = 3.0;
    let stage2_height = 12.0;
    let fairing_height = total_height - stage1_height - interstage_height - stage2_height;

    // Build mesh manually with all required attributes to avoid merge issues
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    // Helper: add a cylinder section at base_y with given height and radius
    let mut add_cylinder = |base_y: f32,
                            height: f32,
                            radius: f32,
                            positions: &mut Vec<[f32; 3]>,
                            normals: &mut Vec<[f32; 3]>,
                            uvs: &mut Vec<[f32; 2]>,
                            indices: &mut Vec<u32>,
                            index_offset: &mut u32| {
        let rings = 16;
        let segments = 32;
        for ring in 0..=rings {
            let y = base_y + height * (ring as f32 / rings as f32);
            let v = ring as f32 / rings as f32;
            for seg in 0..segments {
                let angle = seg as f32 * std::f32::consts::TAU / segments as f32;
                let x = radius * angle.cos();
                let z = radius * angle.sin();
                positions.push([x, y, z]);
                let nr = (x * x + z * z).sqrt();
                normals.push([x / nr, 0.0, z / nr]);
                uvs.push([seg as f32 / segments as f32, v]);
            }
        }
        for ring in 0..rings {
            for seg in 0..segments {
                let next_seg = (seg + 1) % segments;
                let a = *index_offset + ring * segments + seg;
                let b = *index_offset + ring * segments + next_seg;
                let c = *index_offset + (ring + 1) * segments + seg;
                let d = *index_offset + (ring + 1) * segments + next_seg;
                indices.extend_from_slice(&[a, c, b, b, c, d]);
            }
        }
        *index_offset += (rings + 1) * segments;
    };

    // Helper: add a cone at (center_x, center_z) with base at base_y, apex at base_y + height
    let mut add_cone = |center_x: f32,
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
            let normal_len = (nx * nx + nz * nz + 0.25).sqrt();
            normals.push([nx / normal_len, -0.5 / normal_len, nz / normal_len]);
            uvs.push([seg as f32 / segments as f32, 0.0]);
        }
        for seg in 0..segments {
            let next_seg = (seg + 1) % segments;
            let a = apex_idx;
            let b = *index_offset + seg;
            let c = *index_offset + next_seg;
            indices.extend_from_slice(&[a, b, c]);
        }
        *index_offset += segments;
    };

    let mut idx_offset = 0u32;

    // Stage 1 (white)
    add_cylinder(
        0.0,
        stage1_height,
        hull_radius,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut idx_offset,
    );

    // Interstage (dark band)
    add_cylinder(
        stage1_height,
        interstage_height,
        hull_radius,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut idx_offset,
    );

    // Stage 2 (white)
    add_cylinder(
        stage1_height + interstage_height,
        stage2_height,
        hull_radius,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut idx_offset,
    );

    // Fairing / nose cone
    add_cone(
        0.0,
        0.0,
        stage1_height + interstage_height + stage2_height,
        fairing_height,
        hull_radius,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut idx_offset,
    );

    // Engine bells at the base (9 Merlin 1D engines on octaweb ring, radius 1.2m)
    let engine_y = 3.0;
    let engine_radius = 0.65;
    let engine_height = 2.0;
    let engine_ring_radius = 1.2;

    for i in 0..rocket.stages[0].engines.len() {
        let angle = i as f32 * std::f32::consts::TAU / rocket.stages[0].engines.len() as f32;
        let x = engine_ring_radius * angle.cos();
        let z = engine_ring_radius * angle.sin();
        // Cone base at engine_y - engine_height, apex at engine_y
        add_cone(
            x,
            z,
            engine_y - engine_height,
            engine_height,
            engine_radius,
            &mut positions,
            &mut normals,
            &mut uvs,
            &mut indices,
            &mut idx_offset,
        );
    }

    // Stage 2 engine (Merlin Vacuum) at y=12m body frame → y=47m visual
    let stage2_engine_y = 47.0;
    let stage2_engine_radius = 0.8;
    let stage2_engine_height = 2.5;
    add_cone(
        0.0,
        0.0,
        stage2_engine_y - stage2_engine_height,
        stage2_engine_height,
        stage2_engine_radius,
        &mut positions,
        &mut normals,
        &mut uvs,
        &mut indices,
        &mut idx_offset,
    );

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

/// Spawn one visual strut per configured landing leg as a child of the
/// rocket, angled from the lower hull out to the foot radius. The pose is
/// static (always shown deployed); collision/physics never read these
/// entities.
fn spawn_landing_leg_meshes(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    material: &Handle<StandardMaterial>,
    parent: Entity,
    height_m: f32,
    hull_radius_m: f32,
    spec: &LandingGearSpec,
) {
    let count = spec.count.max(1);
    let stroke_m = spec.stroke_m as f32;
    let base_radius_m = spec.base_radius_m as f32;
    // Rocket mesh is offset so base is at y=0, top at y=height_m.
    // Legs attach at the base (y=0) and extend downward.
    let root_y = stroke_m * 0.25;
    let foot_y = -stroke_m * 0.6;

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
