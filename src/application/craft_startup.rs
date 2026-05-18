use crate::infrastructure::bevy_adapters::components::CameraController;
use crate::infrastructure::bevy_adapters::craft_components::*;
use crate::infrastructure::bevy_adapters::craft_effects::CraftGlowMaterial;
use crate::infrastructure::bevy_adapters::craft_ui::*;
use bevy::prelude::*;

pub fn spawn_craft(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    solar_camera_query: Query<Entity, With<CameraController>>,
) {
    let glow_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.0),
        emissive: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands
        .spawn((
            CraftComponent::saucer(),
            CraftVisual {
                kind: crate::domain::entities::craft::CraftKind::Saucer,
                core_pulse_phase: 0.0,
                ring_rotation: 0.0,
                dome_base_scale: 1.0,
            },
            SceneRoot(asset_server.load("models/ufo_flying_saucer_spaceship_ovni.glb#Scene0")),
            Transform::from_translation(Vec3::new(0.0, 5.0, 0.0)),
            Visibility::default(),
            CraftGlowMaterial(glow_mat.clone()),
        ))
        .with_children(|parent| {
            // Glow halo sphere around the craft for emissive pulse effects
            parent.spawn((
                Mesh3d(meshes.add(Sphere::new(1.2))),
                MeshMaterial3d(glow_mat),
                Transform::from_xyz(0.0, 0.0, 0.0),
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
        p.spawn(txt("=== CRAFT ===", bright));
        p.spawn(txt("0m/s  5m  0%", bright)).insert(FlightLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("DC: 0.00", bright)).insert(DcFieldLabel);
        p.spawn(txt("Pulse: 0.00", bright)).insert(PulseLabel);
        p.spawn(txt("Lift: 0.0 kN", bright)).insert(LiftLabel);
        p.spawn(txt("ZPE: 0.0 kW", bright)).insert(ZpeLabel);
        p.spawn(txt("Energy: 0.00 MJ", bright)).insert(EnergyLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("CAM: Chase", bright)).insert(CamLabel);
        p.spawn(txt("", bright)).insert(GainLabel);
        p.spawn(txt("---", dim));
        p.spawn(txt("WASD=move  QE=yaw", dim));
        p.spawn(txt("Arrows=pitch/roll  RF=vert", dim));
        p.spawn(txt("Shift=sprint  Ctrl=hover", dim));
        p.spawn(txt("V=camera  wheel=zoom", dim));
    });
}
