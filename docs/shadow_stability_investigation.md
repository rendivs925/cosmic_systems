# Shadow stability and display-path investigation — 2026-10-10

Baseline: `e501707`. Native release, default features (including DEM), rocket
prelaunch at Papua, chase camera, Vulkan on RTX 5070 Laptop, X11/i3,
`COSMIC_SYSTEMS_PRESENT_MODE=none`, `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`.

## Stationary shadows: controlled result

Three shadow-enabled captures were **pixel-identical**. Three shadow-disabled
captures were also pixel-identical. Re-enabling shadows reproduced the first
shadow-enabled capture exactly. Thus no stationary flicker was detected in this
scene and sampled interval; this does not establish moving-camera stability.

### Method

The existing screenshot path (`F12`), simulation clock, Bevy virtual clock,
directional light, and terrain telemetry were reused. A temporary `Last` system
in `src/main.rs` exposed two diagnostic controls:

- F8: `Time<Virtual>::pause()` and `SimulationTime.paused = true`.
- F9: invert `DirectionalLight.shadows_enabled`.

The extra system was removed after capture, and the normal release binary was
rebuilt. These are **not shipping key bindings**. F9 also toggles the existing
flight recorder; no frame-timing comparison is derived from this shadow run.

After 65 seconds of startup, the window was focused and both clocks were paused
(logged presentation time: 64.222874221 seconds). Another 30 seconds elapsed
before capture. Terrain workers were not forcibly stopped: the existing terrain
telemetry reported zero publication, activation, mesh construction, and ready
queue activity during the capture interval. The paused presentation clock also
stopped cloud/water animation and time-driven terrain reconciliation.

Three screenshots were taken with shadows enabled, three disabled, then one
after restoring shadows. Captures within each group were approximately four
seconds apart. Four seconds were allowed after each shadow toggle. The actual
framebuffer was **1245 × 1548**, as sized by the tiling window manager; it was not
the 1280 × 720 requested at application startup.

Full-frame decoded RGB comparison (including UI), with no crop or threshold:

| Comparison | Changed pixels | RMSE / 255 | Largest channel difference |
| --- | ---: | ---: | ---: |
| Enabled first → second / third | 0 | 0 | 0 |
| Disabled first → second / third | 0 | 0 | 0 |
| Enabled first → restored | 0 | 0 | 0 |
| Enabled first → disabled first | 59,460 | 0.01316635 | 112 |

The last comparison is the positive control: visible pad, tank, tower, and tree
shadows disappear when shadow mapping is disabled. Exact restoration rules out
an intervening visible scene change in the sampled endpoints. Sparse screenshots
do not rule out a transient between captures.

### Evidence

Original F12 captures in `~/cosmic_systems_images/`, in capture order:

```text
cosmic_systems_1791623463278.png  enabled 0
cosmic_systems_1791623467421.png  enabled 1
cosmic_systems_1791623471699.png  enabled 2
cosmic_systems_1791623479914.png  disabled 0
cosmic_systems_1791623484031.png  disabled 1
cosmic_systems_1791623488199.png  disabled 2
cosmic_systems_1791623496440.png  restored
```

Session-local reproduction artifacts (not versioned and not durable):
`/tmp/opencode/shadow_probe.patch`, `shadow_probe.py`, `compare_shadows.py`,
`shadow_probe.log`, and `shadow_{on_0,on_1,on_2,off_0,off_1,off_2,restored}.png`.
The patch applies the temporary controls to the baseline. The Python capture
script uses xdotool; the comparator uses Pillow and NumPy. Compare decoded RGB
arrays with `sqrt(mean((a-b)^2))/255`, using floating-point subtraction, and count
pixels for which any RGB channel differs.

## Camera-pose sweeps: repeatable shadows and gradual cascade softening

A follow-up captured **90 images** at controlled camera poses, with matching
shadows enabled/disabled at every pose. No history-dependent difference was
detected when retracing either path. The visible loss of fine tree-shadow detail
was localized to the first cascade blend, rather than a change in terrain LOD.

### Controls and coverage

Both runs used a verified 1280 × 720 framebuffer, vertical FOV 45 degrees, and
the same release configuration as above. A temporary diagnostic module paused
`Time<Virtual>` and `SimulationTime` after 65 seconds, saved the chase camera's
pose, and drove the camera in `PostUpdate` before transform propagation. A
temporary early return in `stream_terrain_patches` when presentation delta was
zero prevented residency reconciliation and completion publication. Capture
started at real time 90 seconds. Each shadow state was held for 1.5 seconds,
with a screenshot requested 0.8 seconds into the interval.

- First run: yaw `0 → +0.4° → 0 → −0.4° → 0` in 0.1-degree increments,
  followed by backward displacement `0 → 150 m → 0` in 25 m increments along
  the saved camera's local +Z. Seventeen rotation poses plus thirteen dolly
  poses produced 60 images.
- Second run: initial zero pose, then backward displacement `0 → 60 m` in
  5 m increments and a return to zero. Fifteen poses produced 30 images.

Every capture logged 648 mesh entities. A hash of sorted entity IDs, mesh
handles, and local transform bits remained constant within each run (different
runs have different hashes). This checks those inputs, not arbitrary in-place
asset mutations or material contents. All 15 repeated poses in the first run
were pixel-identical in both shadow states; the second run's repeated initial
and final zero poses were also pixel-identical in both states.

The camera override isolates rendering from the normal camera controller. It
does not exercise camera smoothing, clearance, render-origin changes, terrain
handoff, or all application-level camera-dependent presentation updates. These
are held-pose samples; they cannot exclude transients between captures.

### Rotation registration

For a pure rotation at a fixed camera position, image registration uses
`H = K C R C K^-1`, where `K` is the pixel intrinsic matrix, `R` maps base-camera
coordinates into rotated-camera coordinates, and `C = diag(1,-1,-1)` converts
Bevy camera axes to computer-vision axes. OpenCV bilinear warping aligned each
image back to the base view. The baseline was warped outward and back as an
interpolation control. Analysis used the UI-free ROI `x=[410,805), y=[80,440)`.

The post-tonemapping RGB difference `shadow-off − shadow-on` isolates the
shadow-dependent image contribution; it is not a physical shadow visibility
buffer. The baseline mask selected 3,774 pixels with maximum absolute channel
difference above 8/255. Across the eight nonzero unique yaw angles:

- Mean absolute shadow residual: **0.84–1.43 / 255**.
- 95th percentile of per-pixel mean absolute shadow residual: **3.01–5.83 / 255**.
- Corresponding shadow-off registration-control means: **1.18–2.28 / 255**.
- Local maximum per-pixel mean shadow residual reached **45.30 / 255**;
  small average residuals therefore do not prove every edge is stable.

Registered, amplified shadow montages show consistent overall placement with
localized edge/detail differences. No widespread shadow displacement was
identified in these samples. The control is a reference for rasterization and
resampling sensitivity, not proof that every residual is unrelated to shadows.

### First cascade transition

Logged flight-camera cascade far bounds were 250, 1,077.2173, 4,641.589, and
20,000 m, with 0.2 overlap. Their world texel sizes were approximately 0.20654,
0.88916, 3.83008, and 16.50195 m. The additional UI-camera cascade entries in the
logs were not used for this analysis.

Bevy 0.17.3's `bevy_pbr/src/render/shadows.wgsl`,
`fetch_directional_shadow`, linearly blends the first two cascades from
`250 * (1 - 0.2) = 200 m` to 250 m of view depth. The shader also scales normal
bias by each cascade's texel size. Thus both filtering resolution and bias can
contribute to the appearance change; this experiment does not separate them.

The fine dolly sweep was registered using a plane homography derived from the
logged camera matrix and launch-apron top plane. This is exact for that plane
and approximate for nearby terrain; it is not valid for elevated tree canopies
or tower members. The foreground tree's ground-shadow sample at baseline pixel
`(722,430)` was approximately 198.0 m deep under this plane approximation.

In the registered ground-only ROI `x=[709,747), y=[420,443)`, peak mean-RGB
shadow difference decreased gradually:

| Camera backward displacement | Approximate sample depth | Peak RGB difference / 255 |
| ---: | ---: | ---: |
| 0 m | 198 m | 69.33 / 255 |
| 10 m | 208 m | 62.24 / 255 |
| 20 m | 218 m | 56.77 / 255 |
| 30 m | 228 m | 52.27 / 255 |
| 40 m | 238 m | 50.65 / 255 |
| 50 m | 248 m | 46.29 / 255 |
| 60 m | 258 m | 45.93 / 255 |

The integrated RGB difference likewise decreased from 8,493.67 to 6,224.91 by
50 m, then remained approximately 6,229–6,231 at 55–60 m. These image-space
statistics are descriptive, not radiometric energy measurements. Five-metre
samples and visual inspection support a gradual softness transition through
the expected blend interval, with no sampled discrete disappearance or pop.
They do not establish continuity at every intermediate pose or other cascade
boundaries.

### Motion evidence and reproduction

Evidence is retained outside the repository at
`~/cosmic_systems_images/evidence/shadow-motion/`:

- `motion_00.png` through `motion_59.png`: first sweep (even=on, odd=off).
- `motion_fine_00.png` through `motion_fine_29.png`: fine sweep.
- `motion_shadow_montage.png`, `motion_dolly_montage.png`, and
  `motion_fine_shadow_montage.png`: inspected visual comparisons.
- `motion_probe.log`, `motion_fine_probe.log`: poses, cascade matrices,
  mesh fingerprints, and runtime logs.
- `rotation_analysis.txt`, `transition_analysis.txt`: numerical results.
- `motion_integration.patch` plus `motion_module.patch` (first sweep) or
  `motion_fine_module.patch` (fine sweep): temporary instrumentation against
  `e501707`. `run_motion_probe.py` runs the executable; its current log filename is
  for the fine sweep. `analyze_motion_probe.py` and `analyze_motion_fine.py`
  implement the comparisons using Pillow, NumPy, and OpenCV.

The scripts use `/tmp/opencode` paths; restore inputs there or adjust paths when
reproducing. Apply only one module patch with the integration patch, build the
release binary, run the capture script, then the matching analysis script.
All diagnostic source changes were removed after capture. No shadow tuning was
retained: the observations support the existing cascade-blend explanation and
do not establish a defect requiring a renderer change.

## Display-path comparison: inconclusive

The environment now exposes two active outputs: eDP at 2560 × 1600 / 240 Hz and
HDMI-1-0 at 3840 × 2160 / 144 Hz. XRandR lists AMD Radeon 610M and NVIDIA-G0
providers. Connector/provider names alone do not prove the application's actual
copy or scanout path.

A second run used the restored normal executable. The same floating window was
held at a verified **1280 × 720**, moved wholly between the panel `(100,100)` and
HDMI `(2800,100)`, and kept focused. The scenario and camera were unchanged;
animation continued. After a 60-second warmup, each A–B–B–A stage had a planned
25-second settling period followed by 25 seconds of metrics. No renderer or
simulation tuning was applied.

The intended discriminator was a repeatable display-dependent shift larger than
within-display drift. It was not met:

| Stage | Median reported p50 / p95 / p99 (ms) | Range of reported p50 (ms) |
| --- | --- | --- |
| Panel A1 | 57.804 / 82.813 / 92.203 | 42.462–62.034 |
| HDMI B1 | 60.600 / 82.217 / 92.600 | 60.131–60.778 |
| HDMI B2 | 64.262 / 88.667 / 95.249 | 63.522–66.470 |
| Panel A2 | 26.450 / 58.354 / 75.850 | 25.106–58.504 |

Each row summarizes five logged rolling-window reports, **not independent
per-stage frame percentiles**. At ~60 ms/frame, the existing 600-frame history
spans ~36 seconds, exceeding the planned settling interval. Some reports can
therefore include frames from the previous stage. Large temporal drift and this
history overlap prevent causal attribution. These values are not comparable to
the older 15–23 ms measurements as a code-regression result.

Raw session-local evidence: `/tmp/opencode/display_probe.py`,
`display_probe.log`, and `display_probe_stages.json` (timestamps and verified
window geometry).

`perf` user-space counters work (`perf_event_paranoid=2`), but `nsys`,
`renderdoccmd`, and `apitrace` were not found on PATH. `vulkaninfo` exposes NVIDIA
presentation/Optimus layers, but layer availability is not a timing trace. No
Vulkan acquire/present or PRIME-copy trace was collected.

## Remaining acceptance work

- Continuous camera motion, additional cascade boundaries, and production camera
  transitions with streaming enabled remain outside the held-pose sweeps above.
  A frame-by-frame capture is needed to test transients during those operations.
- Frame pacing: record per-frame timing or ensure each measured history lies
  completely within its display stage, control background/thermal drift, and
  obtain acquire/submit/present/copy timings. The uninstrumented gap is real in
  the older profile, but attributing all of it to cross-GPU presentation remains
  a hypothesis.

No shadow configuration change or performance optimization is justified by these
experiments.

## Validation and final scope

Only this report and the repair note remain changed. `git diff --exit-code --
src/main.rs` confirmed the diagnostic source was fully removed. After restoration:

- `cargo fmt --check`, `cargo check`, and `cargo clippy` passed.
- `cargo test`: 892 library tests passed, 2 ignored; the main binary's one test
  and elevation converter's three tests also passed.
- `cargo build --release --bin cosmic_systems` passed.
- Twelve-second bounded runs of the release executable in solar, craft, and
  rocket modes each created a window, with no panic/error matches in the logs.
- `git diff --check` passed. Cargo still reports the existing upstream
  `proc-macro-error2 v2.0.1` future-incompatibility notice.

After the motion follow-up, both `src/main.rs` and
`src/infrastructure/bevy_adapters/terrain/streaming.rs` were verified unchanged
from HEAD, the temporary probe module was deleted, and the normal release binary
was rebuilt. The source tree is the same one covered by the validation above.
