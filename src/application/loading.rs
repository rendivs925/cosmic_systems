//! Android loading phase: a branded, premium loading experience.
//!
//! Texture decoding, GPU upload and pipeline compilation are the main sources
//! of start-up stutter. The loading screen runs on real (wall-clock) time while
//! virtual simulation time is paused, so the first interactive frames have
//! their pipelines and uploads finished before interaction.
//!
//! The user sees a clean branded screen with an animated indicator and cycling
//! contextual tips — no developer jargon. The phase is deadline-driven so it
//! can never sit indefinitely:
//! - assets normally settle when every texture resolves;
//! - after `ASSET_TIMEOUT_SECONDS` pending assets are abandoned;
//! - after `HARD_DEADLINE_SECONDS` the screen closes unconditionally;
//! - after `SKIP_AFTER_SECONDS` a tap closes it immediately.
//!
//! On completion the screen fades out smoothly into the scene.

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
    fade_started_at: Option<f32>,
}

#[derive(Component)]
struct LoadingRoot;

/// The animated pulse indicator (a small circle that scales/breathes).
#[derive(Component)]
struct PulseIndicator;

/// Cycling tip text.
#[derive(Component)]
struct TipLabel;

/// "Tap to continue" hint, shown only after the skip threshold.
#[derive(Component)]
struct SkipHint;

/// Brand text.
#[derive(Component)]
struct BrandLabel;

/// Contextual tips shown during loading. These are player-facing, not
/// developer-facing: no texture counts, no pipeline stages.
const LOADING_TIPS: &[&str] = &[
    "Drag to look around",
    "Pinch to zoom",
    "Tap a planet to focus",
    "Select mode from the launcher",
];

const TIP_INTERVAL_SECS: f32 = 3.0;

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
                row_gap: px(SPACE_3),
                padding: UiRect::all(px(SPACE_6)),
                ..default()
            },
            BackgroundColor(SURFACE),
            GlobalZIndex(1000),
            LoadingRoot,
        ))
        .with_children(|root| {
            // Spacer pushes content toward the vertical centre with more weight
            // above so the brand feels anchored, not cramped.
            root.spawn(Node {
                height: px(SPACE_8),
                ..default()
            });

            // Brand wordmark.
            root.spawn((
                Text::new("COSMIC SYSTEMS"),
                TextFont {
                    font: ui_font.clone(),
                    font_size: FONT_DISPLAY,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
                BrandLabel,
            ));

            // Thin accent rule under the brand.
            root.spawn((
                Node {
                    width: px(48),
                    height: px(2),
                    margin: UiRect::bottom(px(SPACE_6)),
                    ..default()
                },
                BackgroundColor(ACCENT),
                BorderRadius::all(px(1)),
            ));

            // Animated pulse indicator.
            root.spawn((
                Node {
                    width: px(14),
                    height: px(14),
                    margin: UiRect::new(px(0), px(0), px(SPACE_4), px(SPACE_4)),
                    ..default()
                },
                BackgroundColor(ACCENT),
                BorderRadius::all(px(7)),
                PulseIndicator,
            ));

            // Cycling tip.
            root.spawn((
                Text::new(LOADING_TIPS[0]),
                TextFont {
                    font: ui_font.clone(),
                    font_size: FONT_BODY,
                    ..default()
                },
                TextColor(TEXT_SECONDARY),
                TipLabel,
            ));

            // "Tap to continue" — hidden until the skip threshold.
            root.spawn((
                Node {
                    margin: UiRect::top(px(SPACE_8)),
                    padding: UiRect::new(px(SPACE_4), px(SPACE_4), px(SPACE_2), px(SPACE_2)),
                    display: Display::None,
                    ..default()
                },
                BackgroundColor(SURFACE_RAISED),
                BorderRadius::all(px(RADIUS_FULL)),
                SkipHint,
            ))
            .with_child((
                Text::new("Tap to continue"),
                TextFont {
                    font: ui_font.clone(),
                    font_size: FONT_CAPTION,
                    ..default()
                },
                TextColor(TEXT_PRIMARY),
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
    mut roots: Query<(Entity, &mut BackgroundColor, &mut Node), With<LoadingRoot>>,
    mut pulses: Query<&mut Node, (With<PulseIndicator>, Without<LoadingRoot>)>,
    mut tips: Query<&mut Text, With<TipLabel>>,
    mut skip_hints: Query<&mut Node, (With<SkipHint>, Without<LoadingRoot>)>,
) {
    // --- Animated pulse (runs on real time, independent of loading state). ---
    let elapsed = real_time.elapsed_secs() - preload.started_at;
    let phase = (elapsed * 2.2).sin() * 0.5 + 0.5; // 0..1 breathing
    for mut node in &mut pulses {
        let scale = 0.65 + phase * 0.7; // 0.65..1.35
        let size = 14.0 * scale;
        node.width = px(size);
        node.height = px(size);
    }

    // --- Cycling tip. ---
    let tip_index = (elapsed / TIP_INTERVAL_SECS) as usize % LOADING_TIPS.len();
    for mut text in &mut tips {
        text.0 = LOADING_TIPS[tip_index].to_string();
    }

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

    // Show the skip hint after the threshold.
    let settled = preload.settled_at.is_some();
    for mut node in &mut skip_hints {
        let should_show = !settled && elapsed >= SKIP_AFTER_SECONDS;
        node.display = if should_show {
            Display::Flex
        } else {
            Display::None
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

    // --- Fade-out transition before despawn. ---
    let fade_started = preload.fade_started_at.get_or_insert(elapsed);
    let fade_elapsed = elapsed - *fade_started;
    let fade_t = (fade_elapsed / FADE_OUT_SECONDS).clamp(0.0, 1.0);
    let alpha = 1.0 - fade_t;

    for (entity, mut bg, mut node) in &mut roots {
        // Lerp the background alpha toward transparent.
        let mut c = bg.0;
        if let Color::LinearRgba(ref mut lin) = &mut c {
            lin.alpha = alpha;
        }
        bg.0 = c;

        if fade_t >= 1.0 {
            node.display = Display::None;
            commands.entity(entity).despawn();
            diagnostics::log("loading: fade complete, root despawned");
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
    }
}
