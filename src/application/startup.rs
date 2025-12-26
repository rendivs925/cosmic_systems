use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::services::physics;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::{
    CameraController, CameraMode, Selectable, *,
};
use bevy::prelude::*;
use bevy::render::mesh::primitives::{Meshable, SphereKind};
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::PrimitiveTopology;
use std::collections::HashMap;
use std::f32::consts::TAU;
use std::path::Path;

// Setup scene for gyro propulsion simulation
pub fn setup_gyro(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // Camera
    commands.spawn(Camera3dBundle {
        transform: Transform::from_xyz(0.0, 2.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Light
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 4.0),
        ..default()
    });

    // Spawn 3 gyros in an array
    for i in -1..=1 {
        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Mesh::from(Cylinder {
                    radius: 0.5,
                    half_height: 0.05,
                })),
                material: materials.add(StandardMaterial {
                    base_color: Color::srgb(0.3, 0.5, 0.3),
                    ..default()
                }),
                transform: Transform::from_xyz(i as f32 * 1.5, 0.0, 0.0),
                ..default()
            },
            GyroscopeComponent {
                domain_gyro: Gyroscope::new(),
            },
        ));
    }

    // Thrust arrow
    commands.spawn((
        PbrBundle {
            mesh: meshes.add(Mesh::from(Capsule3d {
                radius: 0.05,
                half_length: 0.5,
            })),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0, 0.0, 0.0),
                ..default()
            }),
            transform: Transform::from_xyz(0.0, 1.0, 0.0).with_scale(Vec3::new(0.1, 0.1, 1.0)),
            ..default()
        },
        ThrustArrow,
    ));

    // Floor for reference
    commands.spawn(PbrBundle {
        mesh: meshes.add(Mesh::from(Plane3d {
            normal: Dir3::Y,
            half_size: Vec2::splat(5.0),
        })),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.1, 0.1, 0.1),
            ..default()
        }),
        transform: Transform::from_xyz(0.0, -1.0, 0.0),
        ..default()
    });
}

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

    // Camera positioned to view the massively scaled astronomical solar system
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 5000.0, 20000.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        CameraController {
            mode: CameraMode::FreeFlight,
            speed: 50.0,
            sensitivity: 0.002,
            velocity: Vec3::ZERO,
            target_entity: None,
            orbit_distance: 300.0,
            orbit_angle: 0.0,
        },
    ));

    // Sun as the main light source with enormous intensity for massive astronomical distances
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 500000000.0, // Enormous intensity for massive astronomical-scale illumination
            shadows_enabled: false, // Disable shadows for better performance
            color: Color::srgb(1.0, 1.0, 0.98), // Pure sunlight
            range: 2000000.0, // Extremely extended range for all planets across massive distances
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });

    // Add directional light from the Sun's direction for better front illumination
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 200000.0, // Extremely bright directional light for massive distances
            color: Color::srgb(1.0, 1.0, 0.95), // Sunlight color
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(-100000.0, 50000.0, -100000.0)
            .looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Add enormously powerful fill lights covering the massive astronomical distances
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 200000000.0, // Enormously bright fill light for massive distances
            shadows_enabled: false,
            color: Color::srgb(0.9, 0.95, 1.0), // Bright white-blue fill light
            range: 1600000.0,                   // Extended range for massive astronomical distances
            ..default()
        },
        transform: Transform::from_xyz(75000.0, 37500.0, 75000.0), // Offset position for side illumination
        ..default()
    });

    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 200000000.0, // Second fill light from opposite direction
            shadows_enabled: false,
            color: Color::srgb(1.0, 0.9, 0.95), // Warm white fill light
            range: 1600000.0,
            ..default()
        },
        transform: Transform::from_xyz(-75000.0, -37500.0, -75000.0), // Opposite position
        ..default()
    });

    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 150000000.0, // Top illumination
            shadows_enabled: false,
            color: Color::srgb(0.95, 0.95, 1.0), // Cool white
            range: 1600000.0,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 100000.0, 0.0), // Above the solar system
        ..default()
    });

    // Add directional light from the Sun's direction for better front illumination
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 50000.0, // Very bright directional light for vast distances
            color: Color::srgb(1.0, 1.0, 0.95), // Sunlight color
            shadows_enabled: false,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 0.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Add massively powerful fill lights covering the vast astronomical distances
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 50000000.0, // Massively bright fill light for vast distances
            shadows_enabled: false,
            color: Color::srgb(0.9, 0.95, 1.0), // Bright white-blue fill light
            range: 500000.0, // Extremely extended range for vast astronomical distances
            ..default()
        },
        transform: Transform::from_xyz(25000.0, 12500.0, 25000.0), // Offset position for side illumination
        ..default()
    });

    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 50000000.0, // Second fill light from opposite direction
            shadows_enabled: false,
            color: Color::srgb(1.0, 0.9, 0.95), // Warm white fill light
            range: 500000.0,
            ..default()
        },
        transform: Transform::from_xyz(-25000.0, -12500.0, -25000.0), // Opposite position
        ..default()
    });

    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 40000000.0, // Top illumination
            shadows_enabled: false,
            color: Color::srgb(0.95, 0.95, 1.0), // Cool white
            range: 500000.0,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 37500.0, 0.0), // Above the solar system
        ..default()
    });

    // Create optimized starfield background (reduced density for performance)
    create_starfield(&mut commands, &mut meshes, &mut materials);

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

    // Spawn each celestial body (planets and moons)
    for planet in all_celestial_bodies {
        let visual_radius = if planet.name == "Sun" {
            physics::calculate_sun_visual_radius(&solar_params)
        } else {
            physics::calculate_visual_radius(&planet, &solar_params)
        };

        let initial_position =
            physics::calculate_planet_position(&planet, 0.0, &solar_params, Vec3::ZERO);
        let textures = get_planet_textures(&planet.name);
        let albedo_handle = load_texture(&asset_server, textures.albedo);
        let emissive_handle = load_texture(&asset_server, textures.emissive);
        let has_albedo = albedo_handle.is_some();

        let base_color = if planet.name == "Sun" {
            planet.color
        } else if has_albedo {
            Color::WHITE
        } else {
            planet.color
        };

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

        let material = StandardMaterial {
            base_color_texture: albedo_handle.clone(),
            normal_map_texture: None,
            emissive_texture: emissive_handle.clone(),
            base_color,
            emissive: if planet.name == "Sun" {
                LinearRgba::new(1.0, 1.0, 0.8, 1.0)
            } else {
                LinearRgba::BLACK
            },
            unlit: planet.name == "Sun",
            metallic,
            reflectance,
            perceptual_roughness,
            ..default()
        };

        let material_handle = materials.add(material);

        let planet_entity = commands
            .spawn(PbrBundle {
                mesh: create_uv_sphere_mesh(&mut meshes, visual_radius),
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

        entity_map.insert(planet.name.clone(), planet_entity);

        if solar_params.show_orbits {
            if let Some(parent_name) = &planet.parent_entity {
                if let Some(parent_entity) = entity_map.get(parent_name) {
                    let orbit_radius =
                        planet.orbital_distance_au * solar_params.scale_factor * 500.0;
                    let orbit_mesh = create_orbit_mesh(&mut meshes, orbit_radius);
                    let orbit_base_color = planet.color;
                    let orbit_material = materials.add(StandardMaterial {
                        base_color: orbit_base_color.with_alpha(0.22),
                        emissive: orbit_emissive(orbit_base_color, 0.45),
                        unlit: true,
                        alpha_mode: AlphaMode::Blend,
                        double_sided: true,
                        ..default()
                    });
                    let orbit_material_handle = orbit_material.clone();
                    let orbit_motion =
                        orbit_motion_params(&planet.name, planet.orbital_distance_au, true);

                    commands.entity(*parent_entity).with_children(|parent| {
                        parent
                            .spawn(PbrBundle {
                                mesh: orbit_mesh,
                                material: orbit_material_handle,
                                ..default()
                            })
                            .insert(OrbitComponent {
                                radius: orbit_radius,
                                planet_entity,
                                material: orbit_material,
                                base_color: orbit_base_color,
                                tilt: orbit_motion.tilt,
                                wobble_speed: orbit_motion.wobble_speed,
                                wobble_amount: orbit_motion.wobble_amount,
                                spin_speed: orbit_motion.spin_speed,
                                phase: orbit_motion.phase,
                            })
                            .insert(Name::new(format!(
                                "Orbit {} around {}",
                                planet.name, parent_name
                            )));
                    });
                }
            } else if planet.name != "Sun" {
                let orbit_radius = solar_params.au_to_units(planet.orbital_distance_au);
                let orbit_mesh = create_orbit_mesh(&mut meshes, orbit_radius);
                let orbit_base_color = planet.color;
                let orbit_material = materials.add(StandardMaterial {
                    base_color: orbit_base_color.with_alpha(0.25),
                    emissive: orbit_emissive(orbit_base_color, 0.5),
                    unlit: true,
                    alpha_mode: AlphaMode::Blend,
                    double_sided: true,
                    ..default()
                });
                let orbit_material_handle = orbit_material.clone();
                let orbit_motion =
                    orbit_motion_params(&planet.name, planet.orbital_distance_au, false);

                commands
                    .spawn(PbrBundle {
                        mesh: orbit_mesh,
                        material: orbit_material_handle,
                        ..default()
                    })
                    .insert(OrbitComponent {
                        radius: orbit_radius,
                        planet_entity,
                        material: orbit_material,
                        base_color: orbit_base_color,
                        tilt: orbit_motion.tilt,
                        wobble_speed: orbit_motion.wobble_speed,
                        wobble_amount: orbit_motion.wobble_amount,
                        spin_speed: orbit_motion.spin_speed,
                        phase: orbit_motion.phase,
                    })
                    .insert(Name::new(format!("Orbit {}", planet.name)));
            }
        }

        if let Some(atmosphere) = get_atmosphere_config(&planet.name) {
            commands.entity(planet_entity).with_children(|parent| {
                let emissive_color: LinearRgba = atmosphere.color.into();
                parent.spawn(PbrBundle {
                    mesh: create_uv_sphere_mesh(&mut meshes, visual_radius * atmosphere.scale),
                    material: materials.add(StandardMaterial {
                        base_color: atmosphere.color.with_alpha(atmosphere.alpha),
                        emissive: LinearRgba::new(
                            emissive_color.red * atmosphere.emissive_strength,
                            emissive_color.green * atmosphere.emissive_strength,
                            emissive_color.blue * atmosphere.emissive_strength,
                            1.0,
                        ),
                        alpha_mode: AlphaMode::Add,
                        unlit: true,
                        double_sided: true,
                        ..default()
                    }),
                    ..default()
                });
            });
        }

        if let Some(clouds) = get_cloud_layer_config(&planet.name) {
            if let Some(cloud_texture) = load_texture(&asset_server, Some(clouds.texture_path)) {
                commands.entity(planet_entity).with_children(|parent| {
                    parent.spawn((
                        PbrBundle {
                            mesh: create_uv_sphere_mesh(&mut meshes, visual_radius * clouds.scale),
                            material: materials.add(StandardMaterial {
                                base_color_texture: Some(cloud_texture),
                                base_color: Color::srgba(1.0, 1.0, 1.0, clouds.alpha),
                                alpha_mode: AlphaMode::Blend,
                                double_sided: true,
                                perceptual_roughness: 0.9,
                                ..default()
                            }),
                            ..default()
                        },
                        CloudLayer {
                            rotation_period_hours: clouds.rotation_period_hours,
                        },
                    ));
                });
            }
        }

        if planet.name == "Saturn" {
            let ring_outer_radius = visual_radius * 2.5;
            let ring_thickness = visual_radius * 0.1;
            let ring_texture = load_texture(&asset_server, get_ring_texture_path(&planet.name));

            commands.spawn((
                PbrBundle {
                    mesh: meshes.add(Mesh::from(Cylinder {
                        radius: ring_outer_radius,
                        half_height: ring_thickness,
                    })),
                    material: materials.add(StandardMaterial {
                        base_color_texture: ring_texture,
                        base_color: Color::srgb(0.9, 0.85, 0.75),
                        metallic: 0.0,
                        reflectance: 0.9,
                        perceptual_roughness: 0.15,
                        alpha_mode: AlphaMode::Blend,
                        double_sided: true,
                        ..default()
                    }),
                    transform: Transform::from_translation(initial_position),
                    ..default()
                },
                Selectable {
                    name: "Saturn Rings".to_string(),
                    selected: false,
                },
            ));
        }
    }
}
fn create_uv_sphere_mesh(meshes: &mut ResMut<Assets<Mesh>>, radius: f32) -> Handle<Mesh> {
    let mesh = Sphere { radius }
        .mesh()
        .kind(SphereKind::Uv {
            sectors: 64,
            stacks: 32,
        })
        .build();
    meshes.add(mesh)
}

fn create_orbit_mesh(meshes: &mut ResMut<Assets<Mesh>>, radius: f32) -> Handle<Mesh> {
    const SEGMENTS: usize = 256;
    let mut positions = Vec::with_capacity(SEGMENTS);
    let mut normals = Vec::with_capacity(SEGMENTS);
    let mut uvs = Vec::with_capacity(SEGMENTS);
    let mut indices = Vec::with_capacity(SEGMENTS * 2);

    for i in 0..SEGMENTS {
        let angle = (i as f32 / SEGMENTS as f32) * TAU;
        positions.push([radius * angle.cos(), 0.0, radius * angle.sin()]);
        normals.push([0.0, 1.0, 0.0]);
        uvs.push([angle / TAU, 0.5]);
        indices.push(i as u32);
        indices.push(((i + 1) % SEGMENTS) as u32);
    }

    let mut mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default());
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    meshes.add(mesh)
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
    let tilt = Vec2::new((base * 2.0 - 1.0) * max_tilt, (offset * 2.0 - 1.0) * max_tilt);
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

fn orbit_emissive(color: Color, intensity: f32) -> LinearRgba {
    let [r, g, b, _] = color.as_rgba_f32();
    LinearRgba::new(r * intensity, g * intensity, b * intensity, 1.0)
}

// Create minimal starfield for performance (disabled for optimal performance)
fn create_starfield(
    _commands: &mut Commands,
    _meshes: &mut ResMut<Assets<Mesh>>,
    _materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    // Starfield disabled for performance optimization
    // Previously created 1500+ stars which caused performance issues
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

struct AtmosphereConfig {
    color: Color,
    alpha: f32,
    scale: f32,
    emissive_strength: f32,
}

fn get_planet_textures(planet_name: &str) -> PlanetTextureSet {
    match planet_name {
        "Sun" => PlanetTextureSet {
            albedo: None,
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

fn get_atmosphere_config(planet_name: &str) -> Option<AtmosphereConfig> {
    match planet_name {
        "Earth" => Some(AtmosphereConfig {
            color: Color::srgb(0.25, 0.45, 0.85),
            alpha: 0.15,
            scale: 1.02,
            emissive_strength: 0.02,
        }),
        "Venus" => Some(AtmosphereConfig {
            color: Color::srgb(0.95, 0.85, 0.65),
            alpha: 0.2,
            scale: 1.03,
            emissive_strength: 0.02,
        }),
        "Mars" => Some(AtmosphereConfig {
            color: Color::srgb(0.85, 0.35, 0.15),
            alpha: 0.08,
            scale: 1.01,
            emissive_strength: 0.01,
        }),
        "Titan" => Some(AtmosphereConfig {
            color: Color::srgb(0.8, 0.6, 0.35),
            alpha: 0.18,
            scale: 1.03,
            emissive_strength: 0.02,
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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join(path)
        .exists()
}

fn load_texture(asset_server: &AssetServer, path: Option<&'static str>) -> Option<Handle<Image>> {
    let path = path?;
    if asset_exists(path) {
        Some(asset_server.load(path))
    } else {
        None
    }
}
