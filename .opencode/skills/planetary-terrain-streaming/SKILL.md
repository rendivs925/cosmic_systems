---
name: planetary-terrain-streaming
description: Use when changing terrain LOD, patch streaming, asynchronous generation, terrain caches, visibility culling, mesh upload, prefetching, or terrain performance in Cosmic Systems.
---

# Planetary Terrain Streaming

Apply this skill for any change in `terrain_streaming.rs`,
`terrain_patch_manager.rs`, `terrain_render.rs`, terrain task scheduling, patch
caches, LOD selection, terrain visibility, or terrain performance telemetry.

## Existing Project Authority

Start with these components and extend them rather than adding competing
managers:

- `TerrainStreamingResource` and streaming systems in
  `src/infrastructure/bevy_adapters/terrain_streaming.rs`.
- `TerrainPatchManager` in `src/domain/services/terrain_patch_manager.rs`.
- Cube-sphere patch selection, projected error, neighbour balance, and stitch
  helpers in `src/domain/services/cube_sphere.rs`.
- Render asset/entity lifetime handling in
  `src/infrastructure/bevy_adapters/terrain_render.rs`.
- Terrain registration and schedule ordering in
  `src/infrastructure/plugins/mod.rs`.

Do not add a `TerrainManager`, second worker queue, second patch cache, or
parallel LOD implementation.

## Non-Negotiable Frame Contract

The Bevy main thread may:

- read camera/rocket state;
- evaluate bounded LOD and visibility requests;
- schedule/cancel/deprioritize tasks;
- poll completed task results;
- upload a bounded number of ready meshes/materials.

The Bevy main thread must not synchronously sample/generate expensive terrain,
run erosion, wait for a task, or regenerate large meshes inside a frame-critical
system.

Worker tasks may:

- sample deterministic terrain fields;
- generate erosion/hydrology fields;
- calculate normals, colours, stitch-aware mesh data, and byte estimates;
- return plain data for bounded main-thread asset upload.

Use Bevy task pools already configured by the application. Task input must own
the data it needs and task output must not mutate Bevy `World`, assets, or ECS.

## Patch Lifecycle And Scheduling

Preserve the authoritative lifecycle:

```text
UNREQUESTED -> REQUESTED -> GENERATING -> READY -> VISIBLE -> CACHED -> EVICTED
```

- Requests must be idempotent and keyed by stable patch identity.
- Keep a usable parent/coarser representation while child work is pending.
- Generate progressively: coarse/available terrain first, then refinement.
- Never remove the only visible fallback before a replacement is ready.
- Cancel or deprioritize obsolete tasks when the planet, camera view, or request
  generation changes. Never block waiting for cancellation.
- New work must be capacity-limited per frame and by a bounded task queue.
- Prioritize visible, nearby work over visible medium distance, then prefetch,
  then background work. Do not consume worker capacity on hidden or stale work.

## Visibility And LOD

Terrain request selection must apply all of the following before generation:

1. camera frustum/viewport test;
2. planetary horizon/back-face test with conservative patch bounds;
3. screen-space geometric error and existing LOD policy;
4. balanced-neighbour constraints;
5. active memory and generation budgets.

- Do not request terrain simply because it exists in the quadtree.
- Use projected/screen-space geometric error from patch size, terrain error,
  camera distance, and FOV. Avoid arbitrary distance-only thresholds.
- Use split/merge hysteresis so stationary cameras do not cause LOD churn.
- Neighbour level difference must remain <= 1, including cube-face boundaries.
- Reuse the existing conservative horizon and viewport helper paths. Do not
  replace them with optimistic culling that produces missing edge terrain.
- A camera-local neighbourhood must retain enough siblings/parents to preserve
  progressive, crack-free coverage.

## Caching And Memory

Cache by stable `PatchKey` and retain explicit ownership in the existing patch
manager/resource. Caches may hold deterministic geometry, erosion data, material
inputs, and ready render data when profiling justifies each tier.

- Every cache has a stated CPU/GPU memory budget and deterministic eviction
  policy, normally LRU while protecting required visible fallback patches.
- Cache hits must be equivalent to fresh deterministic generation.
- Avoid retaining unlimited material assets, mesh handles, task outputs, or
  stale patch entities after eviction/planet swap.
- Release both ECS render entities and unique Bevy asset handles when patches
  are evicted.
- Keep exact contact caching separate from visual patch caching. Contact-cache
  misses must directly use the same authoritative source.
- Preallocate task staging arrays and use `Vec::with_capacity` for known mesh
  sizes. Do not allocate temporary vectors per vertex or sample.

## Rates, Budgets, And Priorities

Rendering runs per frame. Camera/LOD can run at presentation rate when cheap;
terrain scheduling should be cadence-limited when camera/job state is stable.
Geological stages run only on cache generation, not at render rate.

Maintain explicit budget controls for:

- visible/active patch count;
- terrain CPU cache bytes;
- generated mesh and GPU residency;
- in-flight task count and task submission rate;
- ready-result uploads per frame;
- terrain scheduling CPU time.

For a 60 FPS target, terrain scheduling should consume a small bounded portion
of the 16.67 ms frame budget. Profile actual device behaviour before changing
limits. Do not make a global quality reduction the first response to an
individual terrain bottleneck.

## Prefetching

When camera velocity and a stable trajectory are available, prefetch toward
`camera_position + velocity * prediction_time` at lower priority than visible
terrain. Prefetch is optional and must obey the same horizon, memory, queue,
and cancellation budgets. Never let prefetch delay an immediately visible tile.

## Rendering Boundary

`terrain_render.rs` consumes ready data. It must not resample terrain,
recalculate erosion, decide physical heights, or become another source of LOD
truth.

- Geometry represents macro/medium shape only; shader inputs represent
  micro-detail and continuous material variation.
- Mesh uploads are coalesced and bounded. Preserve ready work across frames when
  the upload budget is exhausted.
- Keep terrain surface, ocean, atmosphere, and cloud rendering independent.
- Do not rely on skirts to conceal incorrect LOD selection or unbalanced edges.

## Implementation Checklist

Before editing streaming code, answer:

1. Which existing lifecycle/resource/system owns this behaviour?
2. Is the work only for currently visible or near-camera terrain?
3. Can a worker perform every expensive deterministic step?
4. What fallback is rendered while it is pending?
5. What cancels stale work?
6. What budget constrains CPU memory, GPU residency, worker tasks, and uploads?
7. Which telemetry proves the change improved the actual bottleneck?

Reject synchronous patch generation, unbounded queues/caches, generation for
the far side of a planet, ECS-per-vertex designs, duplicate cache ownership,
and hidden reductions in authoritative terrain quality.
