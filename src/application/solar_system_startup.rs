use crate::application::material_factory::*;
use crate::application::mesh_factory::*;
use crate::domain::entities::planet::Planet;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;
use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::render::render_asset::RenderAssetUsages;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::HashMap;
#[cfg(target_arch = "wasm32")]
use std::collections::VecDeque;
use std::f32::consts::TAU;
#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

// Setup scene for solar system simulation
pub fn setup_space(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    // Insert solar system parameters as a resource
    let solar_params = SolarSystemParameters::for_visualization();
    commands.insert_resource(solar_params.clone());

    // Set up dark space environment with maximum ambient light for planet visibility
    commands.insert_resource(ClearColor(Color::srgb(0.005, 0.005, 0.01))); // Extremely dark space
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.2, 0.25, 0.3), // Bright ambient light with slight blue tint
        brightness: 0.15,                   // Maximum ambient brightness for planet visibility
    });

    // Camera positioned to view the full set of orbits on load
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 120000.0, 1500000.0)
                .looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
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
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 500000000.0, // Enormous intensity for massive astronomical-scale illumination
            shadows_enabled: false, // Disable shadows for better performance
            color: Color::srgb(1.0, 1.0, 0.98), // Pure sunlight
            range: 4000000.0, // Extended range to reach outer planets with correct sun-facing light
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });

    // Create optimized starfield background (reduced density for performance)
    create_starfield(&mut commands, &mut meshes, &mut materials, &solar_params);

    let shared_orbit_material = materials.add(create_orbit_material(
        Color::srgb(0.8, 0.9, 1.0), // Subtle blue-white tint
        orbit_emissive(Color::srgb(0.8, 0.9, 1.0), 0.2), // Much subtler glow
        0.15, // Much more transparent - very subtle lines
    ));
    commands.insert_resource(SharedOrbitMaterial {
        handle: shared_orbit_material.clone(),
    });

    // Create all planets and moons
    let planets = vec![
        Planet::create_sun(),
        Planet::create_mercury(),
        Planet::create_venus(),
        Planet::create_earth(),
        Planet::create_mars(),
        Planet::create_jupiter(),
        Planet::create_saturn(),
        Planet::create_uranus(),
        Planet::create_neptune(),
    ];

    // Create all moons
    let moons = vec![
        // Earth's moon
        Planet::create_moon(),
        // Mars' moons
        Planet::create_phobos(),
        Planet::create_deimos(),
        // Jupiter's major moons
        Planet::create_io(),
        Planet::create_europa(),
        Planet::create_ganymede(),
        Planet::create_callisto(),
        // Saturn's major moons
        Planet::create_mimas(),
        Planet::create_enceladus(),
        Planet::create_tethys(),
        Planet::create_dione(),
        Planet::create_rhea(),
        Planet::create_titan(),
        Planet::create_hyperion(),
        Planet::create_iapetus(),
        // Uranus' major moons
        Planet::create_miranda(),
        Planet::create_ariel(),
        Planet::create_umbriel(),
        Planet::create_titania(),
        Planet::create_oberon(),
        // Neptune's major moons
        Planet::create_triton(),
        Planet::create_proteus(),
        Planet::create_nereid(),
        Planet::create_larissa(),
    ];

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
            shared_orbit_material,
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
            &shared_orbit_material,
        );
    }
}

#[cfg(target_arch = "wasm32")]
#[derive(Resource)]
pub(crate) struct SpawnQueue {
    pending: VecDeque<Planet>,
    entity_map: HashMap<String, Entity>,
    position_map: HashMap<String, Vec3>,
    axial_tilts: HashMap<String, f32>,
    shared_orbit_material: Handle<StandardMaterial>,
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
        shared_orbit_material,
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
            shared_orbit_material,
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
    shared_orbit_material: &Handle<StandardMaterial>,
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
    let emissive_texture = if planet.name == "Sun" {
        albedo_handle.clone()
    } else if has_albedo {
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
    let material = create_planet_material(
        None,
        None,
        None,
        base_color,
        emissive,
        planet.name == "Sun",
        metallic,
        reflectance,
        perceptual_roughness,
    );
    #[cfg(not(target_arch = "wasm32"))]
    let material = create_planet_material(
        albedo_handle.clone(),
        None,
        emissive_texture.clone(),
        base_color,
        emissive,
        planet.name == "Sun",
        metallic,
        reflectance,
        perceptual_roughness,
    );

    let material_handle = materials.add(material);

    let planet_entity = commands
        .spawn(PbrBundle {
            mesh: create_uv_sphere_mesh(meshes, visual_radius),
            material: material_handle.clone(),
            transform: Transform::from_translation(initial_position),
            ..default()
        })
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
        commands.entity(planet_entity).insert(PendingMaterialTextures {
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
            let orbit_base_color = planet.color;
            #[cfg(not(target_arch = "wasm32"))]
            let orbit_mesh = create_orbit_mesh_ellipse(meshes, &orbit_shape, orbit_base_color);
            #[cfg(target_arch = "wasm32")]
            let orbit_mesh = create_placeholder_orbit_mesh(meshes);
            let orbit_material_handle = shared_orbit_material.clone();
            let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, true);

            // Spawn moon orbit as a separate entity (not a child) to avoid inheriting parent spin.
            #[cfg(target_arch = "wasm32")]
            let orbit_entity = commands
                .spawn(PbrBundle {
                    mesh: orbit_mesh.clone(),
                    material: orbit_material_handle.clone(),
                    transform: Transform::default(),
                    ..default()
                })
                .insert(OrbitComponent {
                    radius: orbit_shape.semi_major_axis_units,
                    planet_entity: parent_ent,
                    material: orbit_material_handle.clone(),
                    base_color: orbit_base_color,
                    tilt: orbit_motion.tilt,
                    wobble_speed: orbit_motion.wobble_speed,
                    wobble_amount: orbit_motion.wobble_amount,
                    spin_speed: orbit_motion.spin_speed,
                    phase: orbit_motion.phase,
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
                    .spawn(PbrBundle {
                        mesh: orbit_mesh.clone(),
                        material: orbit_material_handle.clone(),
                        transform: Transform::default(),
                        ..default()
                    })
                    .insert(OrbitComponent {
                        radius: orbit_shape.semi_major_axis_units,
                        planet_entity: parent_ent,
                        material: orbit_material_handle.clone(),
                        base_color: orbit_base_color,
                        tilt: orbit_motion.tilt,
                        wobble_speed: orbit_motion.wobble_speed,
                        wobble_amount: orbit_motion.wobble_amount,
                        spin_speed: orbit_motion.spin_speed,
                        phase: orbit_motion.phase,
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
                    color: orbit_base_color,
                    segments: 128,
                });
            }
        }
    } else if planet.name != "Sun" {
        let orbit_shape = physics::orbit_shape_for(&planet, solar_params);
        let orbit_base_color = planet.color;
        #[cfg(not(target_arch = "wasm32"))]
        let orbit_mesh = create_orbit_mesh_ellipse(meshes, &orbit_shape, orbit_base_color);
        #[cfg(target_arch = "wasm32")]
        let orbit_mesh = create_placeholder_orbit_mesh(meshes);
        let orbit_material_handle = shared_orbit_material.clone();
        let orbit_motion = orbit_motion_params(&planet.name, planet.orbital_distance_au, false);

        #[cfg(target_arch = "wasm32")]
        let orbit_entity = commands
            .spawn(PbrBundle {
                mesh: orbit_mesh.clone(),
                material: orbit_material_handle.clone(),
                transform: Transform::default(),
                ..default()
            })
            .insert(OrbitComponent {
                radius: orbit_shape.semi_major_axis_units,
                planet_entity,
                material: orbit_material_handle.clone(),
                base_color: orbit_base_color,
                tilt: orbit_motion.tilt,
                wobble_speed: orbit_motion.wobble_speed,
                wobble_amount: orbit_motion.wobble_amount,
                spin_speed: orbit_motion.spin_speed,
                phase: orbit_motion.phase,
            })
            .insert(Name::new(format!("Orbit {}", planet.name)))
            .id();

        #[cfg(not(target_arch = "wasm32"))]
        {
            commands
                .spawn(PbrBundle {
                    mesh: orbit_mesh.clone(),
                    material: orbit_material_handle.clone(),
                    transform: Transform::default(),
                    ..default()
                })
                .insert(OrbitComponent {
                    radius: orbit_shape.semi_major_axis_units,
                    planet_entity,
                    material: orbit_material_handle.clone(),
                    base_color: orbit_base_color,
                    tilt: orbit_motion.tilt,
                    wobble_speed: orbit_motion.wobble_speed,
                    wobble_amount: orbit_motion.wobble_amount,
                    spin_speed: orbit_motion.spin_speed,
                    phase: orbit_motion.phase,
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
                    PbrBundle {
                        mesh: create_ring_mesh(meshes, ring_inner_radius, ring_outer_radius),
                        material: ring_material_handle.clone(),
                        transform: Transform::default(),
                        ..default()
                    },
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
                    PbrBundle {
                        mesh: create_ring_mesh(meshes, ring_inner_radius, ring_outer_radius),
                        material: ring_material_handle.clone(),
                        transform: Transform::default(),
                        ..default()
                    },
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
        let cloud_material =
            create_cloud_material(cloud_texture.clone(), clouds.alpha);
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
                    PbrBundle {
                        mesh: create_uv_sphere_mesh(meshes, visual_radius * clouds.scale),
                        material: cloud_material_handle.clone(),
                        ..default()
                    },
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
                    PbrBundle {
                        mesh: create_uv_sphere_mesh(meshes, visual_radius * clouds.scale),
                        material: cloud_material_handle.clone(),
                        ..default()
                    },
                    CloudLayer {
                        rotation_period_hours: clouds.rotation_period_hours,
                    },
                ));
            }
        });
    }
}

struct OrbitMotionParams {
    tilt: Vec2,
    wobble_speed: f32,
    wobble_amount: f32,
    spin_speed: f32,
    phase: f32,
}

fn orbit_motion_params(name: &str, orbital_distance_au: f32, is_moon: bool) -> OrbitMotionParams {
    let base = orbit_hash(name, 1);
    let offset = orbit_hash(name, 7);
    let max_tilt = if is_moon { 0.28 } else { 0.16 };
    let tilt = Vec2::new(
        (base * 2.0 - 1.0) * max_tilt,
        (offset * 2.0 - 1.0) * max_tilt,
    );
    let wobble_amount = if is_moon { 0.06 } else { 0.035 };
    let wobble_speed = 0.05 + base * 0.12 + orbital_distance_au * 0.002;
    let spin_speed = 0.02 + offset * 0.05;
    let phase = base * TAU;

    OrbitMotionParams {
        tilt,
        wobble_speed,
        wobble_amount,
        spin_speed,
        phase,
    }
}

fn orbit_hash(name: &str, seed: u32) -> f32 {
    let mut hash = 2166136261u32 ^ seed;
    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16777619);
    }
    (hash % 10_000) as f32 / 10_000.0
}

// Create minimal starfield for performance (disabled for optimal performance)
fn create_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    solar_params: &SolarSystemParameters,
) {
    // Single GPU draw: build a mesh of tiny quads with per-star color/size
    let mut rng = StdRng::seed_from_u64(1337);
    let star_count = 100000;
    let radius = solar_params.au_to_units(400.0); // push far beyond planetary orbits

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(star_count * 4);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(star_count * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(star_count * 4);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(star_count * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(star_count * 6);

    let quad = [
        (Vec2::new(-1.0, -1.0), Vec2::new(0.0, 0.0)),
        (Vec2::new(1.0, -1.0), Vec2::new(1.0, 0.0)),
        (Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0)),
        (Vec2::new(-1.0, 1.0), Vec2::new(0.0, 1.0)),
    ];

    for i in 0..star_count {
        // Uniform point on sphere - balanced in all directions
        // Remove galactic plane bias for even star distribution
        let z = rng.gen_range(-1.0..1.0);
        let theta = rng.gen_range(0.0..std::f32::consts::TAU);
        let r = (1.0_f32 - z * z).sqrt();
        let dir = Vec3::new(r * theta.cos(), z, r * theta.sin());
        let center = dir * radius;

        let size = rng.gen_range(900.0..2600.0);
        let color_tint = rng.gen_range(0.7..1.2);
        let color = [
            1.2 * color_tint,
            1.05 * color_tint,
            0.95 * color_tint,
            1.0,
        ];

        // Build a camera-independent quad facing outward along dir
        let up_hint = if dir.dot(Vec3::Y).abs() > 0.9 {
            Vec3::X
        } else {
            Vec3::Y
        };
        let right = dir.cross(up_hint).normalize_or_zero();
        let up_vec = right.cross(dir).normalize_or_zero();

        let base_index = (i * 4) as u32;
        for (offset, uv) in quad {
            let world_pos = center + right * offset.x * size + up_vec * offset.y * size;
            positions.push(world_pos.into());
            normals.push(dir.into());
            uvs.push(uv.into());
            colors.push(color);
        }

        indices.extend_from_slice(&[
            base_index,
            base_index + 1,
            base_index + 2,
            base_index,
            base_index + 2,
            base_index + 3,
        ]);
    }

    let mut mesh = Mesh::new(
        bevy::render::mesh::PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(bevy::render::mesh::Indices::U32(indices));

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 1.0),
        unlit: true,
        alpha_mode: AlphaMode::Add,
        emissive: LinearRgba::new(1.0, 0.98, 0.9, 1.0),
        ..default()
    });

    commands.spawn((
        PbrBundle {
            mesh: meshes.add(mesh),
            material,
            transform: Transform::IDENTITY,
            ..default()
        },
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

struct PlanetTextureSet {
    albedo: Option<&'static str>,
    emissive: Option<&'static str>,
}

struct CloudLayerConfig {
    texture_path: &'static str,
    alpha: f32,
    rotation_period_hours: f32,
    scale: f32,
}

#[derive(Resource)]
#[allow(dead_code)]
pub(crate) struct SharedOrbitMaterial {
    pub(crate) handle: Handle<StandardMaterial>,
}

fn get_planet_textures(planet_name: &str) -> PlanetTextureSet {
    match planet_name {
        "Sun" => PlanetTextureSet {
            albedo: Some("textures/planets/sun/albedo.png"),
            emissive: None,
        },
        "Earth" => PlanetTextureSet {
            albedo: Some("textures/planets/earth/albedo.png"),
            emissive: Some("textures/planets/earth/emissive.png"),
        },
        "Mercury" => PlanetTextureSet {
            albedo: Some("textures/planets/mercury/albedo.png"),
            emissive: None,
        },
        "Venus" => PlanetTextureSet {
            albedo: Some("textures/planets/venus/albedo.png"),
            emissive: None,
        },
        "Mars" => PlanetTextureSet {
            albedo: Some("textures/planets/mars/albedo.png"),
            emissive: None,
        },
        "Jupiter" => PlanetTextureSet {
            albedo: Some("textures/planets/jupiter/albedo.png"),
            emissive: None,
        },
        "Saturn" => PlanetTextureSet {
            albedo: Some("textures/planets/saturn/albedo.png"),
            emissive: None,
        },
        "Uranus" => PlanetTextureSet {
            albedo: Some("textures/planets/uranus/albedo.png"),
            emissive: None,
        },
        "Neptune" => PlanetTextureSet {
            albedo: Some("textures/planets/neptune/albedo.png"),
            emissive: None,
        },
        "Moon" => PlanetTextureSet {
            albedo: Some("textures/planets/moon/albedo.png"),
            emissive: None,
        },
        "Phobos" => PlanetTextureSet {
            albedo: Some("textures/planets/phobos/albedo.png"),
            emissive: None,
        },
        "Deimos" => PlanetTextureSet {
            albedo: Some("textures/planets/deimos/albedo.png"),
            emissive: None,
        },
        "Io" => PlanetTextureSet {
            albedo: Some("textures/planets/io/albedo.png"),
            emissive: None,
        },
        "Europa" => PlanetTextureSet {
            albedo: Some("textures/planets/europa/albedo.png"),
            emissive: None,
        },
        "Ganymede" => PlanetTextureSet {
            albedo: Some("textures/planets/ganymede/albedo.png"),
            emissive: None,
        },
        "Callisto" => PlanetTextureSet {
            albedo: Some("textures/planets/callisto/albedo.png"),
            emissive: None,
        },
        "Mimas" => PlanetTextureSet {
            albedo: Some("textures/planets/mimas/albedo.png"),
            emissive: None,
        },
        "Enceladus" => PlanetTextureSet {
            albedo: Some("textures/planets/enceladus/albedo.png"),
            emissive: None,
        },
        "Tethys" => PlanetTextureSet {
            albedo: Some("textures/planets/tethys/albedo.png"),
            emissive: None,
        },
        "Dione" => PlanetTextureSet {
            albedo: Some("textures/planets/dione/albedo.png"),
            emissive: None,
        },
        "Rhea" => PlanetTextureSet {
            albedo: Some("textures/planets/rhea/albedo.png"),
            emissive: None,
        },
        "Titan" => PlanetTextureSet {
            albedo: Some("textures/planets/titan/albedo.png"),
            emissive: None,
        },
        "Hyperion" => PlanetTextureSet {
            albedo: Some("textures/planets/hyperion/albedo.png"),
            emissive: None,
        },
        "Iapetus" => PlanetTextureSet {
            albedo: Some("textures/planets/iapetus/albedo.png"),
            emissive: None,
        },
        "Miranda" => PlanetTextureSet {
            albedo: Some("textures/planets/miranda/albedo.png"),
            emissive: None,
        },
        "Ariel" => PlanetTextureSet {
            albedo: Some("textures/planets/ariel/albedo.png"),
            emissive: None,
        },
        "Umbriel" => PlanetTextureSet {
            albedo: Some("textures/planets/umbriel/albedo.png"),
            emissive: None,
        },
        "Titania" => PlanetTextureSet {
            albedo: Some("textures/planets/titania/albedo.png"),
            emissive: None,
        },
        "Oberon" => PlanetTextureSet {
            albedo: Some("textures/planets/oberon/albedo.png"),
            emissive: None,
        },
        "Triton" => PlanetTextureSet {
            albedo: Some("textures/planets/triton/albedo.png"),
            emissive: None,
        },
        "Proteus" => PlanetTextureSet {
            albedo: Some("textures/planets/proteus/albedo.png"),
            emissive: None,
        },
        "Nereid" => PlanetTextureSet {
            albedo: Some("textures/planets/nereid/albedo.png"),
            emissive: None,
        },
        "Larissa" => PlanetTextureSet {
            albedo: Some("textures/planets/larissa/albedo.png"),
            emissive: None,
        },
        _ => PlanetTextureSet {
            albedo: None,
            emissive: None,
        },
    }
}

fn get_cloud_layer_config(planet_name: &str) -> Option<CloudLayerConfig> {
    match planet_name {
        "Earth" => Some(CloudLayerConfig {
            texture_path: "textures/planets/earth/clouds.png",
            alpha: 0.65,
            rotation_period_hours: 24.0,
            scale: 1.012,
        }),
        "Venus" => Some(CloudLayerConfig {
            texture_path: "textures/planets/venus/clouds.png",
            alpha: 0.4,
            rotation_period_hours: 96.0,
            scale: 1.02,
        }),
        "Titan" => Some(CloudLayerConfig {
            texture_path: "textures/planets/titan/clouds.png",
            alpha: 0.45,
            rotation_period_hours: 382.0,
            scale: 1.02,
        }),
        _ => None,
    }
}

fn get_ring_texture_path(planet_name: &str) -> Option<&'static str> {
    match planet_name {
        "Saturn" => Some("textures/planets/saturn/rings.png"),
        _ => None,
    }
}

fn asset_exists(path: &str) -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        true
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets")
            .join(path)
            .exists()
    }
}

fn load_texture(asset_server: &AssetServer, path: Option<&'static str>) -> Option<Handle<Image>> {
    let path = path?;
    if asset_exists(path) {
        Some(asset_server.load(path))
    } else {
        None
    }
}
