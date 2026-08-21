## Context

Current terrain: `terrain_spawning.rs` creates flat heightmap patches (KSC 256px/10km, RTLS 128px/2km, drone ship 128px/3km, lunar 128px/5km). `terrain_heightmaps.rs` uses per-pixel `StdRng` + sines (not real noise octaves). `terrain_mesh.rs` builds flat XZ grids. `terrain_visibility.rs` has distance visibility + a `scale` LOD field never applied to tessellation. `TerrainComponent` carries planet_entity, position_offset, size_km, resolution, heightmap/surface/normal handles, launch_site_type. `rocket_systems.rs::update_rocket_terrain_interaction` is a toy height sample. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- `TerrainSource` abstraction (procedural + heightmap + DEM-ready).
- Cube-sphere surface with quadtree + screen-space LOD + crack-free transitions.
- Streaming lifecycle with memory bounds.
- Collision terrain separate from render terrain with landing/crash detection.

**Non-Goals:**
- Downloading real DEM datasets now (design the interface only).
- Terrain deformation (impact craters from rockets) — future.
- Replacing the existing launch-site patches; they become detailed site objects.

## Decisions

**Decision: `TerrainSource` trait as the data boundary (AGENTS.md sections 20-21).**
`trait TerrainSource { fn height(&self, planet, lat, lon) -> Height; }` with `ProceduralTerrainSource` (seeded noise octaves + ridged mountains + craters), `HeightmapTerrainSource` (sampled), and `PlanetaryDemSource` (future). Render mesh and collision both consume it.
- Alternative: flat heightmaps only. Rejected: cannot scale to planetary surfaces or swap in DEM.
- Alternative: external noise crate. Rejected: AGENTS.md section 59 — implement deterministic multi-octave noise internally first.

**Decision: Cube-sphere + quadtree for topology and LOD.**
Six cube faces subdivided by a quadtree; patch LOD selected by screen-space error; skirts/stitching for crack-free transitions (AGENTS.md section 22).
- Alternative: polar/UV sphere with spherical clipmaps. Rejected: cube-sphere gives uniform patch sizes and simpler quadtree; clipmap complexity not warranted yet.
- Alternative: keep flat patches. Rejected: cannot represent orbital→surface flight on a sphere.

**Decision: Streaming via a patch resource manager.**
A `TerrainPatchManager` resource tracks requested/generating/ready/visible/cached/evicted states, with configured memory limits and LRU eviction. Generation uses Bevy tasks where it pays off; start single-threaded, measure before parallelizing (AGENTS.md sections 23, 41).
- Alternative: eager full-planet generation. Rejected: violates the "never generate a full planet at max resolution" principle (AGENTS.md section 43).

**Decision: Collision terrain as a second LOD of the same source.**
Collision queries sample the same `TerrainSource` at collision resolution (coarser than render), refined near the rocket. No separate physics mesh.
- Alternative: physics-engine colliders (e.g., bevy_rapier). Rejected: AGENTS.md section 59; a height-source collision model is sufficient and dependency-free.

**Decision: Existing launch-site patches become site objects.**
Generalize `TerrainComponent`/`sample_terrain_height` to route through `TerrainSource`; keep the flat pads as localized high-detail site objects used as landing targets.
- Rationale: preserves existing functionality (AGENTS.md section 5) while unifying the height path.

## Risks / Trade-offs

- [Crack-free LOD complexity] → Use proven skirt/stitching; add patch-boundary continuity tests.
- [Terrain generation cost] → Start coarse; measure before parallelizing/streaming optimizations.
- [Cube-sphere seam distortions] → Document mapping; distribute detail with quad-sphere projection where needed.
- [Collision/render mismatch] → Collision samples the same source; tolerance tests for the configured collision resolution.

## Migration Plan

1. Add `TerrainSource` trait + procedural/heightmap implementations + deterministic generation tests.
2. Add cube-sphere quadtree patch system + LOD selection + crack-free stitching.
3. Add `TerrainPatchManager` streaming lifecycle + memory bounds.
4. Add collision terrain queries (altitude/normal/slope/landing/crash) near the rocket.
5. Generalize existing launch-site patches through `TerrainSource`.
6. Keep existing flat-site features working throughout.

## Open Questions

None.