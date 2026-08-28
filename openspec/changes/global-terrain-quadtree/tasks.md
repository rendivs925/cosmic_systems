## 1. Cube-Sphere Quadtree Domain

- [x] 1.1 Extend cube-sphere patch utilities with deterministic parent, child, same-face neighbor, and cross-face neighbor relationships.
- [x] 1.2 Add a pure six-face quadtree leaf-selection model with explicit root coverage, ancestry, readiness, and visibility state.
- [x] 1.3 Add conservative per-patch geometric-error estimates and projected-error selection inputs using camera projection data.
- [x] 1.4 Add pure quadtree balancing that constrains adjacent visible leaves to the configured LOD difference across face edges.
- [x] 1.5 Add deterministic tests for face-edge adjacency, root coverage, parent-child partitioning, error ordering, balancing, and readiness-independent selection.

## 2. Coherent Terrain Data

- [x] 2.1 Extend the shared terrain source composition with explicit base, macro, optional DEM, and bounded procedural-detail contributions.
- [x] 2.2 Define deterministic elevation bounds and LOD fade rules for detail contributions without making physical height depend on camera state.
- [x] 2.3 Update cube-sphere geometry generation to preserve parent-child edge equality and expose the data required for geomorph or stitch generation.
- [x] 2.4 Preserve site calibration, optional SRTM fallback, terrain collision, rocket spawning, and terrain-map sampling through the composed source.
- [x] 2.5 Add pure tests for edge-height continuity, procedural-detail fade continuity, DEM fallback, source determinism, and collision/render sample agreement.

## 3. Global Streaming Lifecycle

- [x] 3.1 Replace the same-level local 3x3 terrain request window with complete six-root initialization and screen-space-error quadtree traversal.
- [x] 3.2 Keep selected parents visible until all selected children are generated and render-ready; atomically publish child replacement.
- [x] 3.3 Apply horizon/frustum rejection, explicit CPU/memory budgets, and deterministic generation priority without evicting visible surface coverage.
- [x] 3.4 Retain and reuse cached geometry across refinement/coarsening; evict only non-visible descendants or obsolete cached tiles.
- [x] 3.5 Add streaming integration tests for root coverage, parent fallback, balanced refinement, cache reuse, bounded work, and no empty-surface state.

## 4. Tile Geometry And Terrain Rendering

- [x] 4.1 Add edge-stitch index variants for balanced neighboring leaves and retain skirts only as a numerical fallback.
- [x] 4.2 Add bounded parent-to-child geomorph data or an equivalent visual transition that prevents terrain-position popping.
- [x] 4.3 Update terrain mesh spawning, rebasing, and rotation updates to render mixed-LOD leaf sets in the existing floating-origin flight frame.
- [x] 4.4 Add a terrain material interface with stable global geographic coordinates and local tile coordinates.
- [x] 4.5 Map the existing Earth albedo across all Earth terrain tiles and blend it with generated close-range biome, normal, and roughness detail.
- [x] 4.6 Add material and rendering tests for global-image fallback, refinement blending, cross-face seams, floating-origin stability, and deterministic generated surface maps.

## 5. Rocket-Mode Integration

- [x] 5.1 Remove the rocket-mode bound-planet Earth proxy after global root terrain provides equivalent silhouette and horizon coverage.
- [x] 5.2 Preserve sun, moon, cloud, camera, orbit-prediction, telemetry, terrain-map, and debug-system presentation behavior.
- [x] 5.3 Verify ground, ascent, orbital, reentry, and landing views retain continuous terrain without blank space, z-fighting, or a local terrain slab boundary.
- [x] 5.4 Document the initial global-albedo mode, optional SRTM behavior, performance budgets, and future local/global DEM and imagery-provider extension points.

## 6. Validation And Rollout

- [ ] 6.1 Run formatting, checks, clippy, and the complete default test suite after each integrated milestone.
- [x] 6.2 Run DEM-feature checks and tests with procedural fallback and configured local SRTM coverage where available.
- [x] 6.3 Run determinism regression tests and verify no fixed-step rocket-state baseline changes are introduced by terrain presentation work.
- [ ] 6.4 Smoke-test normal, craft, and rocket modes; capture ground-level, ascent, and orbital visual evidence for seam and horizon review.
- [ ] 6.5 Profile root coverage and representative ascent/orbit views; record visible tile count, generation time, CPU memory, GPU memory, and frame time before tuning budgets.
