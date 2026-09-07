## 1. Earth Imagery Package

- [ ] 1.1 Select and review a redistributable global Earth imagery source and
  a bounded high-detail launch/flight region; record license, body-fixed datum,
  coverage, resolution, source checksums, and expected visual error.
- [ ] 1.2 Add a versioned imagery manifest and provenance document under the
  existing terrain asset/document paths, with ignored local-package locations.
- [ ] 1.3 Implement and test an offline converter that maps source pixels to
  cube-sphere imagery tiles using explicit pixel-center, antimeridian, polar,
  and cube-edge rules.
- [ ] 1.4 Produce and verify the local imagery package without adding runtime
  downloads or committing large generated assets.

## 2. Progressive Terrain Imagery

- [ ] 2.1 Add the smallest body-scoped imagery presentation contract needed to
  locate the best available Earth tile for an existing `TerrainPatch`.
- [ ] 2.2 Extend the existing terrain streaming resource to request, cancel,
  retain, and evict imagery with the same visible-first priorities as geometry.
- [ ] 2.3 Extend terrain render state and materials to replace global albedo
  only after detailed imagery is ready, while preserving global fallback and
  geometry-first publication.
- [ ] 2.4 Keep imagery payloads presentation-only and release their Bevy asset
  handles with the existing terrain patch eviction path.

## 3. Budgets And Regression Coverage

- [ ] 3.1 Define explicit imagery CPU/GPU residency, in-flight, and per-frame
  upload budgets with geometry work retaining priority.
- [ ] 3.2 Add cadence-limited imagery residency, pending, eviction, and upload
  backlog fields to the existing terrain streaming metrics.
- [ ] 3.3 Add focused tests for manifest validation, cube-face mapping,
  antimeridian/polar behavior, seam continuity, fallback retention, budgeted
  eviction, and source/collision independence.
- [ ] 3.4 Add render lifecycle tests proving that detailed imagery upgrades a
  patch without recreating terrain geometry or leaving a blank material.

## 4. Validation And Acceptance

- [ ] 4.1 Run `cargo fmt --check`, `cargo check --features dem`, `cargo clippy
  --features dem -- -D warnings`, `cargo test --features dem`, and `cargo build
  --release --features dem`.
- [ ] 4.2 Run bounded normal, craft, and rocket starts with the `dem` feature
  and confirm absent imagery preserves the existing global-albedo fallback.
- [ ] 4.3 Capture a native-display 1x Earth flight-camera run with existing
  performance and terrain telemetry enabled; compare frame percentiles,
  geometry work, and imagery backlog against the pre-change baseline.
- [ ] 4.4 Visually inspect globe-to-ground imagery refinement on a usable
  display for continuous fallback, cube-face seams, blank patches, and stable
  camera-relative presentation.
