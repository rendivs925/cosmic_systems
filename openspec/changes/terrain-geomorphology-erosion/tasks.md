## 1. Baseline and Audit

- [x] 1.1 Identify the single planet terrain composition site that builds `LayeredTerrainSource` and record where `ErodedTerrainSource` must wrap an elevation layer.
- [x] 1.2 Record the current `height_m`, `mesh_height_m`, `moisture`, and `river_strength` values at representative coordinates (including at least one tile edge and one pole-adjacent tile) as pre-change baselines.
- [x] 1.3 Record cold-start/bake time and resident erosion-cache memory for the current default config as the performance baseline.
  `default_config_bake_and_cache_memory_baseline` (prints telemetry): resolution=64, 49,152 bytes/tile (48 KiB), `cache_max_tiles=64` → 3,145,728-byte (3.0 MiB) ceiling; first-tile cold bake ≈35.9 ms in a debug build.
- [x] 1.4 Confirm the erosion module exposes everything needed (`ErodedTerrainSource`, `ErosionConfig`, `HeightRaster` height/flow/moisture) and note any missing accessor before editing.

## 2. Determinism and Seam Regression Tests

- [x] 2.1 Add a regression test that erodes a tile twice with the same seed and asserts identical height, flow, and moisture channels.
- [x] 2.2 Add a regression test that regenerates an evicted tile and asserts height, moisture, and river strength reproduce exactly (cache-history independence).
- [x] 2.3 Add a regression test that two adjacent independently-eroded tiles agree at their shared boundary within the configured feather (seam safety).
- [x] 2.4 Add a regression test that equivalent coordinates (longitude wrap and pole reflection) resolve to the same tile and identical channel values.
- [x] 2.5 Add a regression test that the resident tile count never exceeds `cache_max_tiles` and eviction follows deterministic recency order.

## 3. Compose Erosion into the Terrain Authority

- [x] 3.1 Wrap the planet's elevation layer source with `ErodedTerrainSource` at the single composition site using a declared, validated `ErosionConfig` and seed; do not create a second elevation path.
- [x] 3.2 Ensure the composed authority's `height_m`, `surface_sample`, `moisture`, and `river_strength` all resolve through the eroded field, preserving layer composition and existing biome composition.
- [x] 3.3 Add a test asserting the composed authority actually consumes the eroded field (height differs from the analytic source in an eroded region).
- [x] 3.4 Add a test asserting `ErosionConfig::validate` is invoked at construction and rejects non-physical parameters.

## 4. Erosion-Consistent, Seam-Safe Mesh Height

- [x] 4.1 Change `mesh_height_m` on the erosion source to sample the same eroded field as `height_m` at the patch's own level; remove the analytic-base bypass.
- [x] 4.2 Verify the erosion field remains LOD-independent and deterministic so shared-edge samples are identical across adjacent patches and levels.
- [x] 4.3 Add a test asserting collision (`height_m`) and render (`mesh_height_m`) heights agree at the same location and level.
- [x] 4.4 Add a test asserting two adjacent patches compute identical mesh heights at a shared boundary sample.
- [x] 4.5 Verify existing LOD crack/stitch behavior still holds and update any baseline that legitimately changed.

## 5. Offline Bake with Runtime Cache Fallback

- [x] 5.1 Define the offline-baked elevation/hydrology payload shape for a tile (height, flow, moisture) and where it is produced and loaded.
- [x] 5.2 Implement loading a baked tile when available, falling back to the existing bounded runtime bake when it is not.
- [x] 5.3 Add a test asserting a baked tile and a runtime-baked tile return identical height, flow, moisture, and river strength.
- [x] 5.4 Confirm runtime baking remains a cached/static field only (no per-frame erosion) and that cache size stays bounded.

## 6. Hydrology Signals for River and Wet-Biome Presentation

- [x] 6.1 Ensure surface map, scatter, and overview consumers read `moisture` and `river_strength` from the composed authority rather than any un-eroded path.
- [x] 6.2 Add a test asserting river-channel strength is bounded `[0, 1]` and that a carved channel reads as higher moisture/strength than its surroundings.
- [x] 6.3 Verify river appearance and wet-biome material selection follow the authoritative hydrology signals.

## 7. Validation and Performance Evidence

- [x] 7.1 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`; resolve all failures.
  `fmt`, `check`, no-default `check`, and the full library suite pass; `cargo clippy --lib` is now warning-free. `cargo clippy --all-targets` retains five pre-existing, unrelated warnings (`erosion/simulate.rs:96`, `rocket/presentation.rs:83`, `terrain/mips.rs:164/195`, `rocket/contact/mod.rs:515`) documented rather than fixed in this change.
- [x] 7.2 Run each application mode (`cargo run`, `cargo run -- craft`, `cargo run -- rocket`) and confirm terrain, collision, and modes remain functional; report any mode that cannot run in this environment and why.
- [x] 7.3 Compare cold-start/bake time and resident memory against task 1.3; raise `cache_max_tiles` only if profiling justifies it and record the evidence.
  Default config unchanged: per-tile bake ≈36 ms and the 64-tile ceiling is only 3.0 MiB, so no `cache_max_tiles` increase is warranted. Revisit only if a larger tile set or coarser `tile_deg` raises per-tile bytes.
- [x] 7.4 Confirm no duplicate erosion implementation and no new global resource/coordinate system were introduced; update `openspec/specs` docs only via the delta specs.
