use crate::infrastructure::bevy_adapters::components::*;
use bevy::prelude::*;

pub fn spawn_rockets(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    use crate::domain::entities::rocket::Rocket;

    let rocket = Rocket::falcon9();

    // Create simple rocket mesh (cylinder)
    let mesh = Mesh::from(Cylinder::new(1.85, 70.0)); // Falcon 9 dimensions
    let mesh_handle = meshes.add(mesh);

    // Create rocket material
    let material = StandardMaterial {
        base_color: Color::srgb(0.8, 0.8, 0.8),
        metallic: 0.9,
        perceptual_roughness: 0.2,
        ..default()
    };
    let material_handle = materials.add(material);

    commands.spawn((
        RocketComponent {
            position: Vec3::new(0.0, -6300.0, 0.0), // Near Earth's surface
            velocity: Vec3::ZERO,
            orientation: Quat::IDENTITY,
            angular_velocity: Vec3::ZERO,
            mass: rocket.total_mass_kg(),
            dry_mass_kg: rocket.dry_mass_kg,
            fuel_mass: rocket.fuel_mass_kg,
            thrust: Vec3::ZERO,
            mission_state: RocketMissionState::PreLaunch,
        },
        Mesh3d(mesh_handle),
        MeshMaterial3d(material_handle),
        Transform::from_translation(Vec3::new(0.0, -6300.0, 0.0)),
        Selectable {
            name: "Falcon 9".to_string(),
            selected: false,
        },
    ));
}
