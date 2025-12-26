use bevy::prelude::*;
use crate::domain::entities::gyroscope::Gyroscope;
use crate::domain::entities::planet::Planet;
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::domain::services::physics;
use crate::infrastructure::bevy_adapters::components::{*, CameraController, CameraMode, Selectable};

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
                mesh: meshes.add(Mesh::from(Cylinder { radius: 0.5, half_height: 0.05 })),
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
            mesh: meshes.add(Mesh::from(Capsule3d { radius: 0.05, half_length: 0.5 })),
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
        mesh: meshes.add(Mesh::from(Plane3d { normal: Dir3::Y, half_size: Vec2::splat(5.0) })),
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
) {
    // Insert solar system parameters as a resource
    let solar_params = SolarSystemParameters::for_visualization();
    commands.insert_resource(solar_params.clone());

    // Set up dark space environment with maximum ambient light for planet visibility
    commands.insert_resource(ClearColor(Color::srgb(0.005, 0.005, 0.01))); // Extremely dark space
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.2, 0.25, 0.3), // Bright ambient light with slight blue tint
        brightness: 0.15, // Maximum ambient brightness for planet visibility
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
        transform: Transform::from_xyz(-100000.0, 50000.0, -100000.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });

    // Add enormously powerful fill lights covering the massive astronomical distances
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 200000000.0, // Enormously bright fill light for massive distances
            shadows_enabled: false,
            color: Color::srgb(0.9, 0.95, 1.0), // Bright white-blue fill light
            range: 1600000.0, // Extended range for massive astronomical distances
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

    // Spawn each celestial body (planets and moons)
    for planet in all_celestial_bodies {
        let visual_radius = if planet.name == "Sun" {
            physics::calculate_sun_visual_radius(&solar_params)
        } else {
            physics::calculate_visual_radius(&planet, &solar_params)
        };

        // Calculate proper initial positions
        let initial_position = physics::calculate_planet_position(&planet, 0.0, &solar_params, Vec3::ZERO);

        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Mesh::from(Sphere { radius: visual_radius })),
                material: materials.add(StandardMaterial {
                    base_color: if planet.name == "Sun" {
                        planet.color
                    } else {
                        // Keep original planet colors - the lighting will make them visible
                        planet.color
                    },
                    emissive: if planet.name == "Sun" {
                        LinearRgba::new(1.0, 1.0, 0.8, 1.0) // Make sun glow
                    } else {
                        // Add significant self-illumination to planets for maximum visibility
                        // Convert color to LinearRgba to access components
                        let color_rgba: LinearRgba = planet.color.into();
                        LinearRgba::new(
                            color_rgba.red * 0.15, // Increased from 0.05
                            color_rgba.green * 0.15,
                            color_rgba.blue * 0.15,
                            1.0
                        )
                    },
                    // Enhanced reflective properties for maximum visibility
                    metallic: if planet.name == "Sun" { 0.0 } else { 0.0 }, // Reduce metallic for better color visibility
                    reflectance: if planet.name == "Sun" { 0.0 } else { 0.6 }, // Higher reflectance for better light reflection
                    perceptual_roughness: 0.3, // Lower roughness for more specular highlights
                    ..default()
                }),
                transform: Transform::from_translation(initial_position),
                ..default()
            },
            PlanetComponent {
                domain_planet: planet.clone(),
            },
            Selectable {
                name: planet.name.clone(),
                selected: false,
            },
        ));

        // Add Saturn's rings
        if planet.name == "Saturn" {
            let ring_outer_radius = visual_radius * 2.5;
            let ring_thickness = visual_radius * 0.1;

            commands.spawn((
                PbrBundle {
                    mesh: meshes.add(Mesh::from(Cylinder {
                        radius: ring_outer_radius,
                        half_height: ring_thickness,
                    })),
                    material: materials.add(StandardMaterial {
                        base_color: Color::srgb(0.8, 0.7, 0.4), // Golden ring color
                        metallic: 0.1,
                        perceptual_roughness: 0.8,
                        alpha_mode: AlphaMode::Blend, // Make rings semi-transparent
                        ..default()
                    }),
                    transform: Transform::from_translation(initial_position),
                    ..default()
                },
                PlanetComponent {
                    domain_planet: planet.clone(),
                },
                Selectable {
                    name: "Saturn Rings".to_string(),
                    selected: false,
                },
            ));
        }
    }
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