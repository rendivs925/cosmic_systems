## Why

Rocket mode currently renders a 3x3 local terrain window over a separate textured Earth proxy. The result is a flat visible slab with gaps at its edge instead of a continuous planetary surface. A premium Earth-scale simulator needs one hierarchical surface that remains present from orbit to ground contact.

## What Changes

- Replace the local-only terrain window with a six-face cube-sphere quadtree whose coarse roots continuously cover the active planet.
- Select and refine terrain leaves using projected geometric error, keep parents visible until children are ready, and enforce crack-free neighboring LOD transitions across cube-face boundaries.
- Compose global base elevation, optional local DEM elevation, and bounded LOD-faded procedural detail in the existing shared terrain authority.
- Render the existing global Earth albedo across terrain tiles at long distance, then blend it with tile-local procedural surface detail at close range.
- Add deterministic terrain selection, geometry, seam, and render/collision-consistency regressions.
- Defer online imagery and large external data streaming; the existing Earth texture is the first global visual layer, while configured local SRTM remains optional.

## Capabilities

### New Capabilities
- `planetary-terrain-material`: Layered terrain presentation combining global Earth imagery and local procedural surface detail across terrain LODs.

### Modified Capabilities
- `terrain-lod`: Require complete planetary root coverage, screen-space-error refinement, parent fallback, and cross-face crack-free balancing.
- `terrain-rendering`: Render the complete terrain hierarchy in the floating-origin flight frame rather than a local detail slab plus a separate globe proxy.
- `terrain-source`: Define coherent layered elevation composition from global, DEM, and procedural detail sources.
- `terrain-collision`: Preserve collision agreement with the same layered terrain surface used by rendered tiles.

## Impact

- Affected code: `terrain_streaming.rs`, `terrain_render.rs`, `cube_sphere.rs`, `terrain_source.rs`, `terrain_surface.rs`, `rocket_planet.rs`, terrain plugin composition, and terrain tests.
- The bound-planet presentation becomes the terrain hierarchy; it no longer needs a separate Earth globe proxy in rocket mode.
- No physics-coordinate, gravity, camera-authority, or external dependency change is required for the initial implementation.
- Future imagery/DEM providers will plug into the layered data and material interfaces without changing terrain selection or collision.
