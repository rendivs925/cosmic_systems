# Native Release Performance Audit

Recorded 2026-09-11 on the real X11 desktop with the release Rocket binary.
This is measurement evidence only. No performance settings or visual quality
were changed, and no optimization is claimed from this audit.

## Environment

| Item | Value |
|---|---|
| Displays | 2560x1600 at 240 Hz and 3840x2160 at 144 Hz |
| Native Rocket window | 1885x2076; nonzero X11 surface |
| GPU | NVIDIA GeForce RTX 5070 Laptop, driver 610.43.03 |
| Renderer selection | `DRI_PRIME=1 __NV_PRIME_RENDER_OFFLOAD=1` |
| Binary | `./target/release/cosmic_systems rocket --features dem` |
| Metrics | `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`; 600-frame rolling windows |

## Measured Results

| Scenario | CPU frame p50/p95/p99 (ms) | Effects visible/update (ms) | Other evidence |
|---|---:|---:|---|
| Discrete-GPU prelaunch | 16.66 / 17.63 / 19.04 | 0 / 0.023 | 1,261 MiB process GPU memory; 40% GPU utilization sample |
| Discrete-GPU launched ascent | 13.53 / 25.28 / 27.36 | 27 / 0.036 | 1,492 MiB process GPU memory; 45% GPU utilization sample |
| Integrated-GPU terrain cold start | 16.25 / 28.38 / 270.07 | 0 / 0.034 | 212-291 target tiles; first terrain batches were 2.1-3.2 ms worker time |
| Integrated-GPU camera/streaming stress | 20.11 / 56.83 / 68.42 | 18 / 0.033 | 582 generated tiles, 111.7 MiB estimated resident geometry, 34 upload-backlog tiles |

RSS during the discrete-GPU ascent sample was 1,772,840 KiB. The integrated
stress run rose from 1,749,180 KiB at launch-site warmup to 1,854,184 KiB
after active streaming; this is one short-run sample, not a leak conclusion.

Terrain streaming remained bounded under stress: two worker bakes per frame,
no cancellations, no evictions before the 128 MiB geometry budget, and no
reported ready-event overflow/backfill recovery. Culling rejected approximately
59-109 horizon candidates and 53-73 frustum candidates from 310 candidates in
the logged samples. The existing telemetry does not expose CPU mesh-build versus
surface-preparation time, CPU upload duration, or GPU upload completion time.

## Limitations And Decision

- `nvidia-smi` exposes utilization and memory, not GPU frame timestamps or
  terrain/shadow/material/cloud/atmosphere/VFX/UI pass timings. No GPU p50,
  p95, p99, worst frame, or pass attribution is claimed.
- The automated flight could exercise launch and active terrain streaming, but
  it did not yield an independently timestamped staging transition. Time warp
  drives bounded fixed-step backlog and therefore is not valid presentation
  frame-pacing evidence for staging.
- The current flight profile does not provide a verified 70 km, exoatmospheric,
  or orbital capture. Those altitude regimes remain unmeasured.
- Camera stress was captured together with active terrain streaming; existing
  metrics do not isolate culling, LOD reconciliation, and upload costs.

The high tail is correlated with terrain streaming/camera stress while effect
updates remain below 0.1 ms. That correlation is insufficient to attribute the
tail to a single terrain phase, so no optimization was made. Task 6.3 remains
incomplete until GPU timestamps, an exact staging marker, altitude checkpoints,
and isolated camera/terrain timing are available.

## Papua Scatter Follow-up — 2026-09-20

The production `EarthTerrainSource` wrapper did not forward
`vegetation_density`, so its default zero value rejected all tree and grass
candidates. Forwarding now exposes the existing layered cover model. A regression
test loads the resident Earth DEM through the public wrapper and uses the same
Papua geodetic-to-terrain conversion as rocket startup. It distinguishes grass
and tree atlas geometry from rocks in the merged mesh.

| Patch containing the launch site | Trees | Grass clumps |
|---|---:|---:|
| L12 | 408 cards | 1,680 |
| L13 | 420 cards | 1,809 |
| L14 | 444 cards | 1,860 |

These are generated patch-wide counts, not camera-visible instances. The sampled
launch cover is 0.650. This test validates generation, not runtime publication or
exact grounding on triangulated terrain.

Bounded normal/craft/rocket startup runs used Xvfb at 1280x720 with Vulkan.
The renderer reported the NVIDIA RTX 5070 Laptop GPU, driver 610.43.03; Xvfb
does not imply software rendering. No panic or renderer validation error was
found in those logs. Concurrent mode startup makes these runs unsuitable as
frame-time baselines.

The first rocket capture reached only L6 despite a focus maximum of L14, so
scatter never generated. Three causes were found and fixed:

1. `projected_patch_error_px` measured distance to the patch center. A camera
   standing on a large patch is far from that center, so the near-camera
   descendant chain was underestimated and never split. It now projects the
   nearest distance to the patch's enclosing spherical cap.
2. Viewport sampling was breadth-first and spent its bounded allowance on
   distant coarse patches. It now expands the highest projected error first.
3. `select_quadtree_leaves` refined freely and relied on a fixed balancing
   reserve, so localized refinement could overrun the cover budget. It now
   charges the exact neighbor-balance closure and the retained ancestor geometry
   before admitting a split, bounded by both leaf count and estimated bytes.

A later rocket capture published a complete cover through L14 with 258 target
leaves under the 300-leaf limit and a 108 MiB resident estimate under the 128 MiB
budget. Trees and rocks are visible on the Papua lowland ground. Per-patch scatter
counts now reach full density at the finest level and decimate by area for coarser
leaves, so close patches carry enough instances to read as vegetation.

**Acceptance is met for runtime generation and visibility.** Remaining known
limits: grounding still samples the LOD height field rather than the triangulated
mesh, and the visual result has not been judged on a real display. Frame pacing
was not measured because these captures ran concurrently with other modes.

## Staging Tracking And GPU Pass Follow-up — 2026-10-05

Recorded on the real X11 desktop with the release Rocket binary and the normal
user `HOME`. Unlike the earlier Xvfb/overridden-`HOME` runs, these are
comparable frame-pacing samples. The primary-vehicle filter added this session
(`PrimaryVehicle`, `Without<SpentStage> + Without<RecoveringStage>`) was verified
against a real staging transition.

Environment: `./target/release/cosmic_systems rocket`, 1280x720 window,
`AutoVsync`, `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, 600-frame rolling windows,
RTX 5070 Laptop (driver 610.43.03). The flight was driven with the shared
`Period` time-acceleration input, then sampled at real time.

### Verified staging transition

The HUD flight log recorded `t+46.1 STAGE SEPARATED (-33000 kg)` followed by
`t+47.8 STAGE 2 IGNITION`. Captures at T+42.5 s (38.9 km) and T+52.5 s (52.9 km)
both showed the chase camera and the entire HUD (STAGE 2, ASCENT, mass, fuel,
thrust) following the core upper stage. Before the filter, the first matching
rocket became the recovering booster once it separated, which also made the
shared telemetry recorder's `single()` query fail and stop recording. Both
consumers now read the non-recovering core stage.

### Frame pacing during ascent

| Scenario | p50 | p95 | p99 |
|---|---:|---:|---:|
| Launched ascent (10 consecutive 600-frame windows) | 16.73–16.81 | 17.76–18.73 | 18.05–21.37 |

Frame time is vsync-bound at ~60 Hz, so these values show stable pacing rather
than available headroom. No frame-time spike above ~21 ms was observed across
the sampled ascent.

### GPU pass timings (new opt-in diagnostics)

The existing opt-in `RenderDiagnosticsPlugin` now reports the top five GPU passes.
Nested/overlapping spans are not additive; representative ascent values:

| Pass | GPU ms |
|---|---:|
| `main_opaque_pass_3d` | 1.92–1.97 |
| `main_transparent_pass_3d` | 0.94–0.98 |
| `bloom` | 0.38 |
| `shadow_directional_light_0_cascade_3` | 0.13 |
| `tonemapping` | 0.11 |

Every measured pass is well inside the 16.7 ms frame budget, so GPU submission
is not the ascent bottleneck.

### Terrain scheduling during ascent

| Metric | Value |
|---|---:|
| `scheduling` p95 | 0.26–0.27 ms |
| `scheduling` p99 | 0.27–0.52 ms |
| `top_frame_cpu_ms` | 1.01–1.54 ms |
| `queue_peak` | 0 |

The earlier resident-patch prioritization change held through ascent: no
geometry backlog accumulated and per-frame terrain scheduling stayed sub-
millisecond.

### Orbital and no-vsync follow-up — 2026-10-05

- **Orbital capture:** a chase view at 89.4 km (T+78.3 s, 1698 m/s) renders the
  curved horizon, deep-blue ocean band, and atmosphere limb correctly. This
  closes the earlier "no exoatmospheric capture" gap.
- **No-vsync run:** with the window temporarily set to `AutoNoVsync`, the ascent
  p50 stayed at 16.4–18.6 ms (about 58 fps) rather than dropping to the ~4 ms
  implied by the measured passes. GPU passes summed to ~3.7 ms (opaque ~2.0,
  transparent ~1.2, bloom ~0.38, tonemapping ~0.11, upscaling ~0.06) and terrain
  scheduling p95 stayed below ~1.9 ms, so the ~17 ms frame is not explained by
  the instrumented passes. The p95/p99 tail worsened to 27–46 ms, with a worst
  terrain frame of ~7.1 ms. The present-mode override was reverted; the
  environment may still pace to vblank, or the remaining ~13 ms is unmeasured
  GPU/main-thread work. This is a real open question, not a resolved bottleneck.

### Remaining limits

- The no-vsync result above leaves ~13 ms/frame unaccounted for; further
  investigation needs full GPU timing and main-thread/render-thread stall
  attribution.
- Water appearance at altitude was improved this session (visible-depth scale
  4 km → 60 m), but the terrain itself still renders as broad low-relief ground
  at coarse LOD; that is not judged.
- The large circular ground shadow traced to the visual Sun-disc proxy casting
  shadows is no longer present in these captures (the disc now has
  `NotShadowCaster`/`NotShadowReceiver`), but the fix has not been compared
  against every Sun elevation.
