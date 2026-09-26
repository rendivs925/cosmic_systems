## Context

The per-patch surface detail path in
`src/infrastructure/bevy_adapters/terrain/surface/` already builds one merged
`Mesh` per close-range patch. `scatter.rs` owns `MeshAccum`, whose `push_boulder`
emits a UV-sphere displaced only by per-vertex radial jitter, and
`scatter_count_for_level` decimates each scatter category by patch LOD.
`build_vegetation_mesh` already grounds candidates with `mesh_height_m`,
`slope_deg_at`, and `surface_normal` from the shared `TerrainSource`. Budget
constants (`ROCK_COUNT`, `ROCK_MAX_LUMPS`, `BOULDER_SEGMENTS`,
`BOULDER_RINGS`, and the derived `MAX_VEGETATION_MESH_BYTES` reservation) live in
`surface/mod.rs`. See proposal.md - Why for motivation.

This change improves only the rock category of that existing path. It does not
touch terrain generation, collision, LOD selection, streaming, or the render
origin.

## Goals / Non-Goals

**Goals:**

- Rocks read as natural geology: irregular deterministic form, varied size and
  aspect, blue-noise clusters on steeper ground, embedded into the slope, and
  darkened at the base by contact occlusion.
- Keep exactly one merged scatter mesh per patch and reuse the existing
  `MeshAccum`, hash, budget, and LOD decimation owners.
- Keep generation deterministic from patch identity and seed, independent of
  frame rate, spawn order, and render cadence.
- Keep the per-patch rock count and the streaming byte reservation bounded and
  consistent.

**Non-Goals:**

- No per-rock ECS entities, no new component, resource, plugin, or asset.
- No collision, altitude, landing, or physics participation for rocks.
- No change to placement semantics of trees, grass, or the river ribbon.
- No unbounded rock count increase; any budget change requires measured evidence.

## Decisions

### 1. Replace the displaced-sphere primitive with a noise-displaced base shape

`MeshAccum` gains a procedural rock builder that starts from a closed base shape
(icosphere-like ring topology consistent with the existing ring/segment index
scheme) and displaces each vertex along its radial direction by a deterministic
fBm sampled from a rock-local seed. A low-frequency term gives the overall
asymmetry; one or two higher-frequency terms add fractured detail. `push_boulder`
is superseded by this builder.

*Alternatives considered:* keep the UV sphere and increase jitter (rejected - it
still reads as a circle and cannot produce facets); subdivide a box or
tetrahedron (rejected - more code paths and seam handling than the existing ring
topology needs); external mesh assets (rejected - breaks determinism-at-startup
and adds assets).

### 2. Optional thermal smoothing reuses the project's talus concept

A cheap iterative pass relaxes vertices whose local slope exceeds a talus
threshold by averaging with neighbours, producing the softened, debris-like
form of scree without a new erosion implementation. It is toggled per rock from
the same deterministic sequence, so some rocks are angular and some weathered.
This is a bounded local vertex operation, not a terrain-scale erosion pass.

*Alternatives considered:* full thermal erosion field per rock (rejected -
expensive and duplicates the terrain erosion owner); no smoothing (rejected -
all rocks would share one angular character).

### 3. Size and aspect vary from the instance seed

Each rock gets a deterministic non-uniform scale (a longer horizontal axis than
vertical) plus a per-instance base radius, replacing the current narrow radius
range and spherical footprint. Variation is drawn from the existing `hash01`
chain so it stays reproducible.

### 4. Blue-noise placement via a deterministic jittered grid

Candidate positions come from a stratified grid over the patch's local UV, with
a deterministic per-cell jitter and a per-candidate rejection derived from
`slope_deg_at`. This is a deterministic blue-noise approximation: it preserves
minimum separation far better than independent per-slot hashing while remaining
cheap and stable. Acceptance probability rises with slope so scree concentrates
on steep ground and gentle ground retains only sparse outcrops. Cell count and
acceptance stay capped by the existing `ROCK_COUNT` and
`scatter_count_for_level` budget, so the merged-mesh size bound still holds.

*Alternatives considered:* true Poisson-disc dart throwing (rejected - variable
iteration count and order sensitivity risk determinism and cost); pure slope
threshold (rejected - produces salt-and-pepper, not clusters).

### 5. Embed using authoritative height and surface normal

Placement continues to read `mesh_height_m`, `slope_deg_at`, and
`surface_normal` from the `TerrainSource` authority. Each rock's base is pushed
below the sampled surface by a fraction of its radius along the surface normal,
and embedded depth scales with slope so steeper ground seats rocks deeper. This
is presentation-only: the authoritative samples are never written.

*Alternatives considered:* derive a local height field from the rendered patch
triangles (rejected - makes presentation depend on render state and duplicates
terrain sampling); snap the rock bottom exactly to the surface (rejected -
still reads as floating on slopes).

### 6. Contact occlusion is baked into vertex colour

Vertex colour is multiplied by a factor derived from each vertex's normalised
axial position, darkest at the embedded base and unchanged at the crown. This
needs no extra draw call, uniform, or per-frame lighting. It composes with the
existing moisture/slope rock tint.

*Alternatives considered:* a screen-space or material AO pass (rejected -
over-scoped and not per-rock); no occlusion (rejected - the goal is grounding).

### 7. Budgets remain evidence-gated

The per-patch rock cap, lump cap, tessellation, and the derived
`MAX_VEGETATION_MESH_BYTES` reservation are updated only to match the new
primitive's vertex/index footprint, not to raise rock counts. Any count or
quality increase is deferred until profiling shows the merged scatter mesh is
not a bottleneck, keeping the streaming memory reservation honest.

## Risks / Trade-offs

- [Higher per-rock vertex count raises merged-mesh size] -> keep tessellation
  bounded, keep the count cap unchanged, and recompute
  `MAX_VEGETATION_MESH_BYTES` so the streaming reservation stays conservative;
  increase detail only with measured evidence.
- [Determinism could drift if smoothing or placement uses iteration order] ->
  seed all hashing from patch identity and index only, fix iteration order, and
  add a repeat-generation equality test.
- [Blue-noise grid could align with patch UV seams] -> offset the grid from
  patch identity and verify adjacent patches do not show a regular lattice.
- [Deeper embedding could clip rocks into terrain on very steep faces] ->
  clamp embed depth and base radius, and verify rocks remain visible on extreme
  slopes in tests.
- [Contact occlusion could darken too aggressively on small rocks] -> keep the
  occlusion factor bounded and evaluated from axial position, not absolute size.

## Migration Plan

1. Add the procedural rock builder and its deterministic tests alongside the
   existing `push_boulder`, then switch the rock loop to it.
2. Update budget constants and the byte-reservation formula in one commit with
   the primitive footprint change.
3. Roll back by reverting the commit; rocks are presentation-only, so no
   persisted simulation data or authoritative terrain state changes.

## Open Questions

- None blocking. Exact talus threshold, displacement octave count, and occlusion
  strength are aesthetic constants that can be tuned during implementation
  without changing the specs, approach, or task breakdown.
