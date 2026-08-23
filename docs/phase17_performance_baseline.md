# Phase 17 Performance Baseline (measure-only)

Recorded 2026-08-23 on `feat/rocket-simulation` @ Phase 17. This file
establishes evidence for a future optimization phase; **nothing was
optimized** here (AGENTS.md section 12/41).

## Environment

| Item | Value |
|---|---|
| CPU | AMD Ryzen 9 8940HX (32 threads) |
| GPU | NVIDIA RTX 5070 Laptop (8 GB) |
| Display | Xvfb + PRIME offload (headless CI-style run) |
| Binary | `./target/release/cosmic_systems --features dem`, vsync on (Bevy default), 1280x720 |
| Method | 35 s runs, RSS sampled every 4 s (`ps`), GPU util averaged from 6 `nvidia-smi` samples |

## Results

| Scenario | Peak RSS | GPU util (avg) | Notes |
|---|---|---:|---|
| solar | 610 MB | 15 % | full n-body visualization |
| craft | 628 MB | 22 % | UFO mode |
| rocket ascent (falcon9) | 640 MB | 15 % | pad -> ascent, terrain streaming active |

CPU: one 30 s rocket-mode run measured 35.5 s task-clock (~1.2 cores
average) — the main loop is far from saturating the machine.

Panic-free and staging-intact in all sampled runs.

## Interpretation / anchors

- Memory is dominated by Bevy/wgpu/shader pipelines plus terrain patch
  caches (~600 MB floor); the marginal cost of an entire extra simulation
  mode is ~30 MB.
- GPU utilization is low under vsync at this resolution; rendering is not
  the bottleneck at 720p.
- Fixed-step physics cost is time-acceleration-independent BY DESIGN:
  `SimulationTime::fixed_timestep()` stretches dt instead of running more
  steps (pinned by `time_acceleration_scales_fixed_timestep` and the
  Phase-17 `burn_rig_invariants_hold_across_time_warp_factors` test).
  A live 100x HUD run could not be driven headlessly (no input injection);
  its per-step cost is therefore identical to 1x by construction, with
  accuracy bounds covered by the Phase-17 scenarios instead.

## Known caveats

- Xvfb + offload numbers are indicative, not desktop-representative;
  absolute frame times should be re-measured on a real display before any
  optimization decision.
- No allocation profiling was attempted (`/usr/bin/time` unavailable;
  adding tooling is out of scope for a measure-only phase).
- Phase 9's `perf record` findings remain the deeper CPU profile anchor:
  render/extract path dominates; largest self-time items inside project
  code were terrain noise/geometry generation (<2% combined).

## Optimization candidates (evidence-based, deferred)

None actioned. If a future phase targets performance, re-measure first on
real display hardware and start from Phase 9's profile plus the terrain
generation symbols identified there.
