## Why

Rocket mode currently launches inside a 10 km flat launch-site override and shows largely smooth terrain outside it. The simulator needs visible local relief and an optional path to real Earth elevation data while preserving one authoritative terrain source for rendering, collision, spawning, and maps.

## What Changes

- Reduce the launch-site flattening to a pad-scale, continuous clearance area so nearby terrain retains its natural relief.
- Extend deterministic procedural terrain with a local-detail layer that produces ridges, drainage-like variation, and believable surface variation at the active terrain LOD.
- Make supplied local SRTM elevation data selectable as the Earth terrain authority, with deterministic procedural fallback where coverage is absent.
- Preserve raw DEM resolution through rendering and collision rather than degrading it through the coarse erosion raster.
- Fix DEM tile caching so repeated height queries do not repeatedly parse an on-disk tile.
- Improve terrain patch invalidation so stable visible terrain does not rebuild each frame.

## Capabilities

### New Capabilities
- `earth-elevation-data`: Selects local real-world Earth elevation data with deterministic coverage and fallback behavior.

### Modified Capabilities
- `terrain-source`: Adds high-frequency deterministic local terrain detail while preserving one source for all terrain consumers.
- `terrain-rendering`: Exposes nearby relief without unnecessary stable-patch mesh rebuilds.

## Impact

- Affects the existing `TerrainSource`, DEM source, site-aware terrain wrapper, terrain streaming, and rocket-mode terrain setup.
- Does not add a second terrain renderer, collision system, coordinate frame, or asset format.
- Uses existing optional `dem` dependencies and external SRTM files supplied outside the repository; no global elevation dataset is bundled.
