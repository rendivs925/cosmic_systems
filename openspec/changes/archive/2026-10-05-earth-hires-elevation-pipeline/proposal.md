## Why

Earth's terrain authority is a 2048-per-face CSDEM resampled from 1 arc-minute
ETOPO1 (~4.9 km effective ground sample distance). Cube-sphere LOD 12-14 patches
bilinearly upsample that raster, so real sub-10 km relief is absent and the
surface reads as smooth at launch and landing scale. A world-class flight
simulator needs measured high-resolution relief near the vehicle without
materializing a full planet or letting terrain I/O block collision.

## What Changes

- Add a versioned, offline-built, cube-sphere **elevation tile-payload
  pyramid** with explicit provenance, license, datum, coverage, resolution, and
  per-tile minimum/maximum elevation and geometric error.
- Add a **tile-backed terrain source** that composes a resident
  moderate-resolution global base with streamed high-resolution regional tiles
  behind the existing `TerrainSource` boundary.
- Wire elevation tile requests, worker-task decoding, bounded residency, and
  eviction into the existing terrain streaming lifecycle; tile work never
  delays geometry publication and never blocks fixed-step collision.
- Drive terrain LOD from payload per-tile geometric error, removing the
  level-8 metadata ceiling that deeper patches currently inherit.
- Activate the reviewed local measured DEM (KSC) through the existing local
  elevation package overlay once its vertical datum is resolved.
- Extend existing terrain streaming telemetry with elevation tile residency,
  load backlog, and fallback rate.
- **BREAKING**: none. Native builds without the payload keep the current
  resident CSDEM plus procedural detail; browser (`wasm`, no `dem`) keeps the
  existing procedural source unchanged.

## Capabilities

### New Capabilities
- `elevation-tile-payload`: A versioned, offline-prepared cube-sphere elevation
  pyramid that supplies per-tile height payloads, coverage, and geometric error
  to the terrain authority without runtime downloads.

### Modified Capabilities
- `terrain-source`: The terrain authority gains a tile-backed composition that
  preserves a resident coarse fallback and guarantees I/O-free height sampling.
- `terrain-lod`: LOD selection consumes per-tile geometric error from the
  elevation payload instead of a globally conservative envelope.

## Impact

- `src/domain/services/dem_terrain_source.rs`, `local_elevation.rs`,
  `terrain_source/` (new tiled source and catalog wiring), `cube_sphere/lod.rs`
  error plumbing.
- `src/infrastructure/bevy_adapters/terrain/streaming.rs` and
  `streaming/metrics.rs` reuse the existing request, worker, cache, budget, and
  telemetry owners.
- New offline converter binary under `src/bin/`, a manifest under
  `assets/configs/terrain/`, provenance under `docs/datasets/`, and ignored
  local data under `assets/large_files/terrain/`.
- No change to rocket physics, reference frames, simulation time, or the
  authoritative `TerrainSource` contract that collision and altitude consume.
- No new runtime dependency and no runtime network access.
