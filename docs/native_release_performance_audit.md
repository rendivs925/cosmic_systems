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
