## 1. Terrain source abstraction

- [x] 1.1 Add `TerrainSource` trait (`height(&planet, lat, lon)` and related queries) in domain/services
- [x] 1.2 Implement `ProceduralTerrainSource` with deterministic multi-octave noise, ridged mountains, and craters (seeded)
- [x] 1.3 Implement `HeightmapTerrainSource` sampling existing heightmap images
- [x] 1.4 Add `PlanetaryDemSource` interface stub (DEM-ready, no data download)
- [x] 1.5 Unit tests: deterministic regeneration, runtime independence, heightmap sampling, source-swap equivalence

## 2. Cube-sphere quadtree terrain

- [x] 2.1 Add cube-sphere face mapping from planet-centered direction to (face, uv)
- [x] 2.2 Add quadtree patch model with subdivision to a defined minimum resolution
- [x] 2.3 Add patch mesh generation from `TerrainSource` (replacing the flat-grid path in terrain_mesh.rs)
- [x] 2.4 Add screen-space LOD selection based on camera distance and geometric error
- [x] 2.5 Add crack-free stitching/skirts across patch LOD boundaries
- [x] 2.6 Tests: spherical alignment, quadtree subdivision correctness, crack-free boundary continuity, LOD distance behavior

## 3. Terrain streaming

- [x] 3.1 Add `TerrainPatchManager` resource with requested/generating/loading/ready/visible/cached/evicted lifecycle
- [x] 3.2 Add memory limits and eviction policy for cached patches
- [x] 3.3 Add streaming system producing/evicting patches near the rocket and camera
- [x] 3.4 Tests: lifecycle transitions, memory bound enforcement, no full-planet generation

## 4. Collision terrain

- [x] 4.1 Add collision queries: altitude above surface along normal, surface normal, slope
- [x] 4.2 Add ground contact, landing detection, and crash detection
- [x] 4.3 Increase collision resolution near the rocket landing region
- [x] 4.4 Wire collision into the rocket interaction system (replacing toy `update_rocket_terrain_interaction`)
- [x] 4.5 Tests: radar altitude, normal/slope correctness, landing/crash detection, render-collision consistency

## 5. Launch-site integration

- [x] 5.1 Generalize `TerrainComponent` and existing launch-site height sampling through `TerrainSource`
- [x] 5.2 Keep KSC/RTLS/drone-ship/lunar site patches functional as localized detailed site objects
- [x] 5.3 Regression: existing site features still spawn and render

## 6. Validation

- [x] 6.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [x] 6.2 Confirm solar, craft, and rocket modes remain functional