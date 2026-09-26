## 1. Baked Terrain Self-Shadow

- [x] 1.1 Identify the existing build-time Sun direction convention used by
  `SunLight` and the fixed-sun shadow path, and derive the body-fixed Sun
  direction for a terrain bake from the shared ephemeris state and the patch's
  `body_to_inertial` rotation.

  `body_fixed_sun_direction` derives the body-fixed unit direction from
  `EphemerisSnapshot::solar_inertial_relative_state(SUN, planet)` and
  `body_fixed_to_planet_inertial_rotation`, matching `environment.rs`.
- [x] 1.2 Add a pure, Bevy-free heightfield ray-march that samples the
  authoritative terrain height field toward the recorded Sun direction and
  returns a bounded self-shadow visibility term, with unit tests for
  unobstructed, fully occluded, and grazing cases.

  `ray_visibility` (with `smoothstep01`) marches quadratically spaced samples
  against `TerrainSource::height_m`. Tests:
  `flat_terrain_is_unoccluded_and_a_ridge_blocks_the_sun_ray` and
  `grazing_sun_ray_produces_a_partial_soft_shadow`.
- [x] 1.3 Bake the self-shadow term per terrain patch in the same body-fixed
  frame as its mesh, attach it to the existing patch render state, and release
  it with the patch on eviction.

  `bake_terrain_occlusion` writes an R8G8 patch-local map (red self-shadow,
  green sky occlusion); the handle lives in `TerrainPatchRenderState` and
  `release_patch_render_assets` removes owned maps.
- [x] 1.4 Record the Sun direction used by each bake and refresh a resident
  patch only when its geometry is regenerated or the Sun direction changes
  beyond a configured tolerance.

  `baked_sun_inertial` is recorded in the render state; `refresh_terrain_occlusion`
  rebakes only when the inertial direction moves past `refresh_tolerance_rad`,
  bounded by `max_refreshes_per_frame`. Rotation alone never rebakes.

## 2. Occlusion Term And Terrain Shading

- [x] 2.1 Add a pure ambient/sky-occlusion calculation from the same height
  field and patch inputs, with unit tests showing a crevice occludes more sky
  than an open slope.

  `hemisphere_directions` + `sky_occlusion_visibility`; test
  `crevice_occludes_more_sky_than_an_open_slope`.
- [x] 2.2 Extend the terrain material extension bindings and shader to sample
  the baked self-shadow and occlusion terms through the existing patch material
  path.

  `TerrainSurfaceExtension` gains texture/sampler bindings 111/112 and
  `self_shadow_strength`/`sky_occlusion_strength` uniforms; `terrain_surface.wgsl`
  samples the map with UV1 and applies the strengths.
- [x] 2.3 Apply the self-shadow term to the direct-sun contribution only, and
  the occlusion term to the sky/ambient contribution only; add a test or
  headless assertion that a fully self-shadowed fragment with non-zero ambient
  retains indirect fill.

  The shader scales indirect via `diffuse_occlusion`/`specular_occlusion` and
  reconstructs only the directional-sun contribution
  (`direct_sun_self_shadow_correction`). Rust contract test
  `self_shadow_scales_direct_only_and_occlusion_scales_indirect_only`.
- [x] 2.4 Replace the texture-only crevice darkening with the physical occlusion
  term where it is available, keeping continuous blending across patch and LOD
  boundaries.

  The albedo crevice term is faded by `1.0 - sky_occlusion`, so it vanishes
  where the physical term is present and is unchanged on neutral (unbaked)
  patches; both are bilinearly continuous.

## 3. Water Shadow Receiving

- [x] 3.1 Remove `NotShadowReceiver` from the ocean and river water child
  entities while retaining `NotShadowCaster`.

  Both child spawns in `render.rs` now carry only `NotShadowCaster`; they receive
  the shared directional shadow map.
- [x] 3.2 Sample the baked landscape self-shadow term on the water surface so
  terrain beyond the directional-shadow cascade range still shades water.

  Each ocean patch over a baked height field gets its own water material bound to
  that patch's terrain-occlusion map, sampled through the water mesh's
  patch-local UV1; the baked self-shadow darkens the sea. It attenuates the body
  colour rather than forking the water lighting into direct/indirect terms, which
  would require a second hand-copied PBR fork; the shared material remains the
  fallback for unbaked patches.
- [x] 3.3 Validate shoreline, open-ocean, and distant-hill water shading under
  the shared ephemeris Sun, including the blended transparency path.

  Bounded `rocket` startup compiles the new water pipeline with no WGSL error and
  the blended path does not panic. Aesthetic judgment still needs a real display.

## 4. Budgets, Telemetry, And Validation

- [x] 4.1 Add bounded configuration for height-field ray-march samples per bake
  and occlusion samples per fragment, and report bake and sampling cost through
  `TerrainPerformanceTelemetry`.

  `TerrainOcclusionConfig` caps map resolution, sun/sky rays, sky directions, and
  per-fragment terms; `TerrainFrameAttribution` reports
  `occlusion_bake_ms`, `occlusion_bake_samples`, `occlusion_fragment_samples`,
  and `occlusion_patches_baked`. Defaults are conservative and only raised
  against measured telemetry.
- [x] 4.2 Add deterministic tests proving identical patch, seed, height-field
  parameters, and Sun direction reproduce identical occlusion terms, and that
  the occlusion layer cannot modify terrain source, collision, or simulation
  state.

  `occlusion_bake_is_deterministic_for_identical_inputs` and
  `occlusion_bake_cannot_modify_the_terrain_source`; the bake takes
  `&dyn TerrainSource` and writes only a fresh `Vec<f32>`.
- [x] 4.3 Verify the scientific-lighting Sun, sky, terminator, and directional
  cascades are unchanged, and re-run its shadow/sky regression checks.

  No file owned by `scientific-lighting-and-shadows` was modified; its tests pass
  in the full suite. Only the shared Sun direction and shadow-receiving path were
  consumed read-only.
- [x] 4.4 Run `cargo fmt --check`, `cargo check/test --features dem`,
  `cargo check --no-default-features`, and `cargo clippy --features dem --
  -D warnings`.

  `cargo fmt --check` clean; `cargo check --features dem` and
  `cargo check --no-default-features` clean;
  `cargo clippy --features dem -- -D warnings` clean;
  `cargo test --features dem --lib` -> 752 passed, 1 ignored, plus 3 pre-existing
  `rocket::tests::determinism_regression_tests` baseline failures that reproduce
  unchanged with this change stashed (parallel terrain workstream).
- [x] 4.5 Run bounded startup checks for `cargo run`, `cargo run -- craft`, and
  `cargo run -- rocket`, plus `openspec validate terrain-lighting-ao --strict`,
  and record any display limitation honestly.

  Bounded starts under the existing X display: normal and craft ran to the
  timeout with no panic; rocket initialized the terrain material with no shader
  compilation error. A first rocket run exposed and then resolved a WGSL swizzle
  l-value error, so shader compilation was actively validated. No screenshot or
  native-GPU visual validation was performed; final aesthetic tuning needs a real
  display. `openspec validate terrain-lighting-ao --strict` -> valid. The 3
  pre-existing baseline failures are documented in 4.4.
