use crate::infrastructure::bevy_adapters::components::CameraController;
use crate::infrastructure::bevy_adapters::craft_components::*;
use crate::infrastructure::bevy_adapters::craft_ui::*;
use bevy::prelude::*;

pub fn spawn_craft(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    solar_camera_query: Query<Entity, With<CameraController>>,
) {
    let disc_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.6, 0.65),
        metallic: 0.95,
        perceptual_roughness: 0.15,
        ..default()
    });
    let dome_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.7, 0.8, 0.9, 0.25),
        metallic: 0.0,
        perceptual_roughness: 0.1,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let rim_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.1, 0.2, 0.4),
        metallic: 0.8,
        perceptual_roughness: 0.3,
        emissive: LinearRgba::new(0.0, 0.1, 0.3, 1.0),
        ..default()
    });
    let core_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.3, 0.15, 0.0),
        metallic: 0.6,
        perceptual_roughness: 0.4,
        emissive: LinearRgba::new(0.8, 0.4, 0.0, 1.0),
        ..default()
    });
    let sphere_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.4, 0.45, 0.55),
        metallic: 0.85,
        perceptual_roughness: 0.25,
        ..default()
    });
    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.0, 0.6, 0.8),
        metallic: 0.5,
        perceptual_roughness: 0.2,
        emissive: LinearRgba::new(0.0, 0.4, 0.6, 1.0),
        ..default()
    });

    let disc = meshes.add(Torus::new(0.72, 0.88));
    let dome = meshes.add(Sphere::new(0.6));
    let rim = meshes.add(Cylinder::new(1.0, 0.08));
    let core = meshes.add(Sphere::new(0.2));
    let sphere = meshes.add(Sphere::new(0.5));
    let ring = meshes.add(Torus::new(0.46, 0.54));

    commands
        .spawn((
            CraftComponent::saucer(),
            CraftVisual {
                kind: crate::domain::entities::craft::CraftKind::Saucer,
                core_pulse_phase: 0.0,
                ring_rotation: 0.0,
                dome_base_scale: 1.0,
            },
            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((
                Mesh3d(sphere),
                MeshMaterial3d(sphere_mat.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0),
                CraftPart { part_type: CraftPartType::Sphere, material_handle: sphere_mat.clone() },
            ));
            parent.spawn((
                Mesh3d(disc),
                MeshMaterial3d(disc_mat.clone()),
                Transform::from_xyz(0.0, -0.1, 0.0)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                CraftPart { part_type: CraftPartType::Disc, material_handle: disc_mat.clone() },
            ));
            parent.spawn((
                Mesh3d(dome),
                MeshMaterial3d(dome_mat.clone()),
                Transform::from_xyz(0.0, 0.4, 0.0),
                CraftPart { part_type: CraftPartType::Dome, material_handle: dome_mat.clone() },
            ));
            parent.spawn((
                Mesh3d(rim),
                MeshMaterial3d(rim_mat.clone()),
                Transform::from_xyz(0.0, -0.3, 0.0),
                CraftPart { part_type: CraftPartType::Rim, material_handle: rim_mat.clone() },
            ));
            parent.spawn((
                Mesh3d(core),
                MeshMaterial3d(core_mat.clone()),
                Transform::from_xyz(0.0, 0.0, 0.0),
                CraftPart { part_type: CraftPartType::Core, material_handle: core_mat.clone() },
            ));
            parent.spawn((
                Mesh3d(ring),
                MeshMaterial3d(ring_mat.clone()),
                Transform::from_xyz(0.0, 0.05, 0.0),
                CraftPart { part_type: CraftPartType::InnerRing, material_handle: ring_mat.clone() },
            ));
        });

    for entity in solar_camera_query.iter() {
        commands.entity(entity).insert(Camera {
            order: 1,
            is_active: false,
            ..default()
        });
    }

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 8.0, 10.0).looking_at(Vec3::new(0.0, 5.0, 0.0), Vec3::Y),
        CraftCameraTag,
    ));
}

fn txt(s: &str, c: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont { font_size: 10.0, ..default() },
        TextColor(c),
    )
}

pub fn spawn_craft_ui(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 11,
            clear_color: ClearColorConfig::None,
            ..default()
        },
    ));

    let bright = Color::srgb(0.75, 0.8, 0.85);
    let dim = Color::srgb(0.4, 0.45, 0.5);

    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(60.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect::all(Val::Px(8.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.02, 0.025, 0.035, 0.75)),
        BorderColor::all(Color::srgba(0.15, 0.2, 0.3, 0.3)),
        BorderRadius::all(Val::Px(8.0)),
        CraftUiRoot,
    )).with_children(|p| {
        p.spawn(txt("=== CRAFT CONTROL ===", bright));
        p.spawn(txt("DC Field: 0.00", bright)).insert(DcFieldLabel);
        p.spawn(txt("Pulse: 0.00", bright)).insert(PulseLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("Lift: 0.0 kN", bright)).insert(LiftLabel);
        p.spawn(txt("ZPE: 0.0 kW", bright)).insert(ZpeLabel);
        p.spawn(txt("Energy: 0.00 MJ", bright)).insert(EnergyLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("CAM: External", bright)).insert(CamLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("[< , .>  DC]  [[ ]  Pulse]", dim));
        p.spawn(txt("[V] Camera  [Esc] Release", dim));
        p.spawn(txt("", bright)).insert(GainLabel);
    });
}
