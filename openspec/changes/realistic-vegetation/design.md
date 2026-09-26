## Context

See `proposal.md` - Why. The current scatter path lives in `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs`. It builds one merged `MeshAccum` per patch and is invoked from the terrain bake worker (`TerrainPatchBakeRequest::spawn` in `src/infrastructure/bevy_adapters/terrain/streaming.rs`) via `prepare_patch_surface` in `surface/mod.rs`. Placement today is uniform per-candidate hashing (`hash01`) over patch UV bounds; species differentiation is a single broadleaf/conifer atlas choice; grounding places each base at `direction * (radius + mesh_height)` with a radial `up`, so bases float on slopes. Per-patch budgets (`TREE_COUNT`, `GRASS_CLUMP_COUNT`, `ROCK_COUNT`, `MAX_VEGETATION_MESH_BYTES`) live in `surface/mod.rs`. The shared `TerrainSource` already exposes a climate-based `vegetation_density` (`terrain_source/mod.rs`, `procedural.rs`). Package ingestion already exists for imagery (`imagery_package.rs`) and local elevation (`local_elevation.rs`) with versioned provenance and deterministic fallback, which this change mirrors for land cover.

## Goals / Non-Goals

**Goals:**
- Deterministic blue-noise placement, clumping/clearing, and ecological rules feeding the existing merged per-patch mesh.
- Distinct bounded baked geometry per species, selected deterministically by land cover and climate.
- A presentation-only measured land-cover package that drives species mix and density, with a deterministic fallback.
- Correct surface grounding: bases embedded into slope and aligned to the surface normal.
- Reuse the existing worker task, `MeshAccum`, `surface_normal`, and per-patch budget accounting.

**Non-Goals:**
- Wind/animation shading and GPU instancing/impostors (deferred to later changes).
- Per-plant ECS entities, independent draw calls, or alternate terrain/collision authorities.
- Any change to terrain height, collision, radar altitude, physics, or reference frames.
- Raising scatter budgets or changing the merged-mesh streaming strategy in this increment.

## Decisions

### Keep the merged per-patch mesh first

Continue emitting all species into one `MeshAccum` per patch, generated on the existing `AsyncComputeTaskPool` worker alongside geometry and surface maps. This preserves the one-mesh-per-patch draw profile, the memory reservation in `MAX_VEGETATION_MESH_BYTES`, and patch rebasing. Alternatives considered: per-instance GPU draws or impostor LODs. Both add asset/scheduling complexity and new quality-budget controls; they are deferred until placement and species quality are correct and profiled.

### Deterministic blue-noise placement from patch identity and seed

Replace per-candidate uniform `hash01` sampling with a Poisson-disk / blue-noise process that accepts candidates under a species minimum radius, keyed by `PatchKey` (face, level, tile_x, tile_y) and the simulation seed. To stay independent of generation order, the sampler is a pure function of patch identity and seed and is evaluated entirely within that patch's UV domain; cross-patch spacing is handled by a low-frequency mask shared across patches rather than mutable neighbor state. Alternatives considered: sequential Dart-throwing with a shared global state (rejected - order dependent and non-deterministic across scheduling) and jittered grids (rejected - visible lattice artifacts).

### Clumping/clearing mask plus ecological gates

Multiply candidate density by a deterministic low-frequency mask and reject candidates outside the species altitude band, above the slope limit, at or below the water datum, or below a moisture/cover threshold. The mask is sampled from geographic coordinates (continuous across patch edges), so neighbors agree without communication. This replaces the current single-threshold `vegetation_density` thinning.

### Species selection from land cover and climate, bounded baked skeletons

A small deterministic species set (tropical broadleaf, temperate broadleaf, conifer/boreal, palm, shrub/understory, grass) is selected from measured/fallback land cover, moisture, altitude, and latitude. Each species is assembled from bounded, baked skeleton parameters (trunk/crown/frond/card counts) into the existing prism/cross-card helpers, so geometry is reproducible and bytes are predictable for budget reservation. Alternatives considered: procedural per-plant branching (rejected - unbounded cost and variance) and a single generic tree with color variation (rejected - the visual defect this change addresses).

### Measured land cover as a presentation-only resource

Add a versioned offline land-cover package (for example ESA WorldCover) following the existing `imagery_package.rs` / `local_elevation.rs` conventions: header, provenance metadata, coverage bounds, explicit missing/out-of-coverage fallback to the source's climate `vegetation_density`. The package is resolved into a resource consumed only by placement/species code in the terrain adapter. It is never read by `height_m`, `terrain_collision`, altitude, or physics. Native loads the package directly; WASM falls back to the climate density when the filesystem package is unavailable.

### Surface-aligned grounding

Compute the local surface normal from the terrain source (reusing `terrain_collision::surface_normal` and the existing patch slope sample) and align each plant's `up` to it, then lower the base by a small fraction of the plant's height so it embeds into the slope instead of floating. This replaces the radial `dir` used for `up` today.

### Placement and species logic as pure domain code

New placement and species modules hold the pure, Bevy-free algorithms (blue-noise, mask, ecological gates, species selection, skeleton parameters) so they are unit-testable without Bevy and reusable by any future renderer. The Bevy adapter in `surface/` consumes them and owns mesh emission. This follows the domain/presentation boundary in AGENTS.md and the `planetary-terrain-architecture` skill.

## Risks / Trade-offs

- [Blue-noise sampling increases per-patch CPU cost] -> Bound the candidate count by the existing budgets, reuse preallocated buffers on the worker, and measure generation time before any budget change.
- [Species geometry raises vertex counts] -> Keep skeletons bounded and covered by `MAX_VEGETATION_MESH_BYTES`; add a test asserting the budget still holds.
- [Land-cover package is large or absent] -> Treat it as optional; commit no binary by default, keep the deterministic climate fallback, and validate provenance/coverage on load.
- [Cross-patch density seams from the clump mask] -> Sample the mask from continuous geographic coordinates and test shared-edge continuity.
- [Measured cover accidentally leaks into physics] -> Keep the package behind the presentation adapter and add tests asserting height/collision/regression outputs are unchanged with and without the package.
- [Grounding change alters visible placements] -> Accept as intended; determinism tests pin exact regenerated output.

## Migration Plan

1. Add pure placement/species modules with unit tests for determinism, spacing, ecological gates, mask continuity, and skeleton bounds, without changing the runtime path.
2. Add the land-cover package reader with provenance/fallback tests and the climate-density fallback.
3. Route `build_vegetation_mesh` through the new placement and species logic while keeping merged-mesh output and budgets; add grounding via surface normal.
4. Validate with the full default and DEM test suites, clippy, and the three application modes; confirm no fixed-step physics baselines change.
5. Roll back by restoring uniform `hash01` placement and the binary canopy choice; no persisted state or physics migration is required.

## Open Questions

- The exact land-cover package and class-to-species mapping will be finalized during implementation; any such choice stays behind the presentation boundary and does not change the specs, approach, or task breakdown.
