## 1. Layer Model Foundations

- [x] 1.1 Define a data-driven ground-layer catalog (layer identity, weight ranges, tiling scale, PBR set source) owned by the terrain surface module, with a documented default bounded layer set.
- [x] 1.2 Add a pure layer-weight function deriving weights from authoritative elevation, slope, moisture, and latitude zone, reusing `slope_deg_at`/`overview_slope_deg`, `moisture`, and `zone_lat`.
- [x] 1.3 Add unit tests for weight continuity (no hard bands), normalized/bounded weights, slope-to-rock, snow-line, and deterministic regeneration.
- [x] 1.4 Centralize layered-material budget configuration with the existing terrain budget owners; do not change current resolution or per-patch map counts.

## 2. Per-Patch Layer Data

- [x] 2.1 Extend `PreparedPatchSurface`/`prepare_patch_surface` to carry per-patch layer weights generated on the streaming worker, reusing the existing `LOCAL_SURFACE_MIN_PATCH_LEVEL` detail ring.
- [x] 2.2 Extend the existing per-patch map generation path to emit the layer-weight map while retaining the current albedo/normal output as the fallback base.
- [x] 2.3 Keep coarse-LOD and browser paths on the existing neutral/single-layer output; add tests proving adjacent LOD shared-edge weights agree.
- [x] 2.4 Add a regression test asserting collision/altitude paths never read material layer data.

## 3. Shader And Material

- [x] 3.1 Extend `TerrainSurfaceExtension` bindings and `build_terrain_material` for the layer-weight map and shared layer PBR sets, preserving the single construction path.
- [x] 3.2 Implement weighted layer albedo, normal, and roughness blending in `assets/shaders/terrain_surface.wgsl` using the same weights for all three channels.
- [x] 3.3 Add orientation-blended triplanar projection for layer detail on steep faces, reusing the existing triplanar and tangent-reconstruction approach.
- [x] 3.4 Add macro albedo variation, a micro-detail normal/roughness overlay, and a near-camera high-frequency detail overlay, each faded per pixel by view distance.
- [x] 3.5 Preserve the existing global albedo/imagery base and the single-layer fallback path when the layered catalog is unavailable.

## 4. Presentation Wiring And Budgets

- [x] 4.1 Resolve shared layer PBR sets through the catalog (offline-prepared with provenance, or deterministic procedural generation), reusing `TerrainRenderAssets`.
- [x] 4.2 Wire layer-weight upload, material rebuild, budget telemetry, and patch eviction release in `render.rs` without bypassing the existing upload cap or cache lifecycle.
- [x] 4.3 Select the layered path versus single-layer fallback explicitly from build capability and catalog availability (native `dem` versus browser/no-`dem`).
- [x] 4.4 Confirm no new terrain source, coordinate system, floating origin, or duplicate material shader is introduced.

## 5. Validation

- [x] 5.1 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`; add layered-material and fallback tests as needed.
  `fmt`, `check`, no-default `check`, the full library suite, and the release
  build pass with 11 layer tests; clippy has pre-existing unrelated failures.
- [x] 5.2 Build and check both native `dem` and no-`dem` configurations; confirm the browser fallback compiles and selects the single-layer path.
- [x] 5.3 Verify `cargo run`, `cargo run -- craft`, and `cargo run -- rocket` still start, and terrain streaming/collision are unchanged.
  All three modes started and survived a 20 s bound with no panic or shader error.
- [ ] 5.4 Capture before/after native measurements for terrain material cost and residency; only then consider any budget change.
  No terrain budget was changed; measurement remains a follow-up.
- [ ] 5.5 Run `openspec validate terrain-layered-material --strict` and confirm it passes.
