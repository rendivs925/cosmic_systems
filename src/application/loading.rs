//! Android loading phase: preload every texture and warm up the renderer before
//! the simulation is allowed to run.
//!
//! Texture decoding, GPU upload and pipeline compilation are the main sources of
//! start-up stutter. Holding virtual time paused behind a loading screen lets all
//! of that finish first.
//!
//! The phase is deadline-driven so it can never sit on “Finalizing renderer”
//! forever:
//! - assets normally settle when every texture resolves;
//! - after `ASSET_TIMEOUT_SECONDS` pending assets are abandoned;
//! - after `HARD_DEADLINE_SECONDS` the screen closes unconditionally;
//! - after `SKIP_AFTER_SECONDS` a tap closes it immediately.
//!
//! Animation runs on `Time<Real>` because virtual simulation time is paused.

use bevy::asset::{AssetServer, LoadState};
use bevy::prelude::*;
use bevy::time::{Real, Virtual};

use crate::android::diagnostics;
use crate::android::theme::*;
use crate::generated_assets::PRELOAD_TEXTURES;

/// Minimum warmup before handing over, so the first interactive frames have
/// their pipelines and uploads done.
const WARMUP_MIN_FRAMES: u32 = 15;
const WARMUP_MIN_SECONDS: f32 = 0.75;
/// Abandon unresolved assets after this long.
const ASSET_TIMEOUT_SECONDS: f32 = 30.0;
/// Close the loading screen no matter what after this long.
const HARD_DEADLINE_SECONDS: f32 = 45.0;
/// Allow a tap to skip after this long, in case the device is unusually slow.
const SKIP_AFTER_SECONDS: f32 = 8.0;

pub struct AndroidLoadingPlugin;

#[derive(Resource, Default)]
struct Preload {
    images: Vec<Handle<Image>>,
    failed: usize,
    settled_at: Option<f32>,
    warmup_frames: u32,
    finished: bool,
    started_at: f32,
    timed_out: bool,
    skipped: bool,
}

#[derive(Component)]
struct LoadingRoot;

#[derive(Component)]
struct ProgressFill;

#[derive(Component)]
struct ProgressLabel;

#[derive(Component)]
struct PhaseLabel;

#[derive(Component)]
struct HintLabel;

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
    let ui_font: Handle<Font> = asset_server.load(FONT_PATH);

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
                    font: ui_font.clone(),
                    font_size: FONT_DISPLAY,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
            ));
            root.spawn((
                Text::new("Preparing simulation"),
                TextFont {
                    font: ui_font.clone(),
                    font_size: FONT_BODY,
                    ..default()
                },
                TextColor(TEXT_SECONDARY),
                Node {
                    margin: UiRect::bottom(px(SPACE_6)),
                    ..default()
                },
            ));

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
                    font: ui_font.clone(),
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
                    font: ui_font.clone(),
                    font_size: FONT_BODY,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
                PhaseLabel,
            ));
            root.spawn((
                Text::new(""),
                TextFont {
                    font: ui_font,
                    font_size: FONT_CAPTION,
                    ..default()
                },
                TextColor(TEXT_SECONDARY),
                Node {
                    margin: UiRect::top(px(SPACE_2)),
                    ..default()
                },
                HintLabel,
            ));
        });
}

fn update_loading(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    real_time: Res<Time<Real>>,
    touches: Res<Touches>,
    mut preload: ResMut<Preload>,
    mut virtual_time: ResMut<Time<Virtual>>,
    mut fills: Query<&mut Node, With<ProgressFill>>,
    mut progress_labels: Query<&mut Text, (With<ProgressLabel>, Without<PhaseLabel>)>,
    mut phase_labels: Query<&mut Text, (With<PhaseLabel>, Without<ProgressLabel>)>,
    mut hint_labels: Query<
        &mut Text,
        (With<HintLabel>, Without<ProgressLabel>, Without<PhaseLabel>),
    >,
    mut roots: Query<(Entity, &mut Node), (With<LoadingRoot>, Without<ProgressFill>)>,
) {
    if preload.finished {
        return;
    }

    let total = preload.images.len().max(1);
    let elapsed = real_time.elapsed_secs() - preload.started_at;

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

    // A tap after the skip threshold aborts waiting; a stuck driver should never
    // trap the user on a loading screen.
    if preload.settled_at.is_none() && elapsed >= SKIP_AFTER_SECONDS && touches.any_just_pressed() {
        preload.skipped = true;
        diagnostics::log("loading: skipped by user tap");
    }

    let asset_gate = pending == 0 || elapsed >= ASSET_TIMEOUT_SECONDS || preload.skipped;
    if preload.settled_at.is_none() && asset_gate {
        if pending > 0 {
            preload.timed_out = true;
        }
        preload.settled_at = Some(elapsed);
        diagnostics::log(&format!(
            "loading: assets settled after {elapsed:.1}s ({loaded} loaded, {failed} failed, {pending} pending{})",
            if preload.skipped { ", skipped" } else { "" }
        ));
    }

    // Progress bar and counters.
    let resolved = loaded + failed;
    let fraction = resolved as f32 / total as f32;
    for mut node in &mut fills {
        node.width = percent(fraction * 100.0);
    }
    for mut text in &mut progress_labels {
        text.0 = if failed > 0 {
            format!("{resolved} / {total} textures  ({failed} failed)")
        } else {
            format!("{resolved} / {total} textures")
        };
    }

    let settled = preload.settled_at.is_some();
    let dots = 1 + (real_time.elapsed_secs() * 2.0) as usize % 3;
    let phase = if settled {
        "Finalizing renderer"
    } else {
        "Loading textures"
    };
    for mut text in &mut phase_labels {
        text.0 = format!("{phase}{}", ".".repeat(dots));
    }
    for mut text in &mut hint_labels {
        text.0 = if !settled && elapsed >= SKIP_AFTER_SECONDS {
            "Taking longer than expected — tap to continue".to_string()
        } else {
            String::new()
        };
    }

    if !settled {
        return;
    }

    let settled_at = preload.settled_at.unwrap_or(elapsed);
    preload.warmup_frames += 1;
    let warm =
        preload.warmup_frames >= WARMUP_MIN_FRAMES && elapsed - settled_at >= WARMUP_MIN_SECONDS;
    let forced = elapsed >= HARD_DEADLINE_SECONDS;
    if !warm && !forced {
        return;
    }

    // Hide first, then despawn: even if the layout children outlive the root for
    // a frame, the screen cannot remain visible.
    let mut removed = 0usize;
    for (entity, mut node) in &mut roots {
        node.display = Display::None;
        commands.entity(entity).despawn();
        removed += 1;
    }
    diagnostics::log(&format!("loading: hid and despawned {removed} root(s)"));
    virtual_time.unpause();
    preload.finished = true;
    diagnostics::log(&format!(
        "loading: complete in {elapsed:.1}s ({loaded} loaded, {failed} failed, {} warmup frames, reason={})",
        preload.warmup_frames,
        if forced {
            "deadline"
        } else if preload.skipped {
            "skip"
        } else if preload.timed_out {
            "timeout"
        } else {
            "ready"
        }
    ));
}
