## Why

The existing terrain is a set of flat, low-resolution heightmap patches (KSC pad, RTLS pad, drone ship, lunar site) rendered as flat XZ grids with primitive per-pixel RNG, no real noise, no LOD (the `scale` field is never applied to tessellation), no quadtree, no streaming, no spherical/cube-sphere surface, and a toy `sample_terrain_height` collision check. A rocket flying orbit→surface needs scalable spherical terrain with LOD, deterministic generation, a `TerrainSource` abstraction (so real DEM data can replace procedural later), and separated render vs collision terrain.

## What Changes

- Introduce a `TerrainSource` abstraction (`ProceduralTerrainSource`, `HeightmapTerrainSource`, and an interface for `PlanetaryDemSource`) separating terrain data from rendering and collision.
- Add a cube-sphere planetary surface topology for orbital-to-surface flight, replacing the flat-plane assumption.
- Add hierarchical terrain: quadtree subdivision, screen-space LOD, crack-free transitions, deterministic seeded generation.
- Add terrain streaming with requested → generating → ready → visible → cached → evicted lifecycle and memory limits.
- Separate render terrain from collision terrain: high-resolution collision near the rocket (altitude, surface normal, slope, ground contact, landing detection); no full-planet physics mesh.
- Preserve the existing flat launch-pad patches as localized detailed site objects, generalized to use the shared height function.
- Reuse `LaunchSiteType`, `TerrainComponent`, and the existing spawn/visibility systems where possible.

## Capabilities

### New Capabilities

- `terrain-source`: Terrain data abstraction (`ProceduralTerrainSource`, `HeightmapTerrainSource`, `PlanetaryDemSource`) separating data from rendering and collision.
- `terrain-lod`: Hierarchical cube-sphere terrain with quadtree subdivision, screen-space LOD, crack-free transitions, streaming, caching, and memory limits.
- `terrain-collision`: Collision terrain separated from render terrain, with altitude, surface normal, slope, ground contact, and landing detection near the rocket.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->