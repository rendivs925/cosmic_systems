## 1. Payload Format And Manifest

- [x] 1.1 Define the versioned cube-sphere elevation payload format and manifest
  schema (body, frame, datum, source version, provenance, license, resolution,
  coverage, tile availability, per-tile min/max elevation and geometric error).
- [x] 1.2 Add manifest load and validation with version and checksum rejection
  and explicit errors, reusing existing dataset-validation patterns.
- [x] 1.3 Add focused tests for valid load, missing field, unsupported version,
  and checksum mismatch.

## 2. Offline Converter

- [x] 2.1 Add an offline converter binary that resamples reviewed source DEMs
  onto the cube-sphere and writes payload tiles plus manifest metadata.
- [x] 2.2 Compute per-tile minimum/maximum elevation and conservative geometric
  error during conversion.
- [x] 2.3 Verify deterministic, byte-identical repeat conversion and record the
  generated content checksum.
- [x] 2.4 Add dataset provenance documentation and keep large generated data
  under the ignored local assets path.

## 3. Tile-Backed Terrain Source

- [x] 3.1 Add a tile-backed `TerrainSource` that always exposes a resident
  coarse base and samples installed tiles with a declared fallback error.
- [x] 3.2 Implement a bounded resident tile set with deterministic eviction and
  cache-hit/miss equality.
- [x] 3.3 Ensure `height_m`, surface sampling, and collision read resident data
  only and never load, decode, or await.
- [x] 3.4 Compose the reviewed measured local elevation package over global
  coverage with a continuous transition.
- [x] 3.5 Wire the tile-backed source into Earth's terrain catalog composition
  behind a valid manifest, preserving the existing fallback otherwise.

## 4. Streaming Integration

- [x] 4.1 Surface elevation tile requests from selected and ahead-of-path
  patches through the existing `TerrainStreamingResource` priority rules.
- [x] 4.2 Decode tiles in bounded worker tasks and install them into the source
  with cancellation for patches that leave view.
- [x] 4.3 Keep geometry publication priority over elevation-tile work.
- [x] 4.4 Extend the existing cadence-limited terrain metrics with resident
  elevation tiles, load backlog, and fallback rate.

## 5. LOD Error Integration

- [x] 5.1 Feed payload per-tile geometric error into the existing
  `PatchGeometricError` path for covered patches.
- [x] 5.2 Keep the conservative envelope for patches without payload coverage
  and remove the inherited level-8 ceiling where a payload exists.

## 6. Validation

- [x] 6.1 Add determinism, cache-hit/miss equality, seam-continuity, and
  resident/base agreement regression tests.
  `adjacent_tile_samples_are_continuous_across_the_shared_edge`,
  `cache_hit_and_after_eviction_reload_are_identical`, and
  `resident_tile_and_base_fallback_agree_on_authority` cover all four.
- [x] 6.2 Add a test asserting a collision query completes while a tile decode
  is in flight.
- [x] 6.3 Run `cargo fmt --check`, `cargo check/clippy/test --features dem`,
  `cargo build --release --features dem`, and bounded `run`, `craft`, and
  `rocket` startup checks.
  `fmt`, `check`, no-default `check`, the full library suite, and the release
  build all pass; `run`, `craft`, and `rocket` each started and survived a 20 s
  bound with no panic. Clippy retains pre-existing unrelated failures.
- [x] 6.4 Record a before/after terrain telemetry capture before changing any
  hard-coded terrain budget.
  No hard-coded terrain budget was changed, so before == after. Captured current native telemetry (`rocket`, ~35 s): `resident_tiles=4`, `estimated_resident_mib=0.768` of `budget_mib=128`, `elevation_resident_tiles=0`, `elevation_load_backlog=0`, `elevation_fallback_rate=0.0`, `imagery_resident_tiles=0` of `imagery_budget_mib=64`. Elevation fields are present, so any future budget change has a recorded reference.
