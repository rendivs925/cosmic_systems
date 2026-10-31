# Offscreen capture

A frame capture path that needs no window and no swapchain.

## Why it exists

Every other capture path in this project presents to a window. On a machine whose
Vulkan presentation path is broken — the symptom is one present, then

```text
WARN present_frames: wgpu_hal::vulkan: Suboptimal present of frame 0
Couldn't get swap chain texture, operation unrecoverable: Acquiring a texture failed with a generic error
```

— the app cannot produce a single frame even though rendering itself is fine. A
virtual display does not help either: Xvfb has no DRI3 extension, so wgpu reports
`No supported formats for surface` before the first frame. Both failures are in
presentation, not in this project's rendering.

Offscreen capture removes presentation from the loop: the scene renders into an
`Image` render target and the result is read back and written to a PNG.

## Usage

```bash
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=/tmp/frame.png \
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES=200 \
COSMIC_SYSTEMS_CAPTURE_POSE="-8.0,139.5,1200000" \
COSMIC_SYSTEMS_CAPTURE_TIME=42 \
  ./target/debug/cosmic_systems rocket --site papua
```

| variable | meaning |
| --- | --- |
| `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE` | output PNG path; enables the mode |
| `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES` | frames to render first (default 300) |
| `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_SIZE` | `WIDTHxHEIGHT` (default `1280x720`) |
| `COSMIC_SYSTEMS_CAPTURE_POSE` | existing deterministic viewpoint, unchanged |
| `COSMIC_SYSTEMS_CAPTURE_TIME` | existing deterministic simulation clock, unchanged |

The run exits with status 0 after writing the PNG. A malformed size or frame
count falls back to the defaults rather than failing the run.

## What it does not change

Nothing in the simulation. It sets `primary_window: None`, disables `WinitPlugin`,
drives `Update` from a 60 Hz `ScheduleRunnerPlugin`, and points every camera at
one image target. Cameras are retargeted every frame because mode plugins spawn
them during startup, and the render target is created in `Startup` so it can be
created through `Assets<Image>` rather than guessed at plugin build time.

The capture target is `Bgra8UnormSrgb` with `RENDER_ATTACHMENT | COPY_SRC`, and
the mip chain is untouched. The screenshot is written through Bevy's
`Screenshot::image` and `save_to_disk`, so the byte layout matches a window
capture.

## First results

Captured at 1,200 km nadir over the Papua site, 200-400 frames:

* The streamed planet renders fully with clouds, ocean and land.
* A/B against the same pose seven simulated hours later moves the sun glint and
  the shading, but every rectangular albedo block stays in place — so the blocks
  are albedo or geometry, not lighting or occlusion.
* 400 frames instead of 90 changes nothing, so the blocks are not patches that
  have not streamed in yet.

That is the first time this project could A/B a planet-scale frame on a machine
with no working presentation path.
