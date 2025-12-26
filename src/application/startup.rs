use bevy::prelude::*;
use rand;
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

    // Set up dark space environment with enhanced ambient light for planet visibility
    commands.insert_resource(ClearColor(Color::srgb(0.0, 0.0, 0.05))); // Very dark blue-black space
    commands.insert_resource(AmbientLight {
        color: Color::srgb(0.05, 0.05, 0.08), // Very dim blue ambient light
        brightness: 0.02, // Slightly increased for better planet visibility
    });

    // Camera positioned to view the solar system
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(0.0, 50.0, 200.0).looking_at(Vec3::ZERO, Vec3::Y),
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

    // Sun as the main light source with enhanced intensity for better planet visibility
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 500000.0, // Significantly increased for better planet illumination
            shadows_enabled: false, // Disable shadows for better performance
            color: Color::srgb(1.0, 1.0, 0.95), // Slightly warmer light
            range: 2000.0, // Increased range to illuminate more distant planets
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, 0.0),
        ..default()
    });

    // Add a subtle fill light to make planets more visible from all angles
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 20000.0, // Much dimmer fill light
            shadows_enabled: false,
            color: Color::srgb(0.8, 0.9, 1.0), // Cool blue fill light
            range: 1500.0,
            ..default()
        },
        transform: Transform::from_xyz(100.0, 50.0, 100.0), // Offset position for fill lighting
        ..default()
    });

    // Create starfield background
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
        // Neptune's major moon
        Planet::create_triton(),
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

        // For initial positions, moons start at their parent planet's position
        // The update system will handle proper orbital positioning
        let initial_position = if planet.parent_entity.is_some() {
            // Moon - start near parent position
            // The physics system will update them properly
            Vec3::ZERO
        } else {
            // Planet - calculate position around Sun
            physics::calculate_planet_position(&planet, 0.0, &solar_params, Vec3::ZERO)
        };

        commands.spawn((
            PbrBundle {
                mesh: meshes.add(Mesh::from(Sphere { radius: visual_radius })),
                material: materials.add(StandardMaterial {
                    base_color: planet.color,
                    emissive: if planet.name == "Sun" {
                        LinearRgba::new(1.0, 1.0, 0.8, 1.0) // Make sun glow
                    } else {
                        LinearRgba::BLACK
                    },
                    // Add reflective properties for better visibility
                    metallic: if planet.name == "Sun" { 0.0 } else { 0.1 }, // Slight metallic for planetary surfaces
                    reflectance: if planet.name == "Sun" { 0.0 } else { 0.3 }, // Higher reflectance for better light reflection
                    perceptual_roughness: 0.7, // Moderate roughness for realistic surfaces
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
    }
}

// Create a starfield background for realistic space environment
fn create_starfield(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    // Create distant stars as emissive points
    let star_count = 2000;
    let star_distance = 5000.0; // Stars are very far away

    for _ in 0..star_count {
        // Generate random position on a sphere
        let theta = rand::random::<f32>() * 2.0 * std::f32::consts::PI;
        let phi = (rand::random::<f32>() - 0.5) * std::f32::consts::PI; // Avoid poles for better distribution
        let x = star_distance * phi.sin() * theta.cos();
        let y = star_distance * phi.sin() * theta.sin();
        let z = star_distance * phi.cos();

        // Random star size (very small)
        let star_size = rand::random::<f32>() * 0.5 + 0.1;

        // Random star brightness/color
        let star_brightness = rand::random::<f32>() * 0.8 + 0.2;
        let star_color = if rand::random::<f32>() < 0.1 {
            // 10% blue giants
            Color::srgb(0.7 * star_brightness, 0.8 * star_brightness, 1.0 * star_brightness)
        } else if rand::random::<f32>() < 0.2 {
            // 10% red giants
            Color::srgb(1.0 * star_brightness, 0.6 * star_brightness, 0.4 * star_brightness)
        } else {
            // 80% white/yellow stars
            Color::srgb(0.9 * star_brightness, 0.9 * star_brightness, 1.0 * star_brightness)
        };

        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(Sphere { radius: star_size })),
            material: materials.add(StandardMaterial {
                base_color: star_color,
                emissive: LinearRgba::new(star_brightness * 2.0, star_brightness * 2.0, star_brightness * 2.0, 1.0), // Stars glow
                ..default()
            }),
            transform: Transform::from_translation(Vec3::new(x, y, z)),
            ..default()
        });
    }

    // Create a few brighter, more prominent stars
    for _ in 0..50 {
        let theta = rand::random::<f32>() * 2.0 * std::f32::consts::PI;
        let phi = (rand::random::<f32>() - 0.5) * std::f32::consts::PI;
        let x = star_distance * phi.sin() * theta.cos();
        let y = star_distance * phi.sin() * theta.sin();
        let z = star_distance * phi.cos();

        let star_size = rand::random::<f32>() * 1.0 + 0.5;
        let star_brightness = rand::random::<f32>() * 0.6 + 0.4;

        commands.spawn(PbrBundle {
            mesh: meshes.add(Mesh::from(Sphere { radius: star_size })),
            material: materials.add(StandardMaterial {
                base_color: Color::srgb(1.0 * star_brightness, 1.0 * star_brightness, 0.9 * star_brightness),
                emissive: LinearRgba::new(3.0 * star_brightness, 3.0 * star_brightness, 2.7 * star_brightness, 1.0),
                ..default()
            }),
            transform: Transform::from_translation(Vec3::new(x, y, z)),
            ..default()
        });
    }
}