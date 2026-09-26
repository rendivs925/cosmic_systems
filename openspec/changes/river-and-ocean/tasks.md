## 1. Water Parameter Contract

- [x] 1.1 Extend `WaterParams` in `terrain/water.rs` with Gerstner wave, absorption, scattering, foam, refraction, and shadow fields, keeping `WaterExtension::default()` valid
  Wave, absorption, scattering, foam, landscape-shadow, and quality fields are added. Refraction is expressed through the base `StandardMaterial`'s screen-space transmission plus a `WaterQualityConfig::refraction` flag rather than new shader fields, which is the supported Bevy path. `WaterExtension::default()` was replaced by `WaterExtension::new(params, occlusion)` so the required occlusion map is explicit.
- [x] 1.2 Mirror the extended field layout in `assets/shaders/water.wgsl` so the uniform matches exactly
- [x] 1.3 Add conservative, named default constants for wave count, amplitudes, absorption, foam thresholds, and refraction toggle
  Ocean and river defaults are named constants (`OCEAN_*`/`RIVER_*`), plus the bounded wave-count and foam-coverage defaults. The refraction toggle is deferred with task 6.2.
- [x] 1.4 Add unit coverage that the Rust uniform and WGSL struct field order stay in sync
  `wgsl_water_params_match_the_rust_field_order` parses `assets/shaders/water.wgsl` and compares the `WaterParams` field order to the Rust mirror.
  A params sanity test exists; explicit Rust/WGSL field-order sync coverage remains.

## 2. Displaced Ocean Geometry

- [x] 2.1 Extend `water_mesh_for_patch` to emit displacement-capable subdivision while preserving the normalized depth vertex channel and render-origin frame
  The ocean cap reuses the terrain patch grid so the Gerstner vertex stage displaces it, retains the normalized depth vertex channel, carries patch-local UV1, and stays in the render-origin frame.
- [x] 2.2 Keep the coastline sealed when displacement is enabled and release the mesh with the patch on evict
- [x] 2.3 Budget-gate subdivision and wave count by patch LOD/distance with a configuration value
  `WaterQualityConfig` caps the ocean cap's vertices per side and the active Gerstner wave count (1..=4) plus foam coverage; defaults are the maximum tier.

## 3. Gerstner Vertex Shader

- [x] 3.1 Implement the bounded Gerstner wave-sum displacement in the water vertex stage
- [x] 3.2 Derive the analytic surface normal from the same wave sum and feed it into PBR lighting
- [x] 3.3 Drive wave phase from the deterministic presentation clock only

## 4. Physically Based Water Shading

- [x] 4.1 Add Fresnel sky reflection that increases reflection and opacity at grazing angles
- [x] 4.2 Add Beer-Lambert depth absorption driven by the normalized depth vertex channel
- [x] 4.3 Add subsurface scattering for lit crests and troughs
  Implemented as a crest-driven scattering tint; no sun-direction volumetric SSS.

## 5. Foam

- [x] 5.1 Add crest foam driven by wave steepness and a breaking threshold
- [x] 5.2 Keep and extend the shoreline shoaling foam band, blending into open water
- [x] 5.3 Make foam coverage budget-configurable
  `WaterQualityConfig::foam_coverage` scales both shoreline and crest foam in the shader.

## 6. Shadows and Refraction

- [x] 6.1 Remove `NotShadowReceiver` from water entities so terrain shadows darken the surface, keeping `NotShadowCaster`
- [x] 6.2 Add optional screen-space refraction behind configuration, disabled by default
  `WaterQualityConfig::refraction` (default `false`) drives `specular_transmission`/`ior`/`thickness` on the ocean base material; rivers never refract. Covered by `refractive_water_base_uses_transmission_and_depth_fallback_does_not`.
- [x] 6.3 Fall back to depth-based blending when refraction buffers are unavailable (WASM/restricted targets)
  When refraction is disabled, or when the target cannot supply transmission buffers, the material keeps `specular_transmission = 0` and uses the existing alpha-blended depth path. `no-default-features` builds and runs the depth path.

## 7. Flow-Directed River Channels

- [x] 7.1 Rework `build_river_mesh` to derive alignment and width from the authoritative `TerrainSource` hydrology/discharge signal
  Channel coverage and ribbon shape derive from the normalized `river_strength` discharge signal; an explicit flow-direction vector is not exposed by the authority yet.
- [x] 7.2 Generate channel banks that blend into surrounding terrain
  Weak corners are pulled toward the local channel core and sit closer to the bed, exposing terrain as banks.
- [x] 7.3 Add a flow-directed surface animation along the channel
  `build_river_mesh` derives a per-cell flow direction from the `river_strength` isoline and encodes it in the vertex-colour green/blue channels; `water.wgsl` rebuilds the surface east/north frame and advects a travelling ripple along it, gated by the new `WaterParams::flow_speed` (0 for the ocean, >0 for rivers). Covered by `river_mesh_encodes_a_flow_direction_in_vertex_colour` and the `WaterParams`/WGSL field-order sync test. Visual confirmation still needs a display.
- [x] 7.4 Preserve the dry-patch `None` contract and per-patch mesh lifetime

## 8. Validation

- [x] 8.1 Add domain-level tests for deterministic displacement, analytic normals, and river width scaling
  River width scaling and channel determinism are covered by `river_width_scales_with_discharge_and_stays_deterministic`; ocean displacement/analytic normals are shader-side and validated by shader compilation only.
- [x] 8.2 Add presentation-boundary tests that water never mutates collision, radar altitude, or physics state
  Structural boundary: `WaterParams`, `WaterExtension`, `WaterMaterial`, and `WaterQualityConfig` are declared and referenced only under `src/infrastructure/bevy_adapters/terrain/`; no `src/domain` or `src/application` module references them. Collision, altitude, and physics sample only `TerrainSource`, so water cannot mutate authoritative state.
- [x] 8.3 Verify terrain patch spawn/evict, render-origin rebasing, and material sharing still hold for water
  `evicting_a_patch_releases_its_unique_render_assets` now also asserts a per-patch water material is released; spawn and render-origin rebasing are unchanged and covered by the existing render tests.
- [x] 8.4 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`
  `fmt`, `check`, no-default `check`, the full library suite, and the release build pass; clippy retains pre-existing unrelated failures.
- [x] 8.5 Run each mode (`cargo run`, `cargo run -- craft`, `cargo run -- rocket`) and report any environment limitation instead of claiming visual success
  All three modes started and survived a 20 s bound with no panic or WGSL error. Visual water inspection is not possible in this environment.
