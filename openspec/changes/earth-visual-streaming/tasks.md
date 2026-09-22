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

- [x] 2.1 Select and review a redistributable global Earth imagery source and
  a bounded high-detail launch/flight region; record license, body-fixed datum,
  coverage, resolution, source checksums, and expected visual error.
  NASA Blue Marble: Next Generation (public domain) provides the global overview
  and Copernicus Sentinel-2 L2A true colour provides the bounded Papua region.
  `docs/datasets/earth_imagery_v1.md` records the license, attribution, WGS 84
  body-fixed datum, coverage, resolution, acquisition, downloaded and runtime
  SHA-256 values, and the expected visual error (10 m imagery limits close-range
  detail; thin cirrus in scene 54LUR is left uncorrected).
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
- [x] 2.4 Produce and verify the local imagery package without adding runtime
  downloads or committing large generated assets. Four Sentinel-2B scenes from
  2026-08-22 (54LTR/54LUR/54MTS/54MUS) were reprojected with
  `scripts/imagery_reproject_utm.py` and converted to 5009 cube-sphere tiles
  (levels 8..=12, 256 px, 34 MB) under the ignored
  `assets/large_files/terrain/earth_imagery_v1/`. The level cap is 12 because
  cube-edge distortion makes this longitude need 3692 level-12 tiles; levels
  8..=10 would be too coarse and level 14 would be hundreds of megabytes. The
  runtime log confirms the package loads and streams (`Earth imagery available:
  5009 tiles, 1 regions`, 82-85 resident tiles, ~21 MiB of the 64 MiB budget,
  evictions advancing) with no errors, and an absent package still falls back to
  the global albedo.

## 3. Progressive Terrain Imagery

- [x] 3.1 Add the smallest body-scoped imagery presentation contract needed to
  locate the best available Earth tile for an existing `TerrainPatch`. Added
  `domain/services/imagery_package.rs`: `EarthImageryPackage::load` validates the
  manifest, requires the global overview, and indexes produced tiles once;
  `resolve(&TerrainPatch)` returns the most detailed produced tile at or coarser
  than the patch level inside a covering region, else the global overview.
  Tests cover missing/unverified packages, finest-available resolution, coarser
  fallback, and outside-region global fallback.
- [x] 3.2 Extend the existing terrain streaming resource to request, cancel,
  retain, and evict imagery with the same visible-first priorities as geometry.
  Added `terrain/imagery.rs`: `TerrainImageryResource` derives its desired tile
  set from `TerrainStreamingResource::published`, cancels pending loads for
  patches that leave view, and evicts handles for patches no longer desired.
- [x] 3.3 Extend terrain render state and materials to replace global albedo
  only after detailed imagery is ready, while preserving global fallback and
  geometry-first publication. `apply_terrain_imagery` rebuilds a patch material
  with the ready tile and `imagery_weight = 1.0`; the shader mixes the tile over
  the global overview by that weight. Geometry publication is unchanged.
- [x] 3.4 Keep imagery payloads presentation-only and release their Bevy asset
  handles with the existing terrain patch eviction path. Imagery handles live in
  `TerrainImageryResource` and the render state; both drop when a patch is
  evicted or despawned, and imagery never feeds height, collision, or physics.

## 4. Budgets And Regression Coverage

- [x] 4.1 Define explicit imagery CPU/GPU residency, in-flight, and per-frame
  upload budgets with geometry work retaining priority.
  `TerrainImageryConfig` sets `budget_bytes` (64 MiB) and
  `max_uploads_per_frame` (4); admission is skipped when the next tile would
  exceed the budget, and imagery streams after geometry in the frame.
- [x] 4.2 Add cadence-limited imagery residency, pending, eviction, and upload
  backlog fields to the existing terrain streaming metrics. `ImageryMetrics`
  is captured into `TerrainStreamingMetrics` and logged as
  `imagery_resident_tiles`, `imagery_pending_tiles`, `imagery_resident_mib`,
  `imagery_budget_mib`, and `imagery_evicted_tiles` on the existing cadence.
- [x] 4.3 Add focused tests for manifest validation, cube-face mapping,
  antimeridian/polar behavior, seam continuity, fallback retention, budgeted
  eviction, and source/collision independence. Covered by imagery manifest,
  tile, and package tests plus `imagery_admission_stays_within_budget`.
  Source/collision independence is structural: imagery lives only in
  presentation modules and no imagery type is referenced by `TerrainSource`,
  terrain collision, or rocket physics.
- [x] 4.4 Add render lifecycle tests proving that detailed imagery upgrades a
  patch without recreating terrain geometry or leaving a blank material.
  `ready_imagery_upgrades_the_material_without_recreating_geometry` builds a
  verified temporary package, spawns a resolved patch render state, and runs
  `apply_terrain_imagery`: it asserts the material is upgraded (non-default
  albedo, `imagery_weight == 1.0`), the mesh and surface handles are unchanged,
  and a second frame is a no-op.

## 5. Validation And Acceptance

- [ ] 5.1 Run `cargo fmt --check`, `cargo check --features dem`, `cargo clippy
  --features dem -- -D warnings`, `cargo test --features dem`, and `cargo build
  --release --features dem`. All ran clean (713 dem and 686 no-default lib tests,
  30/30 strict OpenSpec) except `cargo clippy -- -D warnings`, which the user
  intentionally skipped; run it before final acceptance if warnings are wanted.
- [x] 5.2 Run bounded normal, craft, and rocket starts with the `dem` feature
  and confirm absent imagery preserves the existing global-albedo fallback.
  Normal, craft, and rocket each stayed alive for 18 s under Xvfb with zero
  errors or panics. Imagery is rocket-terrain-only: normal and craft never load
  it. A rocket run with the package moved away logged the global-albedo fallback
  and stayed bounded, so a missing package is not a terrain or collision error.
- [ ] 5.3 Capture a native-display 1x Earth flight-camera run with existing
  performance and terrain telemetry enabled; compare frame percentiles,
  geometry work, and imagery backlog against the pre-change baseline.
  Blocked by the environment. A GPU-backed Xvfb ascent (Space launch, chase
  camera, `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`) reached fairing separation at
  110 km with p50 ~52-54 ms, p95 ~120 ms, and p99 ~133 ms, and the imagery
  backlog drained from 85 resident tiles to 0 as the region was left behind
  (191 evictions, resident ceiling 21.25 MiB of 64 MiB). On the real `:0`
  display the rocket window is destroyed externally after tens of seconds with a
  clean exit (code 0) and no error, panic, OOM, or imagery correlation (normal
  mode survives), so a native-display capture comparable to the documented
  pre-change baseline (p50 82.9 ms, p95 103.1 ms, p99 179.3 ms) is still
  pending.
- [ ] 5.4 Visually inspect globe-to-ground imagery refinement on a usable
  display for continuous fallback, cube-face seams, blank patches, and stable
  camera-relative presentation.
  Blocked by the same display instability. Xvfb frames confirm continuous
  fallback and no blank patches at Earth's limb, but the near-ground chase and
  peripheral camera views are dominated by atmospheric haze and pre-existing
  patch-level shading banding, so detailed imagery refinement cannot be judged
  from them; inspection needs a usable display.
