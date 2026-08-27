## Context

See proposal.md. The existing `TerrainSource` is already the authority used by terrain mesh generation, collision, rocket spawning, and the terrain map. It combines a procedural source, optional DEM source, coarse erosion wrapper, and launch-site wrapper; cube-sphere patches are streamed only near the rocket.

The current initial KSC footprint is flat for approximately 10 km, which fully covers the visible ground-level patch window. The optional SRTM path is inactive without external data and currently parses a file before checking its tile cache. Applying the coarse erosion wrapper around DEM samples also destroys the local resolution that makes real elevation data valuable.

## Goals / Non-Goals

**Goals:**
- Preserve one terrain source for all consumers while making nearby non-pad terrain visibly varied.
- Let a native build opt into a local SRTM directory without requiring a repository-bundled global dataset.
- Retain original DEM samples for terrain geometry and collision whenever coverage exists.
- Avoid per-frame regeneration of unchanged terrain geometry.

**Non-Goals:**
- Downloading, redistributing, or committing multi-gigabyte DEM/bathymetry datasets.
- Replacing cube-sphere LOD, changing physical coordinates, or adding GPU erosion/triplanar shaders.
- Claiming procedural terrain is geographically accurate where an actual DEM is unavailable.
- Modeling overhangs, caves, or full global water/river simulation.

## Decisions

### Keep `TerrainSource` as the sole height authority

All changes compose wrappers beneath the existing source interface. This guarantees visual terrain, collision, pad placement, and map sampling agree. A separate high-detail renderer would look better temporarily but would violate contact and altitude authority.

### Add detail after coarse erosion and bypass erosion for raw DEM coverage

The coarse erosion field continues to give procedural Earth continent-scale drainage and macro shaping. A deterministic, seam-safe detail wrapper is applied after it so meter-to-hundreds-of-meters variation survives close-range sampling. When SRTM is configured, `DemTerrainSource` is used directly with its procedural fallback; it is not wrapped in the coarse erosion raster.

This is preferred to increasing erosion-grid resolution because global high-resolution erosion would block the main thread and require a much larger cache. It is preferred to changing the mesh generator because the current cube-sphere sampling already handles any shared source.

### Make site flattening a feathered pad-scale override

Each site retains a flat inner clearance radius for deterministic launches and landings, then blends to the base source over a defined transition band. KSC's existing 0.09-degree radius is reduced to a pad-scale footprint. A hard edge is rejected because it creates a collision and visual discontinuity.

### Configure SRTM through one optional local directory

The Earth source selection reads one documented application-level setting for the SRTM directory only in DEM-enabled builds. If it is absent, unreadable, or lacks a tile, the source remains usable through its deterministic fallback. This avoids asset-path scattering and means non-DEM builds preserve current behavior.

### Cache before loading and reuse valid geometry

`DemTerrainSource` first checks its LRU under the existing mutex, then loads only a cache miss and inserts under a second lock. Streaming only invokes geometry construction for patches that newly transition to generation; existing visible/cached geometry is reused until eviction. This is preferred to background loading for now because it removes known repeated work without introducing asynchronous asset lifetime complexity.

## Risks / Trade-offs

- [No supplied SRTM data] -> The result remains realistic procedural terrain rather than geographically accurate Earth relief; document the configuration and keep fallback explicit.
- [Detail aliases at low LOD] -> Bound local-detail frequency/amplitude to the smallest supported near-surface patch sample spacing and test neighboring samples.
- [Pad transition changes local contact height] -> Keep a flat inner footprint and use a smooth blend with deterministic source sampling; test the boundary and launch placement.
- [Synchronous cache miss stalls] -> Cache-first loading removes repeated parsing; defer background prefetching until profiling shows single-tile loads are a practical bottleneck.
- [Terrain texture/mesh budget growth] -> Reuse stable patches and retain current manager budget; measure before increasing active-window size or texture resolution.

## Migration Plan

1. Keep default builds procedural and preserve all current source consumers.
2. Add documented optional SRTM directory selection for `--features dem` builds.
3. Validate procedural and DEM-feature unit suites, then smoke-test rocket mode with and without a populated local SRTM directory.
4. Roll back by clearing the setting or building without `dem`; no persisted simulation state changes.
