//! Offscreen frame capture: render a scene without a window and write a PNG.
//!
//! Every other capture path in this project needs a presentable swapchain, so a
//! machine whose Vulkan presentation path is broken cannot produce a single
//! frame even though rendering itself works. This module removes that
//! dependency: when `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE` names a path, the app runs
//! with no primary window, renders the existing scene into an image render
//! target, writes one PNG, and exits.
//!
//! This is a measurement facility, not a mode. It changes nothing about the
//! simulation, and it is presentation-only: the target is never read by physics.
//!
//! Environment:
//!
//! - `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=<path.png>` enables the path.
//! - `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES=<n>` frames to render before the
//!   capture (default 300, about five seconds at the 60 Hz runner: enough for
//!   the first terrain patches to stream in and settle).
//! - `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_SIZE=<width>x<height>` (default
//!   `1280x720`).

use std::path::PathBuf;
use std::time::Duration;

use bevy::app::{AppExit, ScheduleRunnerPlugin};
use bevy::asset::RenderAssetUsages;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::image::Image;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::render::view::window::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};

/// Environment variable naming the output PNG.
pub const OFFSCREEN_CAPTURE_ENV: &str = "COSMIC_SYSTEMS_OFFSCREEN_CAPTURE";
/// Environment variable selecting the frame count before capture.
pub const OFFSCREEN_CAPTURE_FRAMES_ENV: &str = "COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES";
/// Environment variable selecting the target size, `WIDTHxHEIGHT`.
pub const OFFSCREEN_CAPTURE_SIZE_ENV: &str = "COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_SIZE";

const DEFAULT_WIDTH: u32 = 1280;
const DEFAULT_HEIGHT: u32 = 720;
const DEFAULT_FRAMES: u32 = 300;

/// A parsed offscreen-capture request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffscreenCapture {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub frames: u32,
}

impl OffscreenCapture {
    /// Parse the environment, or `None` when offscreen capture is not requested.
    ///
    /// Invalid values fall back to the defaults rather than failing the run: a
    /// malformed size must not cost a long simulation its frames.
    pub fn from_env() -> Option<Self> {
        Self::from_values(
            std::env::var(OFFSCREEN_CAPTURE_ENV).ok().as_deref(),
            std::env::var(OFFSCREEN_CAPTURE_FRAMES_ENV).ok().as_deref(),
            std::env::var(OFFSCREEN_CAPTURE_SIZE_ENV).ok().as_deref(),
        )
    }

    pub fn from_values(
        path: Option<&str>,
        frames: Option<&str>,
        size: Option<&str>,
    ) -> Option<Self> {
        let path = path?.trim();
        if path.is_empty() {
            return None;
        }
        let (width, height) = size
            .map(parse_size)
            .unwrap_or((DEFAULT_WIDTH, DEFAULT_HEIGHT));
        let frames = frames
            .and_then(|value| value.trim().parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(DEFAULT_FRAMES);
        Some(Self {
            path: PathBuf::from(path),
            width,
            height,
            frames,
        })
    }
}

fn parse_size(value: &str) -> (u32, u32) {
    let mut parts = value.split(['x', 'X']);
    let width = parts
        .next()
        .and_then(|part| part.trim().parse::<u32>().ok())
        .filter(|value| *value > 0);
    let height = parts
        .next()
        .and_then(|part| part.trim().parse::<u32>().ok())
        .filter(|value| *value > 0);
    match (width, height) {
        (Some(width), Some(height)) => (width, height),
        _ => (DEFAULT_WIDTH, DEFAULT_HEIGHT),
    }
}

/// The image every camera renders into while offscreen capture is active.
#[derive(Resource, Default)]
struct CaptureTarget {
    handle: Option<Handle<Image>>,
    frames: u32,
    requested: u32,
    captured: bool,
}

/// Plugin that renders the scene into an image and writes one PNG.
pub struct OffscreenCapturePlugin {
    capture: OffscreenCapture,
}

impl OffscreenCapturePlugin {
    pub fn new(capture: OffscreenCapture) -> Self {
        Self { capture }
    }

    /// The runner shared by every offscreen capture: a fixed 60 Hz loop, since
    /// there is no window to drive redraws.
    pub fn runner() -> ScheduleRunnerPlugin {
        ScheduleRunnerPlugin::run_loop(Duration::from_secs_f64(1.0 / 60.0))
    }
}

impl Plugin for OffscreenCapturePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(CaptureTarget {
            handle: None,
            frames: 0,
            requested: self.capture.frames,
            captured: false,
        });
        app.insert_resource(CaptureOutputPath(self.capture.path.clone()));
        let width = self.capture.width;
        let height = self.capture.height;
        app.add_systems(
            Startup,
            move |mut images: ResMut<Assets<Image>>, mut target: ResMut<CaptureTarget>| {
                target.handle = Some(images.add(capture_target_image(width, height)));
            },
        );
        app.add_systems(
            Update,
            (retarget_cameras, tick_capture_frames, capture_when_ready).chain(),
        );
    }
}

fn capture_target_image(width: u32, height: u32) -> Image {
    let size = Extent3d {
        width: width.max(1),
        height: height.max(1),
        depth_or_array_layers: 1,
    };
    let mut image = Image::new(
        size,
        TextureDimension::D2,
        vec![0u8; (size.width * size.height * 4) as usize],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    // A render target needs attachment use; the screenshot readback needs a copy
    // source. Both worlds keep the asset resident.
    image.texture_descriptor.usage = TextureUsages::COPY_SRC
        | TextureUsages::COPY_DST
        | TextureUsages::TEXTURE_BINDING
        | TextureUsages::RENDER_ATTACHMENT;
    image
}

/// Point every camera at the capture image. Run every frame: mode plugins spawn
/// their cameras during startup and a retarget system cannot assume it runs
/// after all of them.
fn retarget_cameras(target: Res<CaptureTarget>, mut cameras: Query<&mut Camera>) {
    let Some(handle) = target.handle.as_ref() else {
        return;
    };
    for mut camera in &mut cameras {
        // `RenderTarget` is not comparable; a camera is already pointed at the
        // capture image when its handle matches.
        let already = matches!(
            &camera.target,
            RenderTarget::Image(image) if image.handle == *handle
        );
        if !already {
            camera.target = RenderTarget::Image(ImageRenderTarget {
                handle: handle.clone(),
                scale_factor: bevy::math::FloatOrd(1.0),
            });
        }
    }
}

fn tick_capture_frames(mut target: ResMut<CaptureTarget>) {
    target.frames = target.frames.saturating_add(1);
}

fn capture_when_ready(
    mut commands: Commands,
    mut target: ResMut<CaptureTarget>,
    path: Res<CaptureOutputPath>,
    sim_time: Res<crate::domain::services::simulation_time::SimulationTime>,
) {
    if target.captured {
        return;
    }
    let Some(handle) = target.handle.clone() else {
        return;
    };
    if target.frames < target.requested {
        return;
    }
    target.captured = true;
    // A capture may warm-start the simulation, so record the authoritative
    // clock with the frame instead of implying it is the startup instant.
    bevy::log::info!(
        "Offscreen capture at T+{:.1} s ({} frames)",
        sim_time.sim_time_s,
        target.frames
    );
    commands
        .spawn(Screenshot::image(handle))
        .observe(save_to_disk(path.0.clone()))
        .observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            },
        );
}

/// Output path, kept as a resource so the capture system does not read the
/// environment every frame.
#[derive(Resource)]
struct CaptureOutputPath(PathBuf);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offscreen_capture_is_off_unless_a_path_is_named() {
        assert!(OffscreenCapture::from_values(None, None, None).is_none());
        assert!(OffscreenCapture::from_values(Some("   "), None, None).is_none());
    }

    #[test]
    fn offscreen_capture_defaults_are_usable() {
        let capture = OffscreenCapture::from_values(Some("/tmp/frame.png"), None, None).unwrap();
        assert_eq!(capture.path, PathBuf::from("/tmp/frame.png"));
        assert_eq!(
            (capture.width, capture.height),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
        assert_eq!(capture.frames, DEFAULT_FRAMES);
    }

    #[test]
    fn offscreen_capture_parses_size_and_frames() {
        let capture =
            OffscreenCapture::from_values(Some("out.png"), Some("12"), Some("640x360")).unwrap();
        assert_eq!((capture.width, capture.height), (640, 360));
        assert_eq!(capture.frames, 12);
    }

    #[test]
    fn a_malformed_size_or_frame_count_falls_back_to_the_defaults() {
        let capture =
            OffscreenCapture::from_values(Some("out.png"), Some("zero"), Some("wide")).unwrap();
        assert_eq!(
            (capture.width, capture.height),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
        assert_eq!(capture.frames, DEFAULT_FRAMES);
        let capture =
            OffscreenCapture::from_values(Some("out.png"), Some("0"), Some("0x0")).unwrap();
        assert_eq!(capture.frames, DEFAULT_FRAMES);
        assert_eq!(
            (capture.width, capture.height),
            (DEFAULT_WIDTH, DEFAULT_HEIGHT)
        );
    }
}
