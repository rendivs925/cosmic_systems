## Context

The erosion domain (`src/domain/services/erosion/`) already implements the full simulate layer: seeded per-tile `erode_tile`, thermal `thermal_erode`, hydraulic `hydraulic_erode`, D8 `flow_accumulation`, `carve_rivers`, and the cached `ErodedTerrainSource` adapter. It is compiled, unit-tested, and deterministic, but nothing composes it into the planet authority. The authoritative `LayeredTerrainSource` sums analytic layers, and `cube_sphere::mesh` samples a separate `mesh_height_m` macro field, so erosion is invisible and render geometry can disagree with collision. See `proposal.md` for motivation.

Constraints that shape the approach:

- One `TerrainSource` authority (AGENTS.md sections 21, 50-51); no second elevation/erosion path.
- Determinism is seeded and must not depend on frame rate, evaluation order, or cache history (AGENTS.md 44; `simulation-determinism-regression` skill).
- Terrain data and rendering/collision are separate consumers (AGENTS.md 20-21).
- Performance budgets are evidence-gated (AGENTS.md 41-43; `simulation-performance-profiling` skill).
- `mesh_height_m` currently ignores erosion to keep patch edges LOD-independent; the erosion field itself is LOD-independent by construction, so this constraint can be satisfied by the field rather than by discarding it.

## Goals / Non-Goals

**Goals:**

- Compose the existing erosion/hydrology implementation into the single authoritative source used by collision, mesh generation, and surface materials.
- Make eroded ridgelines, talus slopes, valleys, and dendritic drainage visibly present and seam-safe across tiles and LODs.
- Expose flow accumulation, moisture, and river-channel strength through the authority so hydrology has one owner.
- Prefer an offline-baked elevation/hydrology payload; keep the bounded runtime cache as the fallback.
- Prove determinism, seam-safety, and collision/render agreement with tests before and after activation.

**Non-Goals:**

- No new erosion algorithm and no rewrite of `ErodedTerrainSource`, `erode_tile`, `flow_accumulation`, or `carve_rivers`; they are reused as the single implementation.
- No per-vertex erosion in mesh generation and no per-frame geological simulation.
- No new global resource or second coordinate system.
- No change to the collision surface normal owner (`terrain_collision::sample_surface`).
- No speculative GPU/compute/task-pool optimization; cache tuning is evidence-gated.

## Decisions

### Compose erosion as an inner layer of the one authority

`ErodedTerrainSource` already implements `TerrainSource` and wraps a base source, so it composes beneath `LayeredTerrainSource` rather than replacing it. The layered source keeps owning layer summation and biome composition; the composition site wraps the relevant elevation layer's source so `height_m`, `surface_sample`, `moisture`, and `river_strength` all resolve through the eroded field. The `ErodedTerrainSource` remains the only erosion implementation.

Alternative considered: a new `ErodedLayeredTerrainSource`. Rejected as a duplicate authority and an unnecessary second elevation path.

### Offline bake first, bounded runtime cache fallback

Erosion is a static field and is expensive only at first access. The preferred production path precomputes the eroded height/flow/moisture raster per tile and ships it with the elevation payload, so cold-start and frame time are unaffected. The existing `ErodedTerrainSource` LRU (`cache_max_tiles`) remains the fallback for unbaked regions and for dynamic/oversized worlds. Both paths must return the identical field, which is already required by the deterministic per-tile seed (`tile_seed` mixes the master seed with the tile key).

Alternative considered: runtime-only baking. Rejected as the default because cold-start is user-visible; kept as fallback where baking is impractical.

### Make `mesh_height_m` sample the eroded field at the patch level

Today `ErodedTerrainSource::mesh_height_m` deliberately returns the analytic base "so mesh edges are independent of LOD". The field is already LOD-independent: erosion is baked per geographic tile, not per patch, and the entity's own level only chooses how densely it samples. Seam safety is preserved by (a) the deterministic tile field plus canonical tile keys, and (b) the existing edge feather that blends each tile back to the analytic base, so two independently eroded adjacent tiles agree at their shared boundary. The mesh should therefore sample the same eroded field as `height_m` at the patch's own level. Collision continues to sample the authoritative `height_m`; near the vehicle patches are fine, so the two agree.

Alternative considered: keep the analytic macro mesh and only apply erosion to materials. Rejected because the change must show eroded geometry, and because collision/render disagreement would remain.

### Reuse the existing D8 flow and weathering channels as the single hydrology source

`HeightRaster` already carries `data` (height), `flow`, and `moisture`, and `carve_rivers` raises moisture where flow exceeds the threshold. `ErodedTerrainSource` already feathers and exposes `moisture` and `river_strength`. Consumers (surface maps, scatter, rocket overview map) already query these trait methods, so activation is a composition change, not a new interface. River presentation and wet biomes then read the same field that carved the channel.

Alternative considered: derive river strength in the renderer from a separate flow pass. Rejected as duplicate hydrology and a determinism risk.

### Keep geodesic determinism explicit

Erosion uses latitude-aware `GridSpacing`, canonical lat/lon folding, and per-tile seeds; caching uses a recency vector rather than hash iteration order. These are the mechanisms that make results cache-history independent, and they are load-bearing for the specs, so tasks verify them with regression tests rather than refactoring them.

## Risks / Trade-offs

- [Erosion changes elevations, invalidating existing baselines/bounds] → Regenerate determinism/bounds baselines from the seeded field and treat the change as behavioral; document the new expected values in tests.
- [Render geometry may disagree with collision near tile boundaries if feathering is insufficient] → Require collision/render agreement tests at shared samples and on both sides of tile edges, and tune `edge_feather` only with those tests.
- [Enabling erosion raises cold-start time and resident memory] → Prefer offline bake; keep `cache_max_tiles` bounded; measure before raising it and record evidence.
- [Mesh height changes could reintroduce LOD cracks] → Keep sampling a single deterministic field and assert shared-edge samples are identical across adjacent patches and levels.
- [Erosion hidden behind a feature or a non-default planet silently does nothing] → Ensure the composed authority is the default for planets that declare erosion and that a test asserts the eroded field is actually consumed.
- [Regression scope creep into working physics] → Reuse the existing implementation and add comparison/determinism tests; no algorithm rewrite.

## Migration Plan

1. Add tests first: determinism/cache-history, seam continuity, shared-edge mesh equality, and collision/render agreement for a composed source.
2. Compose `ErodedTerrainSource` into the planet authority composition site, defaulting to the existing `ErosionConfig` and seed.
3. Make `mesh_height_m` sample the eroded field at the patch level and remove the analytic-macro bypass.
4. Activate hydrology consumption in surface maps/scatter where it is already queried.
5. Validate `cargo fmt --check`, `cargo check`, `cargo clippy`, `cargo test`, and each application mode; compare cold-start and frame time against the pre-change baseline.
6. Rollback: unwrap the erosion composition at the single composition site; the underlying sources are unchanged, so the analytic terrain returns. Baselines must then be restored.

## Open Questions

None that block the specs, approach, or task breakdown. Cache sizing beyond the current bounded default is intentionally deferred to profiling evidence.
