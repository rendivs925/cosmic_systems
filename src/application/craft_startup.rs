use crate::domain::services::{physics, planet_factory::PlanetFactory};
use crate::domain::value_objects::solar_system_params::SolarSystemParameters;
use crate::infrastructure::bevy_adapters::components::CameraController;
use crate::infrastructure::bevy_adapters::craft_components::*;
use crate::infrastructure::bevy_adapters::craft_ui::*;
use bevy::anti_alias::taa::TemporalAntiAliasing;
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
        Msaa::Off,
        TemporalAntiAliasing::default(),
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
            p.spawn(txt("Arrows=pitch/roll  RF=vert", dim));
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

    commands
        .spawn((
            craft,
            CraftVisual {
                kind: crate::domain::entities::craft::CraftKind::Saucer,
                core_pulse_phase: 0.0,
                ring_rotation: 0.0,
                dome_base_scale: 1.0,
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
