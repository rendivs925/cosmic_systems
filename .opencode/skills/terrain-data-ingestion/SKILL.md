---
name: terrain-data-ingestion
description: Use when adding or changing SRTM, GeoTIFF, HGT, LRO LOLA, MOLA, heightmap, DEM cache, external terrain data, or procedural fallback behaviour in Cosmic Systems.
---

# Terrain Data Ingestion

Use this skill before extending real-world terrain data support. External data
must integrate with the existing deterministic terrain authority and streaming
budgets.

## Existing Authority

`src/domain/services/dem_terrain_source.rs` owns optional DEM support behind the
`dem` feature. It already provides:

- `DemDataset` for SRTM, LRO LOLA, and MOLA;
- stable `DemTileKey` identity;
- `DemTile` row-major meter-height buffers and geographic bounds;
- bounded `DemTileCache` LRU behaviour;
- `DemTerrainConfig` including dataset, data directory, cache limit, and
  procedural fallback;
- `DemTerrainSource` implementing the shared `TerrainSource` contract.

`terrain_source.rs` owns layering/source selection and `terrain_streaming.rs`
owns visible patch scheduling. Extend them rather than creating a data manager,
second terrain sample API, or independent cache.

## Data Contract

- Preserve `TerrainSource` as the authoritative sampling interface. Collision,
  mesh generation, and terrain visual data consume the same source contract.
- Store elevation in meters with explicit datum/reference assumptions documented.
- Declare dataset body, projection, geographic bounds, horizontal sampling,
  nodata convention, vertical datum, source/version/license, and expected error.
- Convert external coordinates explicitly into the existing body-fixed/geodetic
  convention. Never silently transpose axes, invert latitude rows, or wrap
  longitude differently from the project convention.
- Preserve deterministic interpolation, edge handling, nodata handling, and
  fallback selection independent of evaluation order.
- Procedural fallback is explicit configuration, not a hidden substitution for
  invalid data. Log/configure coverage failures appropriately without per-sample
  log spam.

## Streaming And IO

- Do not load/decode large DEM tiles, build pyramids, or resample datasets in a
  render-critical system.
- Integrate expensive external IO/decoding with the existing bounded terrain
  worker/scheduling path when it affects visible streaming.
- Keep returned task data plain Rust; Bevy asset/entity mutation stays on the
  main thread and is bounded.
- Cache by stable dataset/tile identity under explicit memory budgets and evict
  through existing LRU ownership. Do not create an unbounded per-dataset cache.
- Prioritize visible/near-camera data over prefetch and background coverage.
- Preserve a coarse/procedural fallback while detailed data is unavailable. Never
  stall the frame waiting for a tile.

## DEM Sampling Rules

- Use deterministic interpolation and test samples at interior, edge, corner,
  antimeridian, polar, nodata, and missing-tile cases.
- Ensure adjacent source tiles have continuous boundary behaviour within the
  dataset's documented precision.
- Ensure DEM and procedural layers use the same physical radius/frame and that
  sample heights are finite.
- Avoid holding a cache lock across unnecessary work. Do not replace the current
  cache with contention-prone locking without profiling evidence.
- Keep data loading feature-gated as the project does today and preserve builds
  without local DEM assets.

## Tests And Validation

Add tests next to `dem_terrain_source.rs` for:

- tile identity, lookup, deterministic interpolation, and LRU eviction;
- byte/order/endian handling for each supported format;
- geographic boundary, antimeridian, and nodata behaviour;
- procedural fallback and disabled-data behaviour;
- source/collision/render agreement through `TerrainSource`;
- bounded cache/task behaviour under repeated visible requests.

Run default and `dem` feature checks/tests when changing feature-gated code, then
run terrain streaming/rendering tests and full simulator validation. Do not
commit external proprietary datasets or unverified downloads to the repository.

Reject a separate height authority, synchronous frame-path file IO, ambiguous
datum/projection conversions, unbounded tile retention, random fallback terrain,
and a visual DEM path disconnected from collision sampling.
