use crate::domain::services::{physics, planet_factory::PlanetFactory};
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::CameraController;
use crate::infrastructure::bevy_adapters::craft_components::*;
use crate::infrastructure::bevy_adapters::craft_ui::*;
use bevy::audio::{PlaybackMode, Volume};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::gltf::Gltf;
use bevy::post_process::bloom::{Bloom, BloomPrefilter};
use bevy::prelude::*;

pub fn spawn_craft(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut solar_camera_query: Query<&mut Camera, With<CameraController>>,
) {
    let gltf_handle: Handle<Gltf> =
        asset_server.load("models/ufo_flying_saucer_spaceship_ovni.glb");
    let spawn_position = craft_spawn_position();
    commands.insert_resource(CraftModelLoad {
        gltf_handle,
        done: false,
        spawn_position,
    });

    for mut camera in solar_camera_query.iter_mut() {
        camera.is_active = false;
    }

    commands.spawn((
        Camera3d::default(),
        Camera {
            order: 2,
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
        Transform::from_translation(spawn_position + Vec3::new(0.0, 5.0, 16.0))
            .looking_at(spawn_position + Vec3::Y, Vec3::Y),
        CraftCameraTag,
    ));
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

    commands
        .spawn((
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
        ))
        .with_children(|p| {
            p.spawn(txt("=== CRAFT ===", bright));
            p.spawn(txt("0m/s  5m  0%", bright)).insert(FlightLabel);
            p.spawn(txt("---", dim));
            p.spawn(txt("DC: 0.00", bright)).insert(DcFieldLabel);
            p.spawn(txt("Lift: 0.0 kN", bright)).insert(LiftLabel);
            p.spawn(txt("Energy: 0.00 MJ", bright)).insert(EnergyLabel);
            p.spawn(txt("---", dim));
            p.spawn(txt("CAM: Chase", bright)).insert(CamLabel);
            p.spawn(txt("---", dim));
            p.spawn(txt("WASD=move  QE=yaw", dim));
            p.spawn(txt("KJ=pitch  HL=roll  RF=vert", dim));
            p.spawn(txt("Shift=sprint  Ctrl=hover", dim));
            p.spawn(txt("V=camera  wheel=zoom", dim));
        });
}

#[derive(Resource)]
pub struct CraftModelLoad {
    pub gltf_handle: Handle<Gltf>,
    pub done: bool,
    pub spawn_position: Vec3,
}

pub fn spawn_craft_model(
    mut commands: Commands,
    mut load: ResMut<CraftModelLoad>,
    asset_server: Res<AssetServer>,
    gltf_assets: Res<Assets<Gltf>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if load.done {
        return;
    }
    let Some(gltf) = gltf_assets.get(&load.gltf_handle) else {
        return;
    };
    let Some(scene) = gltf
        .default_scene
        .clone()
        .or_else(|| gltf.scenes.first().cloned())
    else {
        return;
    };

    load.done = true;

    let mut craft = CraftComponent::saucer();
    craft.physics.vertical_position = load.spawn_position.y;

    let bubble_mesh = meshes.add(Mesh::from(Sphere::new(5.0)));
    let ring_mesh = meshes.add(Mesh::from(Torus::new(4.5, 0.2)));
    let core_mesh = meshes.add(Mesh::from(Sphere::new(0.8)));
    let lens_mesh = meshes.add(Mesh::from(Sphere::new(4.0)));
    let wake_mesh = meshes.add(Mesh::from(Sphere::new(1.0)));

    let bubble_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.05, 0.1, 0.25, 0.15),
        emissive: LinearRgba::new(0.02, 0.05, 0.15, 1.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let ring_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.2, 0.4, 0.8, 0.4),
        emissive: LinearRgba::new(0.1, 0.3, 0.6, 1.0),
        alpha_mode: AlphaMode::Blend,
        metallic: 0.8,
        perceptual_roughness: 0.1,
        ..default()
    });
    let core_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.8, 0.9, 1.0, 1.0),
        emissive: LinearRgba::new(0.3, 0.5, 0.8, 1.0),
        ..default()
    });
    let lens_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.12, 0.25, 0.08),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let wake_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.08, 0.04, 0.15, 0.0),
        emissive: LinearRgba::new(0.0, 0.0, 0.02, 1.0),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    commands
        .spawn((
            craft,
            CraftVisual {
                kind: crate::domain::entities::craft::CraftKind::Saucer,
                core_pulse_phase: 0.0,
                ring_rotation: 0.0,
                dome_base_scale: 1.0,
                field_strength: 0.0,
                resonance_phase: 0.0,
                zpe_gain: 0.0,
                polarization_asymmetry: 0.0,
                bubble_radius: 5.0,
                wake_intensity: 0.0,
            },
            AudioPlayer::new(asset_server.load("sounds/craft_electronic.ogg")),
            PlaybackSettings {
                mode: PlaybackMode::Loop,
                volume: Volume::Linear(0.28),
                ..default()
            },
            Transform::from_translation(load.spawn_position),
            Visibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn((SceneRoot(scene), Transform::default()));
            parent.spawn((
                Mesh3d(bubble_mesh),
                MeshMaterial3d(bubble_mat),
                CraftBubble,
                Transform::default(),
                Visibility::default(),
            ));
            parent.spawn((
                Mesh3d(ring_mesh),
                MeshMaterial3d(ring_mat),
                CraftRing,
                Transform::default(),
                Visibility::default(),
            ));
            parent.spawn((
                Mesh3d(core_mesh),
                MeshMaterial3d(core_mat),
                CraftCoreGlow,
                Transform::default(),
                Visibility::default(),
            ));
            parent.spawn((
                Mesh3d(lens_mesh),
                MeshMaterial3d(lens_mat),
                CraftLens,
                Transform::from_xyz(0.0, 0.0, 2.0),
                Visibility::default(),
            ));
            parent.spawn((
                Mesh3d(wake_mesh),
                MeshMaterial3d(wake_mat),
                CraftWake,
                Transform::from_xyz(0.0, 0.0, -2.0),
                Visibility::default(),
            ));
        });
}

fn craft_spawn_position() -> Vec3 {
    let solar_params = SolarSystemParameters::for_visualization();
    let Some(earth) = PlanetFactory::create_by_name("Earth") else {
        return Vec3::new(0.0, 5.0, 0.0);
    };

    let earth_position =
        physics::calculate_planet_position(&earth, 0.0, &solar_params, Vec3::ZERO, None);
    let earth_radius = physics::calculate_visual_radius(&earth, &solar_params);

    earth_position + Vec3::new(earth_radius + 50.0, 5.0, 0.0)
}

fn txt(s: &str, c: Color) -> (Text, TextFont, TextColor) {
    (
        Text::new(s),
        TextFont {
            font_size: 10.0,
            ..default()
        },
        TextColor(c),
    )
}
