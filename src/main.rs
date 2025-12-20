use bevy::prelude::*;

pub mod domain;
pub mod application;
pub mod infrastructure;
pub mod presentation;

use presentation::bevy_systems::*;

#[derive(Resource, Clone, Debug)]
pub struct SimulationParameters {
    pub rpm: f32,
    pub precession_hz: f32,
    pub asymmetry: f32,
    pub thrust_scale: f32,
}

impl SimulationParameters {
    pub fn new() -> Self {
        Self {
            rpm: 30000.0,
            precession_hz: 100.0,
            asymmetry: 0.5,
            thrust_scale: 0.001,
        }
    }
}


fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Cosmic Frontier Simulator - VAMC Propulsion".into(),
                resolution: (1280.0, 720.0).into(),
                ..default()
            }),
            ..default()
        }))

        .insert_resource(SimulationParameters::new())
        .add_systems(Startup, setup)

        .add_systems(Update, update_gyroscopes)
        .add_systems(Update, update_thrust)
        .run();
}

// Setup scene: camera, lights, gyros, thrust arrow
fn setup(
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
                domain_gyro: domain::gyroscope::Gyroscope::new(),
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

