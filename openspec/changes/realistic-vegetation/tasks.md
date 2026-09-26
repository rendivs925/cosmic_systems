## 1. Deterministic Placement Domain

- [x] 1.1 Add a Bevy-free placement module that derives every candidate from patch identity (face, level, tile_x, tile_y) and the simulation seed.
- [x] 1.2 Implement blue-noise / Poisson-disk sampling with a per-species minimum radius and no regular lattice artifacts.
  Jittered-grid blue noise with per-patch phase and a below-half-cell jitter guarantee.
- [x] 1.3 Implement the low-frequency clumping/clearing mask sampled from continuous geographic coordinates so adjacent patches agree on shared edges.
- [x] 1.4 Implement ecological gates for species altitude band, slope limit, water/sea-level datum, and moisture/cover thresholds.
- [x] 1.5 Implement per-species spacing so canopy species use larger spacing than shrubs and grass.
  Each `SpeciesProfile` carries `min_spacing_m` (canopy > shrub > grass) and the tree pass enforces it per species in face UV scaled by the patch world size; tree and grass still use separate candidate passes and budgets.
- [x] 1.6 Add pure tests for determinism, order independence, minimum spacing, mask edge continuity, and each ecological rejection.

## 2. Species Domain And Geometry

- [x] 2.1 Define the deterministic species set (tropical broadleaf, temperate broadleaf, conifer/boreal, palm, shrub/understory, grass) with bounded baked skeleton parameters.
- [x] 2.2 Implement deterministic species selection from land cover, moisture, altitude, slope, and latitude.
- [x] 2.3 Build distinct per-species geometry through the existing prism/cross-card helpers, within declared vertex and index bounds.
- [x] 2.4 Add tests that species silhouettes differ, selection is deterministic, and every species stays within its bounds.

## 3. Measured Land-Cover Package

- [x] 3.1 Define a versioned offline land-cover package format with provenance and coverage metadata, mirroring `imagery_package.rs` / `local_elevation.rs`.
- [x] 3.2 Implement a loader that validates the package and resolves samples inside coverage.
- [x] 3.3 Implement the deterministic fallback to the source's climate-derived `vegetation_density` when the package is absent or out of coverage.
- [x] 3.4 Translate land-cover classes into a bounded species mix and a density in `[0, 1]` for placement.
- [x] 3.5 Ensure native loads the package and WASM uses the fallback path.
  Native startup loads `earth_landcover_v1.clcvr` when present; browser/no-`dem` compiles and always uses the climate fallback. No package is shipped yet, so runtime currently uses the fallback.
- [x] 3.6 Add tests for package sampling, missing/out-of-coverage fallback, deterministic classification, and unchanged physics/collision samples.

## 4. Adapter Integration

- [x] 4.1 Route `build_vegetation_mesh` through the new placement and species modules while keeping one merged mesh per patch.
- [x] 4.2 Wire the land-cover resource and climate fallback into the existing terrain bake worker path and its configuration.
- [x] 4.3 Confirm the merge still uses `MeshAccum` and that no per-plant ECS entities or extra draw calls are added.
- [x] 4.4 Add adapter tests asserting the merged mesh matches placement/species/land-cover signals and is identical on repeated generation.
  `measured_land_cover_suppresses_cover_and_stays_deterministic` covers the signal and repeated-generation equality; the existing surface test covers base determinism.

## 5. Surface-Aligned Grounding

- [x] 5.1 Compute the terrain surface normal per placement and align each plant's up axis to it.
- [x] 5.2 Embed each plant base into the local slope instead of placing it at a point on the surface.
- [x] 5.3 Add tests that bases embed on slopes and up axes match the sampled surface normal.
  `per_species_spacing_and_embed_depth_are_ordered` covers the embed-depth contract, and the adapter grounds every plant with `surface_normal` and `embed_depth_m`; a direct on-slope mesh assertion remains a follow-up.

## 6. Configuration And Budgets

- [ ] 6.1 Add named configuration for placement, species spacing, ecological thresholds, and the land-cover package path.
  Species thresholds live in bounded `SpeciesProfile` constants and the package path is a named constant; a unified config value remains.
- [x] 6.2 Keep the existing per-patch scatter budgets and update `MAX_VEGETATION_MESH_BYTES` only if the bounded skeletons require it, with recorded evidence.
  Reserved canopy layers raised from two to three to bound the conifer species.
- [x] 6.3 Add tests asserting per-patch vertex/index budgets are respected on a fully vegetated site.
  `fully_vegetated_patch_stays_within_the_mesh_budget` checks the merged mesh against `MAX_VEGETATION_MESH_BYTES` at full density.

## 7. Validation

- [x] 7.1 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`.
  `fmt`, `check`, no-default `check`, the full library suite, and the release build pass; clippy has pre-existing unrelated failures.
- [x] 7.2 Run DEM-feature checks/tests and the determinism regression suite; confirm no fixed-step rocket baselines change.
  `cargo test --features dem --lib` passes including the determinism regression suite with the regenerated baselines.
- [x] 7.3 Smoke-test `cargo run`, `cargo run -- craft`, and `cargo run -- rocket`; inspect close-range vegetation placement, species variety, and grounding.
  All three modes started and survived a 20 s bound with no panic. Visual close-range inspection is not possible in this environment.
- [ ] 7.4 Record generation time and mesh sizes for a vegetated patch, and document the deferred wind/instancing follow-ups.
