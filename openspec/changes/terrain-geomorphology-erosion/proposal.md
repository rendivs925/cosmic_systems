## Why

The deterministic thermal/hydraulic erosion and D8 flow-accumulation code in `src/domain/services/erosion/` is compiled and unit-tested but never composed into the planet's authoritative terrain, so rendered and collidable surfaces still show the un-eroded analytic sculpt: no talus slopes, no dendritic drainage, and no hydrology signal for river presentation or wet biomes. Activating that existing implementation behind the one `TerrainSource` authority turns already-built domain work into visible, physically coherent geomorphology.

## What Changes

- Compose the existing `ErodedTerrainSource` into the planet terrain authority so erosion is part of the single `height_m` source used by collision, mesh generation, and surface maps (no second elevation path).
- **BREAKING (behavioral)**: erosion changes authored terrain elevations; existing baselines/bounds for affected planets shift and must be regenerated from the deterministic field.
- Make `mesh_height_m` erosion-consistent and seam-safe so rendered geometry matches the collision surface at a patch's own LOD, instead of the current macro analytic field that diverges from `height_m`.
- Expose D8 flow accumulation, moisture, and river-channel strength through the terrain authority so river presentation and wet biomes consume the same hydrology that carved the surface.
- Prefer offline-baked erosion/hydrology payloads where practical; retain the existing bounded, seeded runtime tile cache as the fallback, never a per-frame geological simulation.
- Keep erosion a cached/static field sampled into meshes; do not add per-vertex erosion, a second erosion implementation, or an every-frame simulation.
- Preserve performance budgets with evidence-gated cache sizes; add regression/determinism tests before changing any physics-like field.

## Capabilities

### New Capabilities

- `terrain-erosion`: deterministic thermal/hydraulic erosion and D8 hydrology field (height, flow accumulation, moisture, river strength) with seeded per-tile determinism, seam-safe feathered tile boundaries, and bounded/offline bake behavior.

### Modified Capabilities

- `terrain-source`: terrain authority now composes erosion/hydrology; `height_m`, `mesh_height_m`, `moisture`, and `river_strength` expose the eroded/hydrology field consistently.
- `terrain-rendering`: patch geometry and materials consume the eroded surface and drainage signals (including seam-safe LOD geometry and river/wet-biome appearance).

## Impact

- `src/domain/services/erosion/` (`mod.rs`, `simulate.rs`, `source.rs`): activate existing `ErodedTerrainSource`, `erode_tile`, `flow_accumulation`, `carve_rivers`; no replacement implementation.
- `src/domain/services/terrain_source/` (`layered.rs`, `mod.rs`): composition site and signal exposure.
- `src/domain/services/cube_sphere/mesh.rs`: `mesh_height_m` sampling contract for seam-safe, erosion-consistent geometry.
- `src/infrastructure/bevy_adapters/terrain/` (surface maps, scatter, streaming) and rocket `terrain_map`: consumers of hydrology/biome signals.
- `openspec/specs/terrain-source`, `openspec/specs/terrain-rendering`: requirement deltas.
- Determinism/bounds baselines, cache memory, and cold-start cost are affected and must be measured.
