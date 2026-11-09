# Performance baselines

Measurements only. No optimisation is claimed from any row here.

## Current status

| field | value |
| --- | --- |
| Last measured commit | `8ea9207` (2026-11-09, release, prelaunch, 720p and 1440p) |
| Measured at the current code checkpoint | **YES — prelaunch only** |
| Reason for the scenario limit | Rocket mode leaves the pad on a Space press and this session has no input injection into the window. Prelaunch is therefore the only reachable scenario; ascent, staging and camera stress remain unmeasured at this commit |
| Frame-time mode | `COSMIC_SYSTEMS_PRESENT_MODE=none` (AutoNoVsync). With the default AutoVsync on this PRIME display the loop quantizes to a 1 s present, p50 ≈ 999.6 ms; that is a present stall, not render cost |
| Next action | none for P0-04's two resolutions; GPU pass timing, worst frame, draw calls and triangle counts remain unexposed by the telemetry |
| Diagnostic modes | implemented 2026-11-01; production cost is one `u32` comparison per fragment, and *no* native frame time with a mode enabled has been measured |

Do not read the tables below as describing the current build. They describe the
builds named in them.

## 2026-11-09 — current-commit release measurement (P0-04)

Environment: Ryzen 9 8940HX (16 cores, 30.6 GiB), NVIDIA GeForce RTX 5070
Laptop (driver 615.78.08) via X11/i3, eDP 2560x1600 @ 240 Hz plus HDMI
3840x2160 @ 144 Hz. Release binary `./target/release/cosmic_systems rocket`
built with `--features dem` from `8ea9207`.
`COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, 600-frame rolling window, prelaunch at
the default Papua site, no camera input.

i3 tiles a new window to its container, so the requested size is not the
measured size by default. Each run was made floating and resized to the named
physical size, and the i3 rect was read back before the measurement window:

| scenario | window rect | CPU p50 / p95 / p99 (ms) | worst | GPU | effects visible / update (ms) |
| --- | --- | ---: | --- | --- | ---: |
| prelaunch, 720p | 1280x720 | 19.76 / 27.25 / 32.15 | not exposed | not exposed | 0 / 0.024 |
| prelaunch, 1440p | 2560x1440 | 51.11 / 59.07 / 63.71 | not exposed | not exposed | 0 / 0.018 |

The figures are the final full 600-sample window after terrain streaming had
settled; earlier windows drift upward while the startup upload burst is still
in the terrain pipeline. Vsync is deliberately off: on this machine AutoVsync
takes about one second per present, which would report the compositor rather
than the frame cost. These are therefore render/update costs without a
present-blocked frame, and they are not comparable to the 2026-09-11 table
above, which was recorded with the default vsync and a different vehicle
state.

Reproduction:

```bash
COSMIC_SYSTEMS_PRESENT_MODE=none \
COSMIC_SYSTEMS_PERFORMANCE_METRICS=1 \
COSMIC_SYSTEMS_WINDOW_SIZE=1280x720 \
  ./target/release/cosmic_systems rocket
# then, once the window exists:
i3-msg '[title="Cosmic Systems - Rocket Flight"] floating enable'
i3-msg '[title="Cosmic Systems - Rocket Flight"] resize set 1280 720'
```

## 2026-09-11 — native release audit

Environment: real X11 desktop, 2560x1600 @ 240 Hz and 3840x2160 @ 144 Hz,
NVIDIA RTX 5070 Laptop (driver 610.43.03) via `DRI_PRIME=1`,
`./target/release/cosmic_systems rocket --features dem`,
`COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, 600-frame rolling windows.

| scenario | CPU p50 (ms) | CPU p95 (ms) | CPU p99 (ms) | worst | GPU time | memory | notes |
| --- | ---: | ---: | ---: | --- | --- | --- | --- |
| prelaunch, discrete GPU | 16.66 | 17.63 | 19.04 | not recorded | not exposed | 1,261 MiB GPU | 40 % GPU utilisation sample |
| launched ascent, discrete GPU | 13.53 | 25.28 | 27.36 | not recorded | not exposed | 1,492 MiB GPU; RSS 1,772,840 KiB | 45 % GPU utilisation sample |
| terrain cold start, integrated GPU | 16.25 | 28.38 | 270.07 | not recorded | not exposed | — | first batches 2.1–3.2 ms worker time |
| camera + streaming stress, integrated | 20.11 | 56.83 | 68.42 | not recorded | not exposed | 111.7 MiB resident geometry | 582 generated tiles, 34 backlog |

Streaming under stress stayed bounded: two worker bakes per frame, no
cancellations, no evictions before the 128 MiB geometry budget, culling rejected
about 59–109 horizon and 53–73 frustum candidates from 310.

Not measured at that time, and still not measured: GPU p50/p95/p99 or worst frame,
per-pass GPU attribution (terrain, shadows, materials, clouds, atmosphere, VFX,
UI), 70 km / exoatmospheric / orbital regimes, staging frame pacing, draw calls,
triangle counts.

## 2026-08-23 — Phase 17 baseline (measure-only)

Environment: Ryzen 9 8940HX, RTX 5070 Laptop, Xvfb + PRIME offload, release
binary at 720p, vsync on, 35 s runs.

| scenario | peak RSS | GPU utilisation (avg) | notes |
| --- | ---: | ---: | --- |
| solar | 610 MB | 15 % | full n-body visualisation |
| craft | 628 MB | 22 % | UFO mode |
| rocket ascent (Falcon 9) | 640 MB | 15 % | pad → ascent, streaming active |

CPU: one 30 s rocket run measured 35.5 s task-clock (~1.2 cores average).
Memory floor is dominated by Bevy/wgpu/shader pipelines plus terrain caches
(~600 MB); one extra mode costs ~30 MB.

## 2026-11-05 — terrain phase attribution (RD-02)

Environment: Ryzen 9 8940HX, RTX 5070 Laptop, **debug** binary, offscreen capture
runner (no window, fixed 60 Hz `ScheduleRunnerPlugin`), warm erosion cache,
`COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, 900 frames, `COSMIC_SYSTEMS_CAPTURE_TIME=0`.
The reported window is the last 600 frames (10 s); the upload figures come from
the window that still contains the startup stream. These are CPU phase timings.
Frame-time percentiles from this runner are the runner's cadence, not render cost,
so they are deliberately not quoted.

| scenario | culling p95 / p99 (ms/frame) | LOD reconcile p95 / p99 (ms/frame) | culling total (ms / 600 f) | LOD total (ms / 600 f) | meshes uploaded | mesh build (ms total / per mesh) | CPU submission proxy (ms total / per mesh) |
| --- | --- | --- | ---: | ---: | ---: | ---: | ---: |
| prelaunch, Papua | 0.257 / 0.283 | 0.865 / 1.028 | 64.99 | 211.98 | 352 | 5.79 / 0.0164 | 67.62 / 0.192 |
| 40 km over Papua | 0.171 / 0.178 | 0.597 / 0.674 | 36.13 | 142.06 | 347 | 6.46 / 0.0186 | 65.13 / 0.188 |

How to read the numbers:

* **Culling and LOD reconcile are cadenced at 30 Hz**, so their p50 is zero
  (every other frame does no selection work). The p95/p99 columns are the
  reconcile-frame cost; the totals are the sum over 600 frames, i.e. about 300
  reconciles. `viewport_culling_ms` includes the per-frame viewport extraction
  plus the frustum/horizon traversal; the traversal is the dominant part.
* **Upload is a startup burst, not a steady rate.** All 347–352 meshes arrived
  in the first window. Per mesh, mesh construction was 0.016–0.019 ms and the
  inclusive CPU submission proxy was 0.188–0.192 ms. `cpu_to_gpu_submission_ms`
  is a CPU-side timer around asset submission, **not** GPU completion; GPU
  upload time is still not exposed.
* Culling counts in the same runs: prelaunch 310 candidates, 18 horizon and 43
  frustum rejected; 40 km 254 candidates, 36 horizon and 102 frustum rejected.
* The 600-frame window (`TERRAIN_ATTRIBUTION_HISTORY_CAPACITY`) replaced a
  120-frame window because the 5 s report cadence otherwise skipped the upload
  burst entirely — the first full report would already be steady state.

## What a complete baseline must contain

The programme needs one row per scenario, at a named commit, with every column
either a number or an explicit "not exposed by telemetry":

| scenario | p50 | p95 | p99 | worst | CPU | GPU | streaming | memory | draw calls |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 1. prelaunch | — | — | — | — | — | — | — | — | — |
| 2. low-altitude flight | — | — | — | — | — | — | — | — | — |
| 3. 10 km | — | — | — | — | — | — | — | — | — |
| 4. 30 km | — | — | — | — | — | — | — | — | — |
| 5. 70 km | — | — | — | — | — | — | — | — | — |
| 6. orbital | — | — | — | — | — | — | — | — | — |
| 7. camera stress | — | — | — | — | — | — | — | — | — |
| 8. streaming stress | — | — | — | — | — | — | — | — | — |
| 9. staging | — | — | — | — | — | — | — | — | — |
| 10. relaunch | — | — | — | — | — | — | — | — | — |

Every `—` is "no current measurement". None of them is an estimate.

## Telemetry that exists

`performance` target reports `sample_count`, `p50_frame_ms`, `p95_frame_ms`,
`p99_frame_ms`, plus Rocket effect counts and update cost. `terrain_performance`
reports completion poll, viewport culling, LOD selection and scheduling timings,
plus patch counts and cache bytes. `render_performance` reports per-pass GPU span
times when the backend supports timestamp queries.

Known gaps in the telemetry itself: GPU upload completion, draw calls, triangle
counts. CPU mesh-build versus surface preparation and the inclusive CPU
submission proxy are now split (RD-02); GPU upload duration still needs backend
timestamp support.
