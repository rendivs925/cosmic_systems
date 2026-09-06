---
name: planetary-terrain-validation
description: Use when reviewing, testing, profiling, debugging, or accepting changes to procedural planetary terrain, terrain streaming, erosion, LOD, rendering, or terrain collision in Cosmic Systems.
---

# Planetary Terrain Validation

Use this skill to review or validate any terrain change. Treat these checks as
acceptance criteria, not optional polish.

## Required Invariants

Preserve or add focused domain tests for the changed behaviour:

- A terrain direction is finite and normalized within tolerance.
- `position.length() ~= planet_radius_m + height_m` for sampled geometry.
- Height, normal, slope, flow, moisture, and all derived positions are finite.
- Surface normals are finite and approximately unit length.
- Same seed/configuration/direction produces identical or explicitly bounded,
  deterministic output independent of sample order and thread scheduling.
- Terrain collision and authoritative render samples agree where they represent
  the same source/detail layer.
- Adjacent patch levels differ by at most one after selection/balancing.
- Cached and uncached generation produce equivalent geometry/source samples.
- Patch lifecycle transitions remain valid; cancellation cannot evict resident
  fallback geometry.
- Cache eviction respects memory limits while protecting required coverage.

## Seams And LOD

Automate seam checks. Do not accept visual inspection as the only evidence.

- Test corresponding edge samples across all cube-face boundaries.
- Test parent/child shared edge samples and coarse/fine stitch indices.
- Validate no height, normal, or material-coordinate discontinuity at cube
  boundaries.
- Validate balanced-neighbour rules before publishing leaf coverage.
- Test LOD selection around split/merge thresholds to prove hysteresis prevents
  oscillation.
- Test horizon and frustum culling conservatively: hidden far-side work is
  rejected, while visible limb/edge patches are retained.
- Test progressive refinement: a parent remains visible while a child is
  generating and ready patches replace it without holes.

Relevant existing tests live beside:

- `src/domain/services/cube_sphere.rs`
- `src/domain/services/terrain_source.rs`
- `src/domain/services/terrain_collision.rs`
- `src/domain/services/terrain_patch_manager.rs`
- `src/infrastructure/bevy_adapters/terrain_streaming.rs`
- `src/infrastructure/bevy_adapters/terrain_render.rs`

Extend the nearest existing test module instead of creating disconnected test
infrastructure.

## Performance Evidence

Never accept an optimisation based on expectation alone. Measure a scenario
that exercises the change and record the relevant metric before and after when
practical.

Terrain telemetry should make bottlenecks explainable, including:

- frame time/FPS and terrain scheduling time;
- visible, requested, generated, ready, resident, and inflight patch counts;
- generation time and generated patches per second;
- cache hits/misses, cache memory, and eviction count;
- task queue depth, stale/cancelled work, and upload backlog;
- CPU mesh generation and Bevy GPU upload timing;
- erosion/hydrology generation time when enabled.
- requested, target, and visible per-LOD patch distributions; replacement-group
  blocking; and in-flight task age.

Use the existing `terrain_streaming` structured metrics. Add a metric only when
it identifies a decision the system can make or a real performance question.
Do not log per frame or per physics tick.

## Debug Presentation

When a terrain feature is difficult to diagnose, add or reuse a presentation-
only debug view. It must never alter terrain simulation. Useful views include:

- height, normal, slope, moisture, temperature, flow, and erosion fields;
- cube face and patch key;
- patch lifecycle state and generation queue state;
- LOD level colour, patch boundaries, stitches, and wireframe;
- horizon/frustum culling decisions;
- resident/cache byte budgets and mesh triangle counts.

Keep debug data derived from authoritative state. Gate expensive debug geometry
behind an explicit mode and do not add per-vertex ECS entities.

In rocket mode, terrain LOD gizmos require both master debug and the explicit
F10 LOD toggle. Their centers must use the body orientation and flight
`RenderOrigin` used by terrain meshes.

## Quality Profiles

If modifying quality settings, retain one algorithm with data-driven budgets:

- Low: reduced LOD/octave/material budgets; erosion disabled only if the
  profile explicitly permits it.
- Medium: cached erosion and medium LOD budget.
- High: higher detail budget and detailed cached terrain fields.
- Ultra: maximum configured geometry/material/erosion budgets.

Profiles may reduce work but must not introduce non-determinism, different
coordinate systems, broken seams, or visual terrain used as physical truth.

## Validation Commands

Run from the repository root after each terrain change:

```text
cargo fmt --check
cargo check --features dem
cargo clippy --features dem -- -D warnings
cargo test --features dem
cargo build --release --features dem
```

Also run bounded mode startup checks when a change affects plugins, rendering,
streaming, or application composition:

```text
timeout 10s cargo run --features dem --quiet
timeout 10s cargo run --features dem --quiet -- craft
timeout 10s cargo run --features dem --quiet -- rocket
```

For graphical environments that cannot display a usable window, record the
environment limitation, inspect structured startup/terrain metrics, and do not
claim visual validation. Preserve all existing mode behaviour.

## Review Findings

During review, report findings first and rank them by severity. Treat the
following as correctness defects, not merely optimisation opportunities:

- terrain generation in a main-thread render-critical path;
- terrain generation coupled to Bevy ECS or render transforms;
- nondeterministic sampling or cache-dependent results;
- collision reading visual/overview terrain rather than the authoritative source;
- full-planet materialization or hidden-side high-LOD work;
- unbounded caches, tasks, allocations, assets, or patch entities;
- seams, cracks, unbalanced LOD, or LOD churn;
- erosion evaluated per render vertex or global hydrology every frame;
- frame-rate-dependent geological/physical results;
- coordinate precision loss at planetary distances.

For every resolved issue, state the invariant or telemetry that now proves the
behaviour. If a validation command cannot run, state exactly why and what was
run instead.
