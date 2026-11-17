//! Android loading phase: preload every texture and warm up the renderer before
//! the simulation is allowed to run.
//!
//! Texture decoding, GPU upload and pipeline compilation are the main sources of
//! start-up stutter. Holding virtual time paused behind a loading screen lets all
//! of that finish first, so the first interactive seconds are smooth instead of
//! hitching.

use bevy::asset::{AssetServer, LoadState};
use bevy::prelude::*;
use bevy::time::Virtual;

use crate::android::diagnostics;
use crate::generated_assets::PRELOAD_TEXTURES;

/// Frames rendered after the last asset resolves, to absorb uploads and shader
/// pipeline compilation triggered by the first render of each material.
const WARMUP_FRAMES: u32 = 90;

pub struct AndroidLoadingPlugin;

#[derive(Resource, Default)]
struct Preload {
    images: Vec<Handle<Image>>,
    failed: usize,
    warmup_frames: u32,
    finished: bool,
    last_report: usize,
}

#[derive(Component)]
struct LoadingScreen;

#[derive(Component)]
struct LoadingLabel;

impl Plugin for AndroidLoadingPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Preload::default())
            .add_systems(Startup, begin_loading)
            .add_systems(Update, update_loading);
    }
}

fn begin_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut preload: ResMut<Preload>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    // Freeze simulation time; rendering keeps running so uploads and pipelines
    // complete while the loading screen is visible.
    virtual_time.pause();

    preload.images = PRELOAD_TEXTURES
        .iter()
        .map(|path| asset_server.load(*path))
        .collect();
    diagnostics::log(&format!(
        "loading: {} textures queued",
        preload.images.len()
    ));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgb(0.02, 0.02, 0.03)),
            LoadingScreen,
        ))
        .with_child((
            Text::new("Loading textures..."),
            TextFont {
                font_size: 28.0,
                ..default()
            },
            TextColor(Color::WHITE),
            LoadingLabel,
        ));
}

fn update_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut preload: ResMut<Preload>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut labels: Query<&mut Text, With<LoadingLabel>>,
    screens: Query<Entity, With<LoadingScreen>>,
) {
    if preload.finished {
        return;
    }

    let mut loaded = 0usize;
    let mut failed = 0usize;
    let mut pending = 0usize;
    for handle in &preload.images {
        match asset_server.get_load_state(handle.id()) {
            Some(LoadState::Loaded) => loaded += 1,
            Some(LoadState::Failed(_)) => failed += 1,
            _ => pending += 1,
        }
    }
    preload.failed = failed;

    if pending == 0 {
        preload.warmup_frames += 1;
        if preload.warmup_frames >= WARMUP_FRAMES {
            for entity in &screens {
                commands.entity(entity).despawn();
            }
            virtual_time.unpause();
            preload.finished = true;
            diagnostics::log(&format!(
                "loading: complete ({loaded} loaded, {failed} failed, {} warmup frames)",
                preload.warmup_frames
            ));
        }
        return;
    }

    if loaded != preload.last_report {
        preload.last_report = loaded;
        for mut text in &mut labels {
            text.0 = format!("Loading textures... {loaded}/{}", preload.images.len());
        }
        diagnostics::log(&format!(
            "loading: {loaded}/{} textures ({} failed)",
            preload.images.len(),
            failed
        ));
    }
}
