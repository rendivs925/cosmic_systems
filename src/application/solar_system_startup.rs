// Re-export functionality from split modules
pub use super::rocket_spawning::*;
pub use super::terrain_spawning::*;
pub use super::texture_config::*;

use crate::application::material_factory::*;
use crate::application::mesh_factory::*;
use crate::application::starfield::spawn_starfield;
use crate::domain::entities::planet::{BodyClass, Planet};
use crate::domain::services::physics;
use crate::domain::services::planet_factory::PlanetFactory;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;

use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;

#[derive(Resource)]
pub struct SolarCameraEnabled(pub bool);

// Setup scene for solar system simulation
pub fn setup_space(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    asset_server: Res<AssetServer>,
    solar_camera_enabled: Option<Res<SolarCameraEnabled>>,
) {
    // Insert solar system parameters as a resource
    let solar_params = SolarSystemParameters::for_visualization();
    commands.insert_resource(solar_params.clone());

    // Set up dark space environment with restrained ambient light for premium contrast.
    commands.insert_resource(ClearColor(Color::srgb(0.005, 0.005, 0.01))); // Extremely dark space
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.2, 0.25, 0.3),
        brightness: 0.07,
        affects_lightmapped_meshes: true,
    });

    let solar_camera_active = solar_camera_enabled
        .as_deref()
        .map_or(true, |enabled| enabled.0);

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

    // Sun as the main light source with enormous intensity for massive astronomical distances
    commands.spawn((
        PointLight {
            intensity: 500000000.0, // Enormous intensity for massive astronomical-scale illumination
            shadows_enabled: false, // Disable shadows for better performance
            color: Color::srgb(1.0, 1.0, 0.98), // Pure sunlight
            range: 4000000.0, // Extended range to reach outer planets with correct sun-facing light
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 0.0),
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

    let mut entity_map: HashMap<String, Entity> = HashMap::new();
    let mut position_map: HashMap<String, Vec3> = HashMap::new();
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
            &asset_server,
            &solar_params,
            &mut entity_map,
            &mut position_map,
            &axial_tilts,
        );
    }

    // Spawn terrain patches
    spawn_terrain_patches(
        &mut commands,
        &asset_server,
        &mut meshes,
        &mut materials,
        &mut images,
        &entity_map,
    );
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
pub(crate) struct SpawnQueue {
    pending: VecDeque<Planet>,
    entity_map: HashMap<String, Entity>,
    position_map: HashMap<String, Vec3>,
    axial_tilts: HashMap<String, f32>,
    spawn_per_frame: usize,
}

#[cfg(target_arch = "wasm32")]
pub fn spawn_bodies_progressively(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    solar_params: Res<SolarSystemParameters>,
    mut queue: ResMut<SpawnQueue>,
) {
    let SpawnQueue {
        pending,
        entity_map,
        position_map,
        axial_tilts,
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
            &asset_server,
            &solar_params,
            entity_map,
            position_map,
            axial_tilts,
        );
    }
}

fn spawn_celestial_body(
    planet: Planet,
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    solar_params: &SolarSystemParameters,
    entity_map: &mut HashMap<String, Entity>,
    position_map: &mut HashMap<String, Vec3>,
    axial_tilts: &HashMap<String, f32>,
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
        .unwrap_or(Vec3::ZERO);
    let parent_tilt = planet
        .parent_entity
        .as_ref()
        .and_then(|parent_name| axial_tilts.get(parent_name).copied());
    let initial_position = physics::calculate_planet_position(
        &planet,
        0.0,
        solar_params,
        parent_position,
        parent_tilt,
    );
    let textures = get_planet_textures(&planet.name);
    let has_albedo = textures.albedo.is_some();

    #[cfg(not(target_arch = "wasm32"))]
    let albedo_handle = load_texture(asset_server, textures.albedo);
    #[cfg(not(target_arch = "wasm32"))]
    let emissive_handle = load_texture(asset_server, textures.emissive);

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

    let emissive = if planet.name == "Sun" {
        LinearRgba::new(1.0, 1.0, 0.8, 1.0)
    } else if has_albedo {
        LinearRgba::new(0.35, 0.35, 0.35, 1.0)
    } else {
        LinearRgba::BLACK
    };

    #[cfg(not(target_arch = "wasm32"))]
    let emissive_texture = if planet.name == "Sun" || has_albedo {
        albedo_handle.clone()
    } else {
        emissive_handle.clone()
    };

    #[cfg(target_arch = "wasm32")]
    let emissive_path = if planet.name == "Sun" {
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
        unlit: planet.name == "Sun",
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
        unlit: planet.name == "Sun",
        metallic,
        reflectance,
        perceptual_roughness,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let material = create_planet_material(material_config);

    let material_handle = materials.add(material);

    let planet_entity = commands
        .spawn((
            Mesh3d(create_uv_sphere_mesh(meshes, visual_radius)),
            MeshMaterial3d(material_handle.clone()),
            Transform::from_translation(initial_position),
        ))
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
        })
        .id();

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
            let moon_thickness = orbit_shape.semi_major_axis_units * 0.0001;
            let moon_segments = 256;
            #[cfg(not(target_arch = "wasm32"))]
            let orbit_mesh = create_orbit_ribbon_mesh(meshes, &orbit_shape, ORBIT_LINE_COLOR, moon_thickness, moon_segments);
            #[cfg(target_arch = "wasm32")]
            let orbit_mesh = create_placeholder_orbit_mesh(meshes);
            let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, true);

            // Create individual material for this moon orbit
            let moon_material = create_orbit_material(
                ORBIT_LINE_COLOR,
                orbit_emissive(ORBIT_LINE_COLOR, 0.03),
                0.06,
            );
            let moon_material_handle = materials.add(moon_material);

            #[cfg(target_arch = "wasm32")]
            let orbit_entity = commands
                .spawn((
                    Mesh3d(orbit_mesh.clone()),
                    MeshMaterial3d(moon_material_handle.clone()),
                    Transform::default(),
                ))
                .insert(OrbitComponent {
                    radius: orbit_shape.semi_major_axis_units,
                    planet_entity: parent_ent,
                    material: moon_material_handle.clone(),
                    base_color: ORBIT_LINE_COLOR,
                    body_class: BodyClass::Moon,
                    orbit_shape,
                    thickness: orbit_shape.semi_major_axis_units * 0.0001,
                    segments: 256,
                    tilt: orbit_motion.tilt,
                    wobble_speed: orbit_motion.wobble_speed,
                    wobble_amount: orbit_motion.wobble_amount,
                    spin_speed: orbit_motion.spin_speed,
                    phase: orbit_motion.phase,
                    distance_rank: 0.5,
                })
                .insert(MoonOrbit)
                .insert(Name::new(format!(
                    "Orbit {} around {}",
                    planet.name, parent_name
                )))
                .id();

            #[cfg(not(target_arch = "wasm32"))]
            {
                commands
                    .spawn((
                        Mesh3d(orbit_mesh.clone()),
                        MeshMaterial3d(moon_material_handle.clone()),
                        Transform::default(),
                    ))
                    .insert(OrbitComponent {
                        radius: orbit_shape.semi_major_axis_units,
                        planet_entity: parent_ent,
                        material: moon_material_handle.clone(),
                        base_color: ORBIT_LINE_COLOR,
                        body_class: BodyClass::Moon,
                        orbit_shape,
                        thickness: orbit_shape.semi_major_axis_units * 0.0001,
                        segments: 256,
                        tilt: orbit_motion.tilt,
                        wobble_speed: orbit_motion.wobble_speed,
                        wobble_amount: orbit_motion.wobble_amount,
                        spin_speed: orbit_motion.spin_speed,
                        phase: orbit_motion.phase,
                        distance_rank: 0.5,
                    })
                    .insert(MoonOrbit)
                    .insert(Name::new(format!(
                        "Orbit {} around {}",
                        planet.name, parent_name
                    )));
            }

            #[cfg(target_arch = "wasm32")]
            {
                commands.entity(orbit_entity).insert(PendingOrbitMesh {
                    mesh: orbit_mesh,
                    orbit_shape,
                    color: ORBIT_LINE_COLOR,
                    segments: 128,
                });
            }
        }
    } else if planet.name != "Sun" {
        let orbit_shape = physics::orbit_shape_for(&planet, solar_params);
        let orbit_base_color = ORBIT_LINE_COLOR;
        let planet_thickness = orbit_shape.semi_major_axis_units * 0.0001;
        let planet_segments = 256;
        #[cfg(not(target_arch = "wasm32"))]
        let orbit_mesh = create_orbit_ribbon_mesh(meshes, &orbit_shape, orbit_base_color, planet_thickness, planet_segments);
        #[cfg(target_arch = "wasm32")]
        let orbit_mesh = create_placeholder_orbit_mesh(meshes);
        let orbit_material = create_orbit_material(
            orbit_base_color,
            orbit_emissive(orbit_base_color, 0.04),
            0.06,
        );
        let orbit_material_handle = materials.add(orbit_material);
        let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, false);

        #[cfg(target_arch = "wasm32")]
        let orbit_entity = commands
            .spawn((
                Mesh3d(orbit_mesh.clone()),
                MeshMaterial3d(orbit_material_handle.clone()),
                Transform::default(),
            ))
            .insert(OrbitComponent {
                radius: orbit_shape.semi_major_axis_units,
                planet_entity,
                material: orbit_material_handle.clone(),
                base_color: orbit_base_color,
                body_class: planet.body_class,
                orbit_shape,
                thickness: orbit_shape.semi_major_axis_units * 0.0001,
                segments: 256,
                tilt: orbit_motion.tilt,
                wobble_speed: orbit_motion.wobble_speed,
                wobble_amount: orbit_motion.wobble_amount,
                spin_speed: orbit_motion.spin_speed,
                phase: orbit_motion.phase,
                distance_rank: (orbit_shape.semi_major_axis_units / 15000.0).clamp(0.0, 1.0),
            })
            .insert(Name::new(format!("Orbit {}", planet.name)))
            .id();

        #[cfg(not(target_arch = "wasm32"))]
        {
            commands
                .spawn((
                    Mesh3d(orbit_mesh.clone()),
                    MeshMaterial3d(orbit_material_handle.clone()),
                    Transform::default(),
                ))
                .insert(OrbitComponent {
                    radius: orbit_shape.semi_major_axis_units,
                    planet_entity,
                    material: orbit_material_handle.clone(),
                    base_color: orbit_base_color,
                    body_class: planet.body_class,
                    orbit_shape,
                    thickness: orbit_shape.semi_major_axis_units * 0.0001,
                    segments: 256,
                    tilt: orbit_motion.tilt,
                    wobble_speed: orbit_motion.wobble_speed,
                    wobble_amount: orbit_motion.wobble_amount,
                    spin_speed: orbit_motion.spin_speed,
                    phase: orbit_motion.phase,
                    distance_rank: (orbit_shape.semi_major_axis_units / 15000.0).clamp(0.0, 1.0),
                })
                .insert(Name::new(format!("Orbit {}", planet.name)));
        }

        #[cfg(target_arch = "wasm32")]
        {
            commands.entity(orbit_entity).insert(PendingOrbitMesh {
                mesh: orbit_mesh,
                orbit_shape,
                color: orbit_base_color,
                segments: 128,
            });
        }
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
