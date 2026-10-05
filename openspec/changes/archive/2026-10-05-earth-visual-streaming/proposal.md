## Why

The current Earth terrain path provides authoritative elevation, stable
cube-sphere geometry, and local procedural material detail, but breadth-first
LOD allocation exhausts its leaf budget before close views reach the existing
local-detail threshold. Close views are also limited by one 2048 by 1024 global
albedo texture. It cannot deliver a
Google-Earth-style globe-to-ground experience without provenance-backed,
multi-resolution Earth imagery that streams within the existing terrain
budgets.

## What Changes

- Reserve a bounded portion of existing terrain LOD capacity for the active
  camera or prelaunch focus, so visible local terrain reaches the configured
  local-surface and vegetation detail level without sacrificing coarse fallback
  coverage across the viewport.
- Add an Earth imagery-source contract with a documented, redistributable source,
  datum, coverage, resolution, license, and offline preparation workflow.
- Add a prepared, multi-resolution imagery package that maps to the existing
  cube-sphere terrain patch identity without downloading imagery at runtime.
- Extend the existing terrain streaming lifecycle so visible Earth patches use
  the best available imagery while retaining the global albedo as an immediate
  fallback during image preparation or upload.
- Add bounded imagery CPU/GPU residency and upload accounting to the existing
  terrain streaming telemetry; image work must never delay terrain geometry or
  invalidate collision authority.
- Define native-display visual and performance acceptance runs for a continuous
  globe-to-ground Earth flight camera path.

## Capabilities

### New Capabilities
- `earth-imagery-streaming`: Supplies provenance-backed, offline-prepared Earth
  imagery to visible terrain patches through the existing streaming lifecycle.

### Modified Capabilities
- None.

## Impact

- `assets/configs/terrain/` and `docs/datasets/` gain an Earth imagery manifest
  and provenance record; large imagery packages remain ignored local assets.
- `src/infrastructure/bevy_adapters/terrain/streaming.rs`, `surface.rs`, and
  `render.rs` reuse the existing request, worker, cache, upload, and material
  ownership paths.
- Terrain rendering gains imagery enrichment only; `TerrainSource`, terrain
  collision, celestial frames, and fixed-step rocket physics remain
  authoritative and unchanged.
- No online imagery service, second globe renderer, second terrain cache, or
  new runtime dependency is introduced.
