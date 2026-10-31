# Performance baselines

Measurements only. No optimisation is claimed from any row here.

## Current status

| field | value |
| --- | --- |
| Last measured commit | `feat/rocket-simulation` @ Phase 17 (2026-08-23) and `e501707`-era release audit (2026-09-11) |
| Measured at the current code checkpoint (`d89e7d6`) | **NO** |
| Reason | The windowed run cannot present on this machine (see `README.md`), and the offscreen capture path is pinned to a 60 Hz `ScheduleRunnerPlugin` by construction, so its frame times are the runner's cadence, not render cost |
| Next action | P0-04, blocked until a presentable display is available |

Do not read the tables below as describing the current build. They describe the
builds named in them.

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

Known gaps in the telemetry itself: CPU mesh-build versus surface-preparation
time, upload duration and GPU upload completion, draw calls, triangle counts.
