## Context

See proposal.md. The repository already has a cube-sphere patch type, deterministic `TerrainSource`, terrain patch lifecycle/cache, floating render origin, optional local SRTM, and a global Earth albedo. The active streamer currently selects one same-level 3x3 window near the rocket and removes it above 20 km, while `RocketPlanet` renders the separate bound-planet globe.

## Goals / Non-Goals

**Goals:**
- Render one continuous Earth terrain hierarchy from orbit through close ground views.
- Reuse the existing coordinate system, terrain authority, height sampling, streaming lifecycle, and render-origin system.
- Support low-cost global presentation first with the checked-in Earth texture, while keeping a stable path for local DEM and future imagery providers.
- Keep terrain selection and generation deterministic and bounded by explicit CPU, memory, and GPU budgets.

**Non-Goals:**
- Downloading, scraping, or redistributing Google Earth, Google Maps, or other unlicensed imagery.
- Shipping a high-resolution global DEM or imagery dataset in the repository.
- Full GPU-driven virtual texturing, runtime online tiles, water simulation, caves, or a full-planet collision mesh in the first implementation.
- Changing f64 simulation state, gravity, rocket physics, collision authority, or camera ownership.

## Decisions

### Use a balanced six-face quadtree, not a local patch window

The active terrain set begins with one root tile for every cube face. A visibility and projected-geometric-error traversal selects leaves; a leaf refines only when its four children are ready. The traversal balances adjacent leaves to the configured maximum level difference, including face-edge neighbors.

This replaces the current same-level 3x3 selection. It is preferred to expanding that window because a larger uniform window grows quadratically, still leaves a horizon boundary, and cannot express a planet-wide coarse surface. It is preferred to a second globe renderer because parents and children become the one visual surface.

### Use projected geometric error with deterministic selection

Each patch has a conservative geometric-error estimate based on its source elevation range and child-to-parent deviation. Its projected error is evaluated from the interpolated camera pose, field of view, viewport height, and camera-to-patch distance. Traversal inputs are quantized where necessary before caching so selection does not depend on frame rate or unordered query iteration.

The existing altitude-only local-terrain cutoff is removed. Horizon and frustum rejection reduce work, but roots and required ancestors remain available for complete silhouette coverage. Distance-only thresholds are rejected because they are resolution- and FOV-dependent.

### Preserve parent fallback and balance neighbors before rendering children

The renderer retains a selected parent until all selected children have meshes. Parent/child transitions use geometry morph data or an equivalent bounded visual transition. Neighbor balancing happens before the ready set is published. Mesh edge indices are stitched for one-level differences; short skirts remain a defensive fallback for numerical/raster gaps.

This avoids blank space and terrain popping. Rendering overlapping parent and child surfaces indefinitely is rejected because it produces z-fighting and material inconsistency.

### Compose heights once; vary rendered detail by LOD

`TerrainSource` remains the authority for base radius, global/procedural macro elevation, optional DEM samples, site calibration, and collision. The source gains an explicit, deterministic representation of bounded detail contributions and conservative elevation/error bounds. Local procedural displacement is sampled from stable geographic coordinates and fades to zero at its prescribed coarser level so parent and child edges agree.

Collision samples the fully composed local surface; it does not read render meshes or texture displacement. A camera-switched "external far / procedural near" height source is rejected because it would change physical ground height solely by changing view distance.

### Separate visual imagery from elevation authority

The existing Earth albedo becomes the initial global visual layer and is mapped by stable geographic coordinates on every terrain tile. Tile-local procedural albedo, normal, roughness, and biome data blend in at fine levels. A terrain material carries global and local coordinates independently, rather than using the current single local UV set for both.

This lets future licensed/public imagery providers replace only the global imagery layer and tile cache. Treating the current texture as elevation is rejected because it cannot supply terrain geometry or collision.

### Stage data-provider work after continuous terrain

Initial delivery uses the checked-in Earth texture plus current deterministic terrain and optional local SRTM. A later extension can add local configurable global DEM assets, public imagery, or an online provider behind a cache-backed visual source. No remote service is selected in this change.

This sequence fixes the architectural visual defect before introducing network reliability, credentials, license, cache-eviction, and asset-format complexity.

## Risks / Trade-offs

- [Root tiles raise baseline memory/draw count] -> Keep root resolution low, enforce budgets, and profile visible leaf count before increasing resolution.
- [Cross-face adjacency is error-prone] -> Add pure coordinate-neighbor tests for every face edge and corner before integrating selection.
- [Parent/child replacement exposes cracks or pops] -> Publish child meshes atomically, enforce balance, stitch edges, and test simulated readiness order.
- [Procedural details differ at LOD boundaries] -> Use geographic-coordinate sampling, explicit fade bands, and edge-equality tests.
- [Material layering increases shader and texture complexity] -> Start with the existing global albedo plus generated local maps; defer virtual texturing and remote images.
- [Terrain work stalls frames] -> Bound generation per frame initially, then move generation and imagery preparation to Bevy tasks only after profiling identifies a bottleneck.
- [Visual change accidentally affects flight behavior] -> Keep all changes presentation-only except the already-authoritative source composition; preserve regression baseline checks.

## Migration Plan

1. Add pure cube-face adjacency, quadtree selection, balance, and error-estimation tests before changing runtime selection.
2. Introduce complete low-resolution root coverage and parent fallback behind the existing terrain plugin while retaining the current source and collision systems.
3. Replace local-only selection with balanced refinement, then remove the rocket-mode bound-planet proxy after visual parity is reached.
4. Add global/local terrain material blending using the existing Earth texture and generated local surface maps.
5. Validate normal, craft, and rocket modes; run determinism regressions and inspect ground, ascent, orbital, and reentry views.
6. Roll back by restoring the current local-window selection and bound-planet proxy; no persisted state or physics migration is required.

## Open Questions

- The initial texture quality is bounded by the checked-in Earth albedo. Selecting a public local global DEM or a licensed online imagery provider is deferred because it does not alter this hierarchy or task breakdown.
