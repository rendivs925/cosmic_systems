//! Android loading phase: preload every texture and warm up the renderer before
//! the simulation is allowed to run.
//!
//! Texture decoding, GPU upload and pipeline compilation are the main sources of
//! start-up stutter. Holding virtual time paused behind a loading screen lets all
//! of that finish first. The screen reports progress, an animated phase label
//! (driven by real time, since virtual time is paused) and a hard timeout so a
//! missing asset can never leave the app stuck.

use bevy::asset::{AssetServer, LoadState};
use bevy::prelude::*;
use bevy::time::{Real, Virtual};

use crate::android::diagnostics;
use crate::android::theme::*;
use crate::generated_assets::PRELOAD_TEXTURES;

/// Frames rendered after the last asset resolves, to absorb uploads and shader
/// pipeline compilation triggered by the first render of each material.
const WARMUP_FRAMES: u32 = 90;

/// Never block forever: after this long, continue with whatever is ready.
const LOAD_TIMEOUT_SECONDS: f32 = 30.0;

pub struct AndroidLoadingPlugin;

#[derive(Resource, Default)]
struct Preload {
    images: Vec<Handle<Image>>,
    failed: usize,
    loaded: usize,
    warmup_frames: u32,
    finished: bool,
    started_at: f32,
}

#[derive(Component)]
struct LoadingRoot;

#[derive(Component)]
struct ProgressFill;

#[derive(Component)]
struct ProgressLabel;

#[derive(Component)]
struct PhaseLabel;

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
    real_time: Res<Time<Real>>,
    mut preload: ResMut<Preload>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    virtual_time.pause();
    preload.started_at = real_time.elapsed_secs();
    preload.images = PRELOAD_TEXTURES
        .iter()
        .map(|path| asset_server.load(*path))
        .collect();
    let total = preload.images.len();
    diagnostics::log(&format!("loading: {total} textures queued"));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                row_gap: px(SPACE_2),
                padding: UiRect::all(px(SPACE_4)),
                ..default()
            },
            BackgroundColor(SURFACE),
            GlobalZIndex(1000),
            LoadingRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("COSMIC SYSTEMS"),
                TextFont {
                    font_size: FONT_DISPLAY,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
            ));
            root.spawn((
                Text::new("Preparing simulation"),
                TextFont {
                    font_size: FONT_BODY,
                    ..default()
                },
                TextColor(TEXT_SECONDARY),
                Node {
                    margin: UiRect::bottom(px(SPACE_6)),
                    ..default()
                },
            ));

            // Progress track.
            root.spawn((
                Node {
                    width: percent(72),
                    max_width: px(420),
                    height: px(8),
                    ..default()
                },
                BackgroundColor(TRACK),
                BorderRadius::all(px(SPACE_1)),
            ))
            .with_child((
                Node {
                    width: percent(0),
                    height: percent(100),
                    ..default()
                },
                BackgroundColor(ACCENT),
                BorderRadius::all(px(SPACE_1)),
                ProgressFill,
            ));

            root.spawn((
                Text::new(format!("0 / {total} textures")),
                TextFont {
                    font_size: FONT_CAPTION,
                    ..default()
                },
                TextColor(TEXT_SECONDARY),
                Node {
                    margin: UiRect::top(px(SPACE_3)),
                    ..default()
                },
                ProgressLabel,
            ));
            root.spawn((
                Text::new("Loading textures"),
                TextFont {
                    font_size: FONT_BODY,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
                PhaseLabel,
            ));
        });
}

fn update_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    real_time: Res<Time<Real>>,
    mut preload: ResMut<Preload>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut fills: Query<&mut Node, With<ProgressFill>>,
    mut progress_labels: Query<&mut Text, (With<ProgressLabel>, Without<PhaseLabel>)>,
    mut phase_labels: Query<&mut Text, (With<PhaseLabel>, Without<ProgressLabel>)>,
    roots: Query<Entity, With<LoadingRoot>>,
) {
    if preload.finished {
        return;
    }
    let total = preload.images.len().max(1);

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
    preload.loaded = loaded;
    preload.failed = failed;

    let elapsed = real_time.elapsed_secs() - preload.started_at;
    let timed_out = pending > 0 && elapsed >= LOAD_TIMEOUT_SECONDS;
    if timed_out {
        diagnostics::log(&format!(
            "loading: timeout after {elapsed:.1}s with {pending} textures pending; continuing"
        ));
    }
    let settled = pending == 0 || timed_out;

    // Progress bar and counters.
    let resolved = loaded + failed;
    let fraction = resolved as f32 / total as f32;
    for mut node in &mut fills {
        node.width = percent(fraction * 100.0);
    }
    for mut text in &mut progress_labels {
        text.0 = format!("{resolved} / {total} textures  ({failed} failed)");
    }

    // Animated phase label: real time keeps moving while virtual time is paused,
    // so the screen never looks frozen.
    let dots = 1 + (real_time.elapsed_secs() * 2.0) as usize % 3;
    let phase = if settled {
        "Finalizing renderer"
    } else {
        "Loading textures"
    };
    for mut text in &mut phase_labels {
        text.0 = format!("{phase}{}", ".".repeat(dots));
    }

    if !settled {
        return;
    }

    preload.warmup_frames += 1;
    if preload.warmup_frames < WARMUP_FRAMES {
        return;
    }

    for entity in &roots {
        commands.entity(entity).despawn();
    }
    virtual_time.unpause();
    preload.finished = true;
    let total_elapsed = real_time.elapsed_secs() - preload.started_at;
    diagnostics::log(&format!(
        "loading: complete in {total_elapsed:.1}s ({loaded} loaded, {failed} failed, {} warmup frames)",
        preload.warmup_frames
    ));
}
