---
name: planetary-terrain-architecture
description: Use when designing or changing procedural planet terrain fields, cube-sphere geometry, coordinates, erosion, hydrology, terrain configuration, or terrain-domain APIs in Cosmic Systems.
---

# Planetary Terrain Architecture

Apply this skill before proposing or implementing changes to planetary terrain
generation, the terrain domain model, cube-sphere topology, erosion, hydrology,
or terrain configuration.

## Existing Project Authority

Read these modules before creating a terrain abstraction or replacing logic:

- `src/domain/services/terrain_source.rs`: deterministic, authoritative terrain
  sampling and layered terrain sources.
- `src/domain/services/cube_sphere.rs`: six-face cube-sphere mapping, patch
  identities, quadtree selection, balancing, stitch data, and mesh geometry.
- `src/domain/services/erosion.rs`: cached deterministic erosion and hydrology
  fields.
- `src/domain/services/terrain_collision.rs`: authoritative collision samples.
- `src/domain/services/terrain_patch_manager.rs`: patch lifecycle and cache
  ownership.
- `src/infrastructure/bevy_adapters/terrain/streaming.rs`: Bevy scheduling and
  async patch generation.
- `src/infrastructure/bevy_adapters/terrain/render.rs`: presentation-only mesh
  upload and render entity lifecycle.

Do not introduce another terrain source, coordinate conversion, LOD hierarchy,
erosion implementation, or patch cache without first extending these owners.

## Current Data Boundary

- Earth is the sole configured terrain-data authority. It uses the versioned,
  resident ETOPO1-derived CSDEM behind the `dem` feature, with deterministic
  procedural terrain only as its explicit missing-file fallback.
- Earth CSDEM metadata supplies conservative cube-sphere patch error through
  level 8. It is source metadata, not an alternative height authority or a
  streamed elevation-payload pyramid.
- Moon remains solid-surface eligible but has no approved terrain manifest,
  local data package, datum, or validated body-fixed mapping. Do not create a
  Moon-specific terrain/collision path or speculative data adapter.

## Architectural Contract

The terrain pipeline has strict one-way ownership:

```text
Cube-sphere base geometry
  -> deterministic terrain field
  -> erosion and hydrology fields
  -> surface samples: height, normal, slope, flow, moisture
  -> sampled mesh and material inputs
  -> Bevy rendering
```

- The canonical terrain is a deterministic function of seed, configuration,
  and normalized surface direction.
- Meshes are disposable sampled representations. They are not terrain truth.
- Rendering must not generate or mutate authoritative terrain data.
- Terrain domain code must remain independent of Bevy ECS where no Bevy type is
  required. Bevy adapters schedule, materialize, and render it.
- Collision samples from the same `TerrainSource` authority as terrain meshes.
  Visual-only approximations must never be used for rocket collision, altitude,
  normals, landing, or physics.
- Do not solve a performance issue by silently changing physical terrain.

## Determinism

For identical seed, `TerrainConfig`, and normalized direction, output must be
bitwise stable where practical and numerically stable everywhere else.

- Never use wall-clock time, evaluation order, entity order, frame rate, or
  uncontrolled random state in terrain generation.
- Derive noise coordinates from deterministic seed/hash functions.
- Keep terrain generation pure: `sample(a)` must not affect `sample(b)`.
- Make expensive deterministic stages cacheable by stable `PatchKey`.
- Preserve deterministic fixed simulation rules for any dynamic terrain process.

When adding a cache, it must be an optimization only: cache misses and hits must
produce equivalent output.

## Coordinates And Precision

Use the project's existing reference-frame and physical-scale infrastructure.

- Use `DVec3`/`f64` for planetary positions, directions, radii, terrain height,
  and simulation-facing calculations.
- Use normalized surface direction as the terrain topology input. Do not make
  latitude/longitude the primary topology.
- Follow `cube face -> local UV -> cube position -> sphere direction` for mesh
  sampling and quadtree identities.
- Compute planetary surface position as `direction * (radius_m + height_m)`.
- Convert to camera-relative Bevy `Vec3` only at the presentation boundary.
- Never use `Transform` or rendered mesh coordinates as simulation truth.
- Reuse `reference_frames.rs` and `RenderOrigin`; do not create a second
  floating-origin system.

## Cube-Sphere And Patches

- Retain the six logical cube faces and `PatchKey { face, level, x, y }` style
  identity already owned by `cube_sphere.rs`.
- Keep hierarchy and adjacency as Rust data. Only visible terrain patches become
  Bevy entities.
- Sample shared boundaries identically across faces and LOD levels.
- Maintain a balanced quadtree: neighbouring patches differ by no more than one
  level. Reuse existing neighbour/balancing code rather than duplicating it.
- Prefer balanced topology and deterministic edge stitching for crack removal.
  Skirts are a fallback, not the primary correctness mechanism.
- Add LOD morphing only when the existing stitch/selection pipeline has the
  required coarse/fine correspondence and a measured popping problem.

## Layered Terrain Field

Keep terrain construction readable, purposeful, and configuration-driven:

```text
continental mask
  -> macro elevation
  -> regional mountain mask
  -> ridged mountain structure
  -> one primary domain warp, optional weak secondary warp
  -> regional detail
  -> cached erosion/hydrology contribution
  -> micro detail for shading where possible
```

- Every noise layer needs a defined spatial scale and visual/physical purpose.
- Use bounded octave counts. Aim for roughly 10-20 meaningful noise evaluations
  per normal terrain sample, not arbitrary deep fBm stacks.
- Mountains require a regional mask; never apply global ridged noise as mountain
  elevation across the entire planet.
- Domain warping is limited by default: one primary warp and at most one weak
  secondary warp. Nested warp chains require profile evidence.
- Keep micro-detail in shader normals, roughness, and procedural material detail
  rather than dense planet meshes.
- Keep constants in terrain configuration or named domain constants. Do not add
  unexplained numeric literals to sample paths.

## Erosion, Hydrology, And Surface Data

- Treat erosion as a separately generated field, never a per-vertex render
  operation and never an every-frame geological simulation.
- Erosion resolution is independent of render resolution. Sample/interpolate a
  cached field into meshes.
- Use tiers: distant terrain uses base noise, medium terrain can use cached
  approximation, and close terrain may request detailed cached fields.
- Thermal erosion enforces the configured talus behaviour through material
  transfer, not visual post-processing.
- Hydrology follows terrain gradient: rainfall -> flow -> accumulation -> river
  likelihood -> erosion/wetness/sediment. Do not add random curve rivers as a
  substitute for a field when physical appearance is required.
- Do not simulate global water every frame. Use generated/static hydrology
  except for local gameplay water systems with an explicit requirement.
- Surface samples should expose only authoritative or derivable data. Keep
  height, normal, slope, flow, moisture, and climate inputs clearly unit-named.

## CPU, GPU, And ECS Boundaries

CPU owns LOD decisions, patch scheduling, deterministic terrain/erosion fields,
mesh topology, and cache policy. GPU owns transformations, triplanar material
projection, procedural material blending, lighting, atmosphere, and detail
appearance.

- Do not create ECS entities for vertices, triangles, noise samples, or erosion
  droplets.
- Use contiguous buffers and preallocated/reused staging arrays in large domain
  computations.
- Keep ocean, atmosphere, clouds, terrain, and terrain collision as independent
  systems. Terrain determines sea-level relation; it does not own ocean meshes.
- Build material transitions continuously from height, normal/slope, climate,
  moisture, and flow. Avoid binary threshold materials.
- Use triplanar material projection for rock, soil, sand, and snow. Do not rely
  on a single planet-wide UV projection.

## Required Design Audit

Before a substantial terrain-domain change, report:

1. Existing capability and exact owner modules.
2. Missing capability and why it is needed now.
3. Reuse/extension path and why a new abstraction is unavoidable, if applicable.
4. Determinism, coordinate, collision-authority, seam, and memory risks.
5. Smallest implementation plan and validation plan.

Reject designs that generate the whole planet, make generation depend on the
render frame, run expensive erosion per vertex, duplicate collision terrain,
or replace tested cube-sphere ownership without a concrete correctness reason.
