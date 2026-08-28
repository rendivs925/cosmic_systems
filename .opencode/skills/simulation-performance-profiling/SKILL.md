---
name: simulation-performance-profiling
description: Use when investigating or optimizing CPU time, GPU time, frame pacing, memory, allocations, task pools, caches, mesh upload, quality adaptation, or performance telemetry in Cosmic Systems.
---

# Simulation Performance Profiling

Use this skill before optimizing any simulator subsystem. Correctness,
architecture, determinism, measurement, then optimization is the required order.

## Existing Authorities

- `src/infrastructure/bevy_adapters/performance_systems.rs`: frame statistics,
  bounded sampling, screenshots, and quality adaptation.
- `PerformanceStats` and quality resources/components: shared performance state.
- Terrain streaming/rendering telemetry: terrain task, cache, residency, and
  upload metrics.
- Bevy task-pool setup in `main.rs` and feature-specific task usage.

Do not create a competing global profiler, quality controller, cache, task pool,
or frame-time history without extending the existing owner.

## Measurement Contract

- State the scenario, hardware/environment, baseline, target metric, and success
  threshold before changing code.
- Measure representative paths: idle map, craft, rocket prelaunch, ascent,
  terrain traversal, and cache/streaming pressure as relevant.
- Distinguish CPU main-thread time, worker time, GPU/render time, memory, frame
  pacing, and simulation throughput. FPS alone does not identify a bottleneck.
- Report p50/p95/p99 frame behaviour where possible, not only an average.
- Record display/headless limitations. A zero-size X11 window is not visual proof.

## Performance Rules

- Never move authoritative simulation to a variable-rate frame path to hide cost.
- Never lower physical correctness, terrain authority, numerical precision, or
  deterministic behaviour without an explicit product decision.
- Main thread schedules, polls, and uploads bounded work. Worker pools perform
  expensive deterministic generation, decoding, and mesh computation.
- Avoid allocations in hot per-frame/per-tick loops. Reuse buffers; reserve known
  capacity; prefer contiguous arrays for large data.
- Bound every cache, task queue, ready-result queue, mesh upload burst, and
  resident asset set. Define an eviction/cancellation policy.
- Let ECS parallelism work before adding locks, unsafe code, custom allocators,
  SIMD, or GPU compute. Profile each escalation.
- Do not log every frame/tick. Use structured cadence-limited metrics and
  actionable warnings.

## Budgeting

For every expensive feature, define:

```text
owner -> tracked metric -> budget -> overload response -> fallback
```

Examples include terrain task count/cache bytes, mesh upload count, replay
snapshot capacity, telemetry cadence, and camera/visual refresh cadence. A
fallback may retain a coarse terrain parent or skip a non-authoritative visual
update; it must not block the frame or corrupt physical state.

## Optimisation Workflow

1. Reproduce and measure the bottleneck.
2. Find the existing authority and all consumers.
3. Select the smallest change that removes measured work/allocation/contention.
4. Add telemetry or a focused regression test for the mechanism.
5. Re-measure the exact scenario against the baseline.
6. Run full correctness validation and all affected mode startups.

Reject speculative rewrites, unbounded concurrency, mutexes around high-volume
terrain/physics data, broad quality degradation without diagnosis, and claims of
performance improvement without measured evidence.
