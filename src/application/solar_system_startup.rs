// Re-export functionality from split modules
pub use super::rocket_spawning::*;
pub use super::texture_config::*;

use crate::application::material_factory::*;
use crate::application::mesh_factory::*;
use crate::application::starfield::spawn_starfield;
use crate::domain::entities::planet::{BodyClass, Planet};
use crate::domain::services::ephemeris::{NaifBodyId, TdbEpoch};
use crate::domain::services::physics;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::value_objects::physical_scale::PhysicalScale;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use crate::infrastructure::bevy_adapters::ephemeris::{EphemerisAuthority, EphemerisSnapshot};
use crate::infrastructure::bevy_adapters::planet_systems::{
    solar_map_position_from_snapshot, solar_map_render_translation,
};

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::DVec3;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;

#[derive(Resource)]
pub struct SolarCameraEnabled(pub bool);

/// Direct normal illuminance from the Sun at Earth's mean orbital distance,
/// before atmospheric attenuation.
pub(crate) const SUN_ILLUMINANCE_AT_EARTH_LUX: f32 = 127_000.0;

pub(crate) fn solar_light_luminous_power_lm(solar_params: &SolarSystemParameters) -> f32 {
    // Bevy point lights use total luminous power (lm). At one rendered AU this
    // yields the measured top-of-atmosphere solar illuminance: E = Phi / 4pi r^2.
    let earth_orbit_units = solar_params.au_to_units(1.0);
    4.0 * std::f32::consts::PI * SUN_ILLUMINANCE_AT_EARTH_LUX * earth_orbit_units.powi(2)
}

pub(crate) fn solar_surface_luminance_nits(solar_params: &SolarSystemParameters) -> f32 {
    // A uniformly radiating sphere has Phi = 4pi^2 r^2 L. Derive the visible
    // surface luminance from the same luminous power used by the point source.
    let radius_units = physics::calculate_sun_visual_radius(solar_params);
    solar_light_luminous_power_lm(solar_params)
        / (4.0 * std::f32::consts::PI.powi(2) * radius_units.powi(2))
}

// Setup scene for solar system simulation
#[expect(
    clippy::too_many_arguments,
    reason = "This startup system receives independent Bevy resources required for scene composition."
)]
pub fn setup_space(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    #[cfg(not(target_arch = "wasm32"))] asset_server: Res<AssetServer>,
    solar_camera_enabled: Option<Res<SolarCameraEnabled>>,
    rocket_mode: Option<Res<RocketMode>>,
    #[cfg(not(target_arch = "wasm32"))] ephemeris_authority: Res<EphemerisAuthority>,
    #[cfg(not(target_arch = "wasm32"))] ephemeris_snapshot: Res<EphemerisSnapshot>,
) {
    // Insert solar system parameters as a resource
    let solar_params = SolarSystemParameters::for_visualization();
    commands.insert_resource(solar_params.clone());
    commands.insert_resource(SolarMapRenderOrigin::default());

    // Central physical scale (meters <-> display units) derived from the
    // authoritative solar-system parameters. Reused by all rocket/terrain
    // subsystems (AGENTS.md sections 15 and 39).
    let physical_scale = PhysicalScale::from_solar_parameters(&solar_params);
    commands.insert_resource(physical_scale);

    // Set up dark space environment with restrained ambient light for premium contrast.
    commands.insert_resource(ClearColor(Color::srgb(0.005, 0.005, 0.01))); // Extremely dark space
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.2, 0.25, 0.3),
        brightness: 0.07,
        affects_lightmapped_meshes: true,
    });

    let solar_camera_active = solar_camera_enabled
        .as_deref()
        .is_none_or(|enabled| enabled.0);

    // Camera positioned to view the full set of orbits on load.
    // Craft mode keeps this controller for solar systems that query it, but disables rendering.
    commands.spawn((
        Camera3d::default(),
        Camera {
            is_active: solar_camera_active,
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            far: 10_000_000.0,
            ..default()
        }),
        Msaa::Sample4,
        Tonemapping::TonyMcMapface,
        Bloom {
            intensity: 0.08,
            prefilter: BloomPrefilter {
                threshold: 0.85,
                threshold_softness: 0.15,
            },
            ..Bloom::NATURAL
        },
        Transform::from_xyz(0.0, 120000.0, 1500000.0).looking_at(Vec3::ZERO, Vec3::Y),
        CameraController {
            mode: CameraMode::FreeFlight,
            speed: 5000.0, // Increased base speed for easier navigation
            sensitivity: 0.0015,
            velocity: Vec3::ZERO,
            target_entity: None,
            orbit_distance: 300.0,
            orbit_angle: 0.0,
            acceleration: 10.0,           // Smooth acceleration
            deceleration: 8.0,            // Smooth deceleration
            adaptive_speed_enabled: true, // Auto-adjust speed based on distance
            min_speed: 50.0,              // Minimum speed for close-up viewing
            max_speed: 50000.0,           // Maximum speed for far travel
            zoom_sensitivity: 50.0,       // Mouse wheel zoom multiplier
        },
    ));

    // The Sun is an isotropic point emitter at solar-map scale. Its luminous
    // power is calibrated to direct sunlight at one rendered AU.
    commands.spawn((
        PointLight {
            intensity: solar_light_luminous_power_lm(&solar_params),
            shadows_enabled: false, // Disable shadows for better performance
            color: Color::srgb(1.0, 1.0, 0.98), // Pure sunlight
            range: 4000000.0, // Extended range to reach outer planets with correct sun-facing light
            // Preserve the Sun's apparent diameter in specular highlights.
            radius: physics::calculate_sun_visual_radius(&solar_params),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
        SolarMapLight,
    ));

    spawn_starfield(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut images,
        &solar_params,
    );

    // Create all planets and moons using the factory
    let planets = PlanetFactory::get_planets();
    let moons = PlanetFactory::get_moons();

    // Combine planets and moons
    let all_celestial_bodies = [planets, moons].concat();

    #[cfg(not(target_arch = "wasm32"))]
    let mut entity_map: HashMap<String, Entity> = HashMap::new();
    #[cfg(target_arch = "wasm32")]
    let entity_map: HashMap<String, Entity> = HashMap::new();
    #[cfg(not(target_arch = "wasm32"))]
    let mut position_map: HashMap<String, DVec3> = HashMap::new();
    #[cfg(target_arch = "wasm32")]
    let position_map: HashMap<String, DVec3> = HashMap::new();
    #[cfg(not(target_arch = "wasm32"))]
    let mut axial_tilts: HashMap<String, f32> = HashMap::new();
    #[cfg(target_arch = "wasm32")]
    let mut axial_tilts: HashMap<String, f32> = HashMap::new();
    for planet in &all_celestial_bodies {
        axial_tilts.insert(planet.name.clone(), planet.axial_tilt_deg);
    }

    #[cfg(target_arch = "wasm32")]
    {
        commands.insert_resource(SpawnQueue {
            pending: all_celestial_bodies.into(),
            entity_map,
            position_map,
            axial_tilts,
            enable_earth_flight_environment: rocket_mode.is_some(),
            spawn_per_frame: 1,
        });
        return;
    }

    // Spawn each celestial body (planets and moons)
    #[cfg(not(target_arch = "wasm32"))]
    for planet in all_celestial_bodies {
        spawn_celestial_body(
            planet,
            &mut commands,
            &mut meshes,
            &mut materials,
            #[cfg(not(target_arch = "wasm32"))]
            &asset_server,
            &solar_params,
            &physical_scale,
            #[cfg(not(target_arch = "wasm32"))]
            &ephemeris_authority,
            #[cfg(not(target_arch = "wasm32"))]
            &ephemeris_snapshot,
            &mut entity_map,
            &mut position_map,
            &axial_tilts,
            rocket_mode.is_some(),
            DVec3::ZERO,
        );
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
pub(crate) struct SpawnQueue {
    pending: VecDeque<Planet>,
    entity_map: HashMap<String, Entity>,
    position_map: HashMap<String, DVec3>,
    axial_tilts: HashMap<String, f32>,
    enable_earth_flight_environment: bool,
    spawn_per_frame: usize,
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_bodies_progressively(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    solar_params: Res<SolarSystemParameters>,
    ephemeris_authority: Res<EphemerisAuthority>,
    ephemeris_snapshot: Res<EphemerisSnapshot>,
    render_origin: Res<SolarMapRenderOrigin>,
    mut queue: ResMut<SpawnQueue>,
) {
    let SpawnQueue {
        pending,
        entity_map,
        position_map,
        axial_tilts,
        enable_earth_flight_environment,
        spawn_per_frame,
    } = &mut *queue;

    for _ in 0..*spawn_per_frame {
        let Some(planet) = pending.pop_front() else {
            return;
        };
        spawn_celestial_body(
            planet,
            &mut commands,
            &mut meshes,
            &mut materials,
            &solar_params,
            &PhysicalScale::from_solar_parameters(&solar_params),
            &ephemeris_authority,
            &ephemeris_snapshot,
            entity_map,
            position_map,
            axial_tilts,
            *enable_earth_flight_environment,
            render_origin.position_units,
        );
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Celestial spawning reuses the startup context without introducing a duplicate manager."
)]
fn spawn_celestial_body(
    planet: Planet,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    #[cfg(not(target_arch = "wasm32"))] asset_server: &AssetServer,
    solar_params: &SolarSystemParameters,
    physical_scale: &PhysicalScale,
    ephemeris_authority: &EphemerisAuthority,
    ephemeris_snapshot: &EphemerisSnapshot,
    entity_map: &mut HashMap<String, Entity>,
    position_map: &mut HashMap<String, DVec3>,
    axial_tilts: &HashMap<String, f32>,
    enable_earth_flight_environment: bool,
    render_origin_units: DVec3,
) {
    let visual_radius = if planet.name == "Sun" {
        physics::calculate_sun_visual_radius(solar_params)
    } else {
        physics::calculate_visual_radius(&planet, solar_params)
    };

    let parent_position = planet
        .parent_entity
        .as_ref()
        .and_then(|parent_name| position_map.get(parent_name).copied())
        .unwrap_or(DVec3::ZERO);
    let parent_tilt = planet
        .parent_entity
        .as_ref()
        .and_then(|parent_name| axial_tilts.get(parent_name).copied());
    let initial_position = if planet.parent_entity.is_none() {
        solar_map_position_from_snapshot(ephemeris_snapshot, &planet.name, physical_scale)
            .unwrap_or_else(|| {
                panic!(
                    "missing DE440 solar-map state for primary body {} at startup",
                    planet.name
                )
            })
    } else {
        physics::calculate_planet_position_f64(
            &planet,
            0.0,
            solar_params,
            parent_position,
            parent_tilt,
        )
    };
    let textures = get_planet_textures(&planet.name);
    let has_albedo = textures.albedo.is_some();

    #[cfg(not(target_arch = "wasm32"))]
    let albedo_handle = load_texture(asset_server, textures.albedo);
    #[cfg(not(target_arch = "wasm32"))]
    let emissive_handle = load_texture(asset_server, textures.emissive);

    let is_sun = planet.name == "Sun";
    let base_color = planet.color;

    let (metallic, reflectance, perceptual_roughness) = match planet.name.as_str() {
        "Sun" => (0.0, 0.0, 0.0),
        "Mercury" => (0.1, 0.3, 0.8),
        "Venus" => (0.1, 0.75, 0.25),
        "Earth" => (0.05, 0.4, 0.45),
        "Mars" => (0.05, 0.25, 0.6),
        "Jupiter" => (0.0, 0.7, 0.15),
        "Saturn" => (0.0, 0.65, 0.15),
        "Uranus" => (0.0, 0.5, 0.25),
        "Neptune" => (0.0, 0.6, 0.25),
        _ => (0.0, 0.5, 0.7),
    };

    let emissive = if is_sun {
        let luminance_nits = solar_surface_luminance_nits(solar_params);
        LinearRgba::new(luminance_nits, luminance_nits, luminance_nits, 1.0)
    } else if has_albedo {
        LinearRgba::new(0.35, 0.35, 0.35, 1.0)
    } else {
        LinearRgba::BLACK
    };

    #[cfg(not(target_arch = "wasm32"))]
    let emissive_texture = if is_sun || has_albedo {
        albedo_handle.clone()
    } else {
        emissive_handle.clone()
    };

    #[cfg(target_arch = "wasm32")]
    let emissive_path = if is_sun {
        textures.albedo
    } else if has_albedo {
        textures.albedo
    } else {
        textures.emissive
    };

    #[cfg(target_arch = "wasm32")]
    let material = create_planet_material(PlanetMaterialConfig {
        base_color_texture: None,
        normal_map_texture: None,
        emissive_texture: None,
        base_color,
        emissive,
        // Emissive output is evaluated only on the lit material path in Bevy.
        unlit: false,
        metallic,
        reflectance,
        perceptual_roughness,
    });
    #[cfg(not(target_arch = "wasm32"))]
    let material_config = PlanetMaterialConfig {
        base_color_texture: albedo_handle.clone(),
        normal_map_texture: None,
        emissive_texture: emissive_texture.clone(),
        base_color,
        emissive,
        // Emissive output is evaluated only on the lit material path in Bevy.
        unlit: false,
        metallic,
        reflectance,
        perceptual_roughness,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let material = create_planet_material(material_config);

    let material_handle = materials.add(material);

    // Terrain and atmosphere are flight simulation data, not solar-map layers.
    // Earth is the only body configured for them while Rocket mode is active.
    let terrain =
        (enable_earth_flight_environment && planet.name == "Earth").then(PlanetTerrain::earth);

    let mut planet_commands = commands.spawn((
        Mesh3d(create_uv_sphere_mesh(meshes, visual_radius)),
        MeshMaterial3d(material_handle.clone()),
        Transform::from_translation(solar_map_render_translation(
            initial_position,
            render_origin_units,
        )),
        SolarMapPosition(initial_position),
    ));
    planet_commands
        .insert(PlanetComponent {
            domain_planet: planet.clone(),
            material: material_handle.clone(),
            has_texture: has_albedo,
            base_reflectance: reflectance,
            base_roughness: perceptual_roughness,
        })
        .insert(Selectable {
            name: planet.name.clone(),
            selected: false,
        });
    if let Some(terrain) = terrain {
        planet_commands.insert(terrain);
    }
    if enable_earth_flight_environment && planet.name == "Earth" {
        planet_commands.insert(PlanetAtmosphere::default_for("Earth"));
    }
    let planet_entity = planet_commands.id();

    #[cfg(target_arch = "wasm32")]
    {
        let eager = matches!(planet.name.as_str(), "Sun" | "Earth");
        commands
            .entity(planet_entity)
            .insert(PendingMaterialTextures {
                material: material_handle.clone(),
                base_color_texture: None,
                normal_map_texture: None,
                emissive_texture: None,
                base_color_path: textures.albedo,
                normal_map_path: None,
                emissive_path,
                eager,
            });
    }

    entity_map.insert(planet.name.clone(), planet_entity);
    position_map.insert(planet.name.clone(), initial_position);

    if let Some(parent_name) = &planet.parent_entity {
        if let Some(parent_ent) = entity_map.get(parent_name).copied() {
            let orbit_shape = physics::orbit_shape_for(&planet, solar_params);
            let moon_thickness = ORBIT_RIBBON_NEAR_WIDTH_UNITS;
            let moon_segments = ORBIT_RIBBON_SEGMENTS;
            let sampled_moon_orbit = match (
                NaifBodyId::for_catalog_name(&planet.name),
                NaifBodyId::for_catalog_name(parent_name),
            ) {
                (Some(target), Some(center)) => {
                    let epoch = ephemeris_snapshot.epoch.unwrap_or_else(|| {
                        panic!(
                            "missing DE440 snapshot epoch while sampling {} orbit",
                            planet.name
                        )
                    });
                    let (sample_start, sample_span_seconds, sampled_path_closed) =
                        orbit_sample_window(
                            ephemeris_authority,
                            epoch,
                            planet.orbital_period_days as f64 * 86_400.0,
                        );
                    let sampled_path_units = ephemeris_authority
                        .sample_relative_orbit_in_solar_inertial(
                            target,
                            center,
                            sample_start,
                            sample_span_seconds,
                            moon_segments,
                        )
                        .unwrap_or_else(|error| {
                            panic!(
                                "cannot sample DE440 orbit ribbon for {}: {error}",
                                planet.name
                            )
                        })
                        .into_iter()
                        .map(|position_m| physical_scale.solar_meters_to_units_vec3(position_m))
                        .collect::<Vec<_>>();
                    let render_anchor_units =
                        sampled_orbit_render_anchor_units(&sampled_path_units);
                    Some((sampled_path_units, render_anchor_units, sampled_path_closed))
                }
                _ => None,
            };
            let orbit_mesh = if let Some((sampled_path_units, render_anchor_units, closed)) =
                &sampled_moon_orbit
            {
                create_sampled_orbit_ribbon_mesh(
                    meshes,
                    sampled_path_units,
                    *render_anchor_units,
                    ORBIT_LINE_COLOR,
                    moon_thickness,
                    *closed,
                )
            } else {
                create_orbit_ribbon_mesh(
                    meshes,
                    &orbit_shape,
                    ORBIT_LINE_COLOR,
                    moon_thickness,
                    moon_segments,
                )
            };
            let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, true);

            // Create individual material for this moon orbit
            let moon_material = create_orbit_material(
                ORBIT_LINE_COLOR,
                orbit_emissive(ORBIT_LINE_COLOR, 0.03),
                0.06,
            );
            let moon_material_handle = materials.add(moon_material);

            commands
                .spawn((
                    Mesh3d(orbit_mesh),
                    MeshMaterial3d(moon_material_handle.clone()),
                    Transform::from_translation(solar_map_render_translation(
                        *position_map
                            .get(parent_name)
                            .expect("moon parent must have a solar-map position"),
                        render_origin_units,
                    )),
                ))
                .insert(OrbitComponent {
                    radius: orbit_shape.semi_major_axis_units,
                    planet_entity: parent_ent,
                    material: moon_material_handle.clone(),
                    base_color: ORBIT_LINE_COLOR,
                    body_class: BodyClass::Moon,
                    orbit_shape,
                    thickness: ORBIT_RIBBON_NEAR_WIDTH_UNITS,
                    render_anchor_units: sampled_moon_orbit
                        .as_ref()
                        .map_or(DVec3::ZERO, |(_, render_anchor_units, _)| {
                            *render_anchor_units
                        }),
                    segments: ORBIT_RIBBON_SEGMENTS,
                    tilt: orbit_motion.tilt,
                    wobble_speed: orbit_motion.wobble_speed,
                    wobble_amount: orbit_motion.wobble_amount,
                    spin_speed: orbit_motion.spin_speed,
                    phase: orbit_motion.phase,
                    distance_rank: 0.5,
                    sampled_path_units: sampled_moon_orbit
                        .as_ref()
                        .map(|(sampled_path_units, _, _)| sampled_path_units.clone()),
                    sampled_path_closed: sampled_moon_orbit
                        .as_ref()
                        .is_some_and(|(_, _, sampled_path_closed)| *sampled_path_closed),
                })
                .insert(MoonOrbit)
                .insert(Name::new(format!(
                    "Orbit {} around {}",
                    planet.name, parent_name
                )));
        }
    } else if planet.name != "Sun" {
        let orbit_shape = physics::orbit_shape_for(&planet, solar_params);
        let orbit_base_color = ORBIT_LINE_COLOR;
        let planet_thickness = ORBIT_RIBBON_NEAR_WIDTH_UNITS;
        let planet_segments = ORBIT_RIBBON_SEGMENTS;
        let target = NaifBodyId::for_catalog_name(&planet.name).unwrap_or_else(|| {
            panic!(
                "missing DE440 target mapping for primary body {}",
                planet.name
            )
        });
        let epoch = ephemeris_snapshot.epoch.unwrap_or_else(|| {
            panic!(
                "missing DE440 snapshot epoch while sampling {} orbit",
                planet.name
            )
        });
        let (sample_start, sample_span_seconds, sampled_path_closed) = orbit_sample_window(
            ephemeris_authority,
            epoch,
            planet.orbital_period_days as f64 * 86_400.0,
        );
        let sampled_path_units: Vec<DVec3> = ephemeris_authority
            .sample_solar_inertial_orbit(target, sample_start, sample_span_seconds, planet_segments)
            .unwrap_or_else(|error| {
                panic!(
                    "cannot sample DE440 orbit ribbon for {}: {error}",
                    planet.name
                )
            })
            .into_iter()
            .map(|position_m| physical_scale.solar_meters_to_units_vec3(position_m))
            .collect();
        let render_anchor_units = sampled_orbit_render_anchor_units(&sampled_path_units);
        let orbit_mesh = create_sampled_orbit_ribbon_mesh(
            meshes,
            &sampled_path_units,
            render_anchor_units,
            orbit_base_color,
            planet_thickness,
            sampled_path_closed,
        );
        let orbit_material = create_orbit_material(
            orbit_base_color,
            orbit_emissive(orbit_base_color, 0.04),
            0.06,
        );
        let orbit_material_handle = materials.add(orbit_material);
        let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, false);

        commands
            .spawn((
                Mesh3d(orbit_mesh),
                MeshMaterial3d(orbit_material_handle.clone()),
                Transform::from_translation(solar_map_render_translation(
                    render_anchor_units,
                    render_origin_units,
                )),
            ))
            .insert(OrbitComponent {
                radius: orbit_shape.semi_major_axis_units,
                planet_entity,
                material: orbit_material_handle.clone(),
                base_color: orbit_base_color,
                body_class: planet.body_class,
                orbit_shape,
                thickness: ORBIT_RIBBON_NEAR_WIDTH_UNITS,
                render_anchor_units,
                segments: ORBIT_RIBBON_SEGMENTS,
                tilt: orbit_motion.tilt,
                wobble_speed: orbit_motion.wobble_speed,
                wobble_amount: orbit_motion.wobble_amount,
                spin_speed: orbit_motion.spin_speed,
                phase: orbit_motion.phase,
                distance_rank: (orbit_shape.semi_major_axis_units / 15000.0).clamp(0.0, 1.0),
                sampled_path_units: Some(sampled_path_units),
                sampled_path_closed,
            })
            .insert(Name::new(format!("Orbit {}", planet.name)));
    }

    if planet.name == "Saturn" {
        let ring_outer_radius = visual_radius * 2.5;
        let ring_inner_radius = visual_radius * 1.6;
        let ring_texture_path = get_ring_texture_path(&planet.name);
        #[cfg(not(target_arch = "wasm32"))]
        let ring_texture = load_texture(asset_server, ring_texture_path);

        #[cfg(target_arch = "wasm32")]
        let ring_material = create_ring_material(
            None,
            Color::srgba(1.5, 1.4, 1.2, 1.0),
            Color::srgb(0.3, 0.25, 0.18).into(),
        );
        #[cfg(not(target_arch = "wasm32"))]
        let ring_material = create_ring_material(
            ring_texture.clone(),
            Color::srgba(1.5, 1.4, 1.2, 1.0),
            Color::srgb(0.3, 0.25, 0.18).into(),
        );
        let ring_material_handle = materials.add(ring_material);

        commands.entity(planet_entity).with_children(|parent| {
            #[cfg(target_arch = "wasm32")]
            {
                let mut ring_entity = parent.spawn((
                    Mesh3d(create_ring_mesh(
                        meshes,
                        ring_inner_radius,
                        ring_outer_radius,
                    )),
                    MeshMaterial3d(ring_material_handle.clone()),
                    Transform::default(),
                    Selectable {
                        name: "Saturn Rings".to_string(),
                        selected: false,
                    },
                ));
                ring_entity.insert(PendingMaterialTextures {
                    material: ring_material_handle.clone(),
                    base_color_texture: None,
                    normal_map_texture: None,
                    emissive_texture: None,
                    base_color_path: ring_texture_path,
                    normal_map_path: None,
                    emissive_path: None,
                    eager: false,
                });
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                parent.spawn((
                    Mesh3d(create_ring_mesh(
                        meshes,
                        ring_inner_radius,
                        ring_outer_radius,
                    )),
                    MeshMaterial3d(ring_material_handle.clone()),
                    Transform::default(),
                    Selectable {
                        name: "Saturn Rings".to_string(),
                        selected: false,
                    },
                ));
            }
        });
    }

    if let Some(clouds) = get_cloud_layer_config(&planet.name) {
        #[cfg(not(target_arch = "wasm32"))]
        let cloud_texture = load_texture(asset_server, Some(clouds.texture_path));
        #[cfg(not(target_arch = "wasm32"))]
        let cloud_material = create_cloud_material(cloud_texture.clone(), clouds.alpha);
        #[cfg(target_arch = "wasm32")]
        let cloud_material = create_cloud_material(None, clouds.alpha);
        #[cfg(not(target_arch = "wasm32"))]
        if cloud_texture.is_none() {
            return;
        }

        commands.entity(planet_entity).with_children(|parent| {
            let cloud_material_handle = materials.add(cloud_material);

            #[cfg(target_arch = "wasm32")]
            {
                let mut cloud_entity = parent.spawn((
                    Mesh3d(create_uv_sphere_mesh(meshes, visual_radius * clouds.scale)),
                    MeshMaterial3d(cloud_material_handle.clone()),
                    Transform::default(),
                    CloudLayer {
                        rotation_period_hours: clouds.rotation_period_hours,
                    },
                ));
                cloud_entity.insert(PendingMaterialTextures {
                    material: cloud_material_handle.clone(),
                    base_color_texture: None,
                    normal_map_texture: None,
                    emissive_texture: None,
                    base_color_path: Some(clouds.texture_path),
                    normal_map_path: None,
                    emissive_path: None,
                    eager: false,
                });
            }

            #[cfg(not(target_arch = "wasm32"))]
            {
                parent.spawn((
                    Mesh3d(create_uv_sphere_mesh(meshes, visual_radius * clouds.scale)),
                    MeshMaterial3d(cloud_material_handle.clone()),
                    Transform::default(),
                    CloudLayer {
                        rotation_period_hours: clouds.rotation_period_hours,
                    },
                ));
            }
        });
    }
}

// Functions moved to texture_config.rs, terrain_spawning.rs, and rocket_spawning.rs

/// Choose a complete period centered on the active epoch when DE440s covers
/// it. The current 1900-2050 dataset cannot cover Neptune's whole period, so
/// its ribbon remains an honest open arc of the available authority data.
fn orbit_sample_window(
    authority: &EphemerisAuthority,
    epoch: TdbEpoch,
    period_seconds: f64,
) -> (TdbEpoch, f64, bool) {
    let coverage = authority.0.provenance().coverage;
    let coverage_start = TdbEpoch::from_julian_date(coverage.start_julian_date_tdb)
        .expect("validated kernel coverage has a finite start epoch");
    let coverage_end = TdbEpoch::from_julian_date(coverage.end_julian_date_tdb)
        .expect("validated kernel coverage has a finite end epoch");
    let coverage_start_seconds = coverage_start.seconds_since_j2000();
    let coverage_end_seconds = coverage_end.seconds_since_j2000();
    let centered_start_seconds = epoch.seconds_since_j2000() - period_seconds * 0.5;
    let centered_end_seconds = centered_start_seconds + period_seconds;
    if centered_start_seconds >= coverage_start_seconds
        && centered_end_seconds <= coverage_end_seconds
    {
        return (
            TdbEpoch::from_seconds_since_j2000(centered_start_seconds)
                .expect("finite centered sample epoch"),
            period_seconds,
            true,
        );
    }

    (
        coverage_start,
        coverage_end_seconds - coverage_start_seconds,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::bevy_adapters::components::MoonOrbit;
    use crate::infrastructure::plugins::{SharedSimulationPlugin, SolarSystemModePlugin};

    #[test]
    fn solar_light_matches_top_of_atmosphere_illuminance_at_one_au() {
        let solar_params = SolarSystemParameters::for_visualization();
        let one_au_units = solar_params.au_to_units(1.0);
        let illuminance_lux = solar_light_luminous_power_lm(&solar_params)
            / (4.0 * std::f32::consts::PI * one_au_units.powi(2));

        assert!((illuminance_lux - SUN_ILLUMINANCE_AT_EARTH_LUX).abs() < 0.1);
    }

    #[test]
    fn solar_surface_emission_matches_the_calibrated_luminous_power() {
        let solar_params = SolarSystemParameters::for_visualization();
        let radius_units = physics::calculate_sun_visual_radius(&solar_params);
        let reconstructed_power_lm = 4.0
            * std::f32::consts::PI.powi(2)
            * radius_units.powi(2)
            * solar_surface_luminance_nits(&solar_params);

        assert!(
            (reconstructed_power_lm - solar_light_luminous_power_lm(&solar_params)).abs()
                < solar_light_luminous_power_lm(&solar_params) * 1e-6
        );
    }

    #[test]
    fn every_catalog_body_except_the_sun_has_a_spawnable_orbit() {
        let solar_params = SolarSystemParameters::for_visualization();
        let planets = PlanetFactory::get_planets();
        let moons = PlanetFactory::get_moons();

        for moon in &moons {
            let parent_name = moon.parent_entity.as_deref().unwrap();
            assert!(
                planets.iter().any(|planet| planet.name == parent_name),
                "{} has no spawnable parent {}",
                moon.name,
                parent_name
            );
        }

        let bodies_with_orbits = planets
            .iter()
            .chain(moons.iter())
            .filter(|planet| planet.name != "Sun");
        for planet in bodies_with_orbits {
            let orbit = physics::orbit_shape_for(planet, &solar_params);
            assert!(
                orbit.semi_major_axis_units.is_finite() && orbit.semi_major_axis_units > 0.0,
                "{} has no valid orbit radius",
                planet.name
            );
            assert!(
                orbit.eccentricity.is_finite(),
                "{} has invalid eccentricity",
                planet.name
            );
        }
    }

    #[test]
    fn startup_samples_each_primary_orbit_from_de440() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()));
        app.init_asset::<Mesh>();
        app.init_asset::<StandardMaterial>();
        app.init_asset::<Image>();
        app.add_plugins((SharedSimulationPlugin, SolarSystemModePlugin));
        app.world_mut().run_schedule(Startup);

        let mut orbits = app.world_mut().query::<(&OrbitComponent, Has<MoonOrbit>)>();
        let primary_paths: Vec<_> = orbits
            .iter(app.world())
            .filter_map(|(orbit, is_moon)| (!is_moon).then_some(orbit))
            .collect();

        assert_eq!(primary_paths.len(), 8);
        for orbit in primary_paths {
            let path = orbit
                .sampled_path_units
                .as_deref()
                .expect("primary orbit uses the DE440 sampled presentation path");
            assert_eq!(path.len(), ORBIT_RIBBON_SEGMENTS);
            assert!(path.iter().all(|position| position.is_finite()));
        }

        let sampled_moon_paths: Vec<_> = orbits
            .iter(app.world())
            .filter_map(|(orbit, is_moon)| is_moon.then_some(orbit))
            .filter_map(|orbit| orbit.sampled_path_units.as_deref())
            .collect();
        assert_eq!(sampled_moon_paths.len(), 1);
        assert_eq!(sampled_moon_paths[0].len(), ORBIT_RIBBON_SEGMENTS);
        assert!(sampled_moon_paths[0]
            .iter()
            .all(|position| position.is_finite()));
    }
}
