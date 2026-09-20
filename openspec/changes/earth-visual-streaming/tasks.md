## 1. Focused Local Terrain

- [x] 1.1 Measure the current prelaunch target LOD distribution and confirm the
  visible focus cannot reach the configured local-surface threshold under the
  existing leaf budget.
- [x] 1.2 Extend the existing quadtree selection to reserve bounded local-detail
  capacity around the prelaunch launch direction or active camera intersection,
  while retaining balanced neighbors and coarse viewport fallback.
- [x] 1.3 Preserve existing visible-first task cancellation, cache protection,
  and geometry-first publication for focused refinement.
- [x] 1.4 Add focused prelaunch and moving-camera regression tests covering
  local-detail reachability, parent fallback, balanced leaves, and no stale task
  publication. Prelaunch reachability and budget are covered by
  `papua_prelaunch_selects_and_requests_scatter_lods`; balance, complete cover,
  determinism, and visible fallback by
  `localized_refinement_charges_balance_and_fallback_costs`; moving-camera
  coarsening by `generated_detail_can_coarsen_after_leaving_the_viewport`;
  cancellation by `stale_requested_patches_are_removed_before_generation`; and
  no-stale-publication by `published_geometry_is_retained_when_its_stitch_pattern_changes`.

## 2. Earth Imagery Package

- [ ] 2.1 Select and review a redistributable global Earth imagery source and
  a bounded high-detail launch/flight region; record license, body-fixed datum,
  coverage, resolution, source checksums, and expected visual error.
  Source and region are selected and documented: NASA Blue Marble: Next
  Generation (public domain) for the global overview and Copernicus Sentinel-2
  L2A 10 m for the bounded Papua region. License, datum, coverage, and
  resolution are recorded. Source checksums and expected visual error remain
  pending actual acquisition.
- [x] 2.2 Add a versioned imagery manifest and provenance document under the
  existing terrain asset/document paths, with ignored local-package locations.
  Added `assets/configs/terrain/earth_imagery_v1.ron` and
  `docs/datasets/earth_imagery_v1.md`. The package path is already ignored by
  the existing `assets/large_files/*` rule.
- [x] 2.3 Implement and test an offline converter that maps source pixels to
  cube-sphere imagery tiles using explicit pixel-center, antimeridian, polar,
  and cube-edge rules. Added `domain/services/imagery_tiles.rs` (pixel-center
  bilinear sampling with longitude wrap and polar clamp, tile rendering,
  region tile selection, deterministic output) and the `imagery_convert` binary.
  Eight tests cover pixel-center mapping, antimeridian wrap, polar clamp,
  adjacent-edge continuity, region coverage/bounds, and determinism; the binary
  was exercised end to end on a synthetic equirectangular image.
- [ ] 2.4 Produce and verify the local imagery package without adding runtime
  downloads or committing large generated assets. Blocked on data acquisition
  and a region/max-level decision: a full 10 m package over the nominal region
  to level 14 is roughly 800 MB of tiles, so production must either cap the
  imagery level or shrink the region.

## 3. Progressive Terrain Imagery

- [ ] 3.1 Add the smallest body-scoped imagery presentation contract needed to
  locate the best available Earth tile for an existing `TerrainPatch`.
- [ ] 3.2 Extend the existing terrain streaming resource to request, cancel,
  retain, and evict imagery with the same visible-first priorities as geometry.
- [ ] 3.3 Extend terrain render state and materials to replace global albedo
  only after detailed imagery is ready, while preserving global fallback and
  geometry-first publication.
- [ ] 3.4 Keep imagery payloads presentation-only and release their Bevy asset
  handles with the existing terrain patch eviction path.

## 4. Budgets And Regression Coverage

- [ ] 4.1 Define explicit imagery CPU/GPU residency, in-flight, and per-frame
  upload budgets with geometry work retaining priority.
- [ ] 4.2 Add cadence-limited imagery residency, pending, eviction, and upload
  backlog fields to the existing terrain streaming metrics.
- [ ] 4.3 Add focused tests for manifest validation, cube-face mapping,
  antimeridian/polar behavior, seam continuity, fallback retention, budgeted
  eviction, and source/collision independence.
- [ ] 4.4 Add render lifecycle tests proving that detailed imagery upgrades a
  patch without recreating terrain geometry or leaving a blank material.

## 5. Validation And Acceptance

- [ ] 5.1 Run `cargo fmt --check`, `cargo check --features dem`, `cargo clippy
  --features dem -- -D warnings`, `cargo test --features dem`, and `cargo build
  --release --features dem`.
- [ ] 5.2 Run bounded normal, craft, and rocket starts with the `dem` feature
  and confirm absent imagery preserves the existing global-albedo fallback.
- [ ] 5.3 Capture a native-display 1x Earth flight-camera run with existing
  performance and terrain telemetry enabled; compare frame percentiles,
  geometry work, and imagery backlog against the pre-change baseline.
- [ ] 5.4 Visually inspect globe-to-ground imagery refinement on a usable
  display for continuous fallback, cube-face seams, blank patches, and stable
  camera-relative presentation.
