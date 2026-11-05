# MASTER_ROADMAP

The single planning and tracking view for the planetary-realism programme.

**Authority model.** OpenSpec is the requirements authority: a task here is a
*unit of work*, and its acceptance criteria are a pointer to the OpenSpec
requirement it satisfies, or — where no requirement exists yet — an explicit note
that an OpenSpec change must be written first. This document never restates a
requirement in conflicting form. See `OPENSPEC_MAP.md`.

**Evidence model.** A task is not complete because code merged. States are:

| state | meaning |
| --- | --- |
| `PLANNED` | defined, not started |
| `IN_PROGRESS` | work has begun, no complete acceptance evidence |
| `IMPLEMENTED` | code exists and compiles; at least one applicable test exists; **not** visually/runtime verified |
| `VERIFIED` | implementation + tests pass + runtime inspected + visual acceptance met + performance evidence where applicable + data provenance documented |
| `BLOCKED` | cannot proceed for a stated reason outside the task's own scope |

`IMPLEMENTED` is deliberately weaker than `VERIFIED`. A task may sit in
`IMPLEMENTED` for a long time; that is the honest state, not a failure.

**Checkpoint procedure.** After every implementation checkpoint: update the task
state, add its evidence row to `VISUAL_EVIDENCE.md` and/or
`PERFORMANCE_BASELINES.md`, record the commit hash, update the known-limitations
list, and name the next highest-priority unblocked task.

---

## Phase P0 — Measurement foundations

The programme cannot be verified without repeatable frames and repeatable
timings. This phase exists because an unverifiable change is not a change.

### P0-01 — Deterministic pose and clock capture view
- **Status:** VERIFIED
- **Priority:** P0
- **Objective:** Make a screenshot the same scene every run by pinning the camera
  to a body-fixed pose and the simulation to an explicit time.
- **Dependencies:** none.
- **Subsystems:** presentation camera, simulation time, ephemeris.
- **Expected files:** `src/infrastructure/bevy_adapters/rocket/capture_view.rs`.
- **Acceptance:** `COSMIC_SYSTEMS_CAPTURE_POSE` + `COSMIC_SYSTEMS_CAPTURE_TIME`
  place the camera at the pose and freeze the cloud/weather/climate inputs; two
  runs at the same pose produce the same frame.
- **Tests:** pose parsing and body-position math in `capture_view.rs`.
- **Visual evidence:** every frame in `VISUAL_EVIDENCE.md` uses it.
- **Performance:** none.
- **Scientific validation:** pose is derived from the site, planet radius and
  ephemeris only; no vehicle state is involved.
- **Risks:** keypress-driven launch captures are *not* reproducible and must never
  be used for A/B.
- **Completion evidence:** commit `0ea824d`; documented in the audit plan.

### P0-02 — Offscreen capture without a swapchain
- **Status:** VERIFIED
- **Priority:** P0
- **Objective:** Produce a frame on a machine whose Vulkan presentation path is
  broken, by rendering into an image render target.
- **Dependencies:** P0-01.
- **Subsystems:** app assembly, render target, screenshot readback.
- **Expected files:** `src/application/offscreen_capture.rs`, `src/main.rs`,
  `docs/measurement/offscreen_capture.md`.
- **Acceptance:** `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=<png>` writes a PNG with no
  window; the run exits 0.
- **Tests:** env parsing, defaults, malformed-input fallback (4 unit tests).
- **Visual evidence:** all 2026-10-31 frames after `b7dbb2d`.
- **Performance:** **not** a frame-pacing harness — the runner is pinned to 60 Hz
  by construction.
- **Scientific validation:** none; presentation only.
- **Risks:** HUD is not drawn in this path, so measurements must read the world.
- **Completion evidence:** commit `b7dbb2d`; produced 7 frames at 300–1,200 km and
  ground level.

### P0-03 — Developer diagnostic render modes
- **Status:** VERIFIED
- **Priority:** P0 (unblocks TD-05, AT-03, TM-01)
- **Objective:** One toggle per term — albedo, normal, UV0/UV1, height, land/sea,
  surface class, LOD level, patch boundary, patch identity, input availability,
  occlusion, roughness, metallic, layer blend, morph offset, detail weight — so a
  defect can be attributed to a term instead of guessed.
- **Dependencies:** P0-02.
- **Subsystems:** terrain material uniform, terrain fragment shader, presentation
  camera, water/cloud visibility.
- **Expected files:** `src/presentation/diagnostics.rs` (mode set, parsing,
  names), `src/infrastructure/bevy_adapters/terrain/diagnostics.rs` (ECS wiring),
  `src/infrastructure/bevy_adapters/terrain/render.rs` (uniform fields, per-patch
  inputs, parity tests), `assets/shaders/terrain_surface.wgsl` (modes),
  `docs/measurement/terrain_diagnostics.md`.
- **Acceptance:** each mode renders a deterministic false colour of exactly one
  input; production is the default and is selected by a `u32` of zero; switching
  writes one uniform per material and recreates no mesh, image, cached patch or
  material asset; unknown mode values fall back to production with a warning;
  the release build registers no diagnostic key binding.
- **Tests:** `terrain_uniform_layout_matches_the_shader_struct` (every
  `#[uniform(104)]` field, in order, against the WGSL struct),
  `diagnostic_mode_codes_match_the_shader_constants`,
  `diagnostic_input_flags_match_the_shader_bits`,
  `material_carries_the_diagnostic_uniform`,
  `patch_identity_separates_neighbours_and_is_stable`, seven
  `terrain::diagnostics` ECS tests (including one that a full visit of every
  mode leaves simulation time and the terrain authority untouched), seven
  `presentation::diagnostics` tests.
- **Visual evidence:** 18 frames at the 300 km pose (production plus one per
  mode), six at 1,200 km, seven at ground and altitude (3/10/30/70 km). See
  `VISUAL_EVIDENCE.md`.
- **Performance:** one `u32` comparison per fragment in production; the
  diagnostic branch is uniform per patch and not taken. Native frame time with
  diagnostics enabled is **unmeasured** (P0-04 is blocked).
- **Scientific validation:** none; diagnostics only. Every mode reads an input the
  production shader already uses, and the terrain authority is untouched.
- **Risks:** a wrong uniform layout silently kills the terrain material, so the
  parity test is mandatory. Diagnostics cover streamed terrain patches only; the
  ocean, clouds and the far-field proxy keep their own materials, which is
  stated in the mode's own documentation rather than hidden.
- **Completion evidence:** commit `f3b1af6`. What the first runs
  showed is recorded under TD-05.

### P0-04 — Performance baseline refresh on current commit
- **Status:** BLOCKED
- **Priority:** P1
- **Objective:** Re-measure p50/p95/p99/worst frame at the current commit.
- **Dependencies:** a presentable window (environment).
- **Subsystems:** performance telemetry.
- **Expected files:** `docs/progress/PERFORMANCE_BASELINES.md`,
  `docs/native_release_performance_audit.md`.
- **Acceptance:** a release run with `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1` at 720p
  and 1440p, recorded with commit hash and machine.
- **Tests:** none (measurement).
- **Visual evidence:** not applicable.
- **Performance:** the measurement itself.
- **Scientific validation:** none.
- **Risks:** the offscreen path is 60 Hz-pinned and cannot substitute.
- **Completion evidence:** blocked on the presentation failure recorded in
  `README.md`; last real numbers are from 2026-09-11.

---

## Phase P1 — Scientific data and terrain correctness

### TD-01 — Ocean surface sits on the sea-level datum
- **Status:** VERIFIED
- **Priority:** P0
- **Objective:** Stop the sea surface from ramping into the terrain at the shore.
- **Dependencies:** none.
- **Subsystems:** water mesh, terrain material.
- **Expected files:** `src/infrastructure/bevy_adapters/terrain/render.rs`
  (`water_mesh_for_patch`).
- **Acceptance:** every ocean-cap vertex lies on the sea-level datum; the waterline
  is cut by the depth buffer, not by the mask triangle boundary; lakes keep their
  local surface.
- **Tests:** `ocean_cap_stays_on_the_datum_across_a_sloped_shore`,
  `ocean_caps_preserve_outward_terrain_triangles_and_stitches`.
- **Visual evidence:** measured on the 3 km nadir frame — pale shoreline pixels
  1.58 % → 0.40 %, median luminance unchanged (0.341 → 0.338). Images from that
  A/B were not retained before the evidence process existed; the number stands,
  the picture does not.
- **Performance:** no measurable change; the cap covers only triangles that reach
  water.
- **Scientific validation:** the datum is `sea_level_height_m` from the terrain
  authority, including the EGM2008 geoid undulation.
- **Risks:** lakes are not at the datum and keep their own surface; a lake that
  crosses a sea-level patch edge is still approximated.
- **Completion evidence:** commit `fb686ba`.

### TD-02 — Launch sites graded in the terrain authority
- **Status:** VERIFIED
- **Priority:** P0
- **Objective:** Make the ground under a facility genuinely level for mesh,
  collision and radar, instead of drawing a flat apron over a slope.
- **Dependencies:** TD-10 and TD-11 (satisfied by the payload precedence and the
  platform floor below).
- **Subsystems:** terrain composition, launch site presentation.
- **Expected files:** `src/domain/services/terrain_source/layered.rs`,
  `catalog.rs`.
- **Acceptance:** inside the flat radius the composed height equals the site's own
  composed height for `height_m` and `mesh_height_m`; outside the blend it equals
  the ungraded surface.
- **Tests:** `graded_site_is_level_under_the_pad_and_returns_to_natural_terrain`,
  `a_source_without_sites_is_unchanged`,
  `default_earth_source_grades_launch_pads_above_the_sea_datum`.
- **Visual evidence:** `2026-11-05-td02-{starbase,ksc,wallops,baikonur,vandenberg,kourou,mahia}-fixed.jpg`
  at `17bc6f5`, plus the existing Papua frame. **All seven captured pads are
  flush and dry.** The pre-fix frames (`-<site>.jpg` at `c582708`) are kept as
  the record of the defect: Starbase, KSC and Wallops stood in open water.
- **Measured site clearance** after TD-11 (`height_m - sea_level_height_m`,
  platform floor 3 m): Starbase **+3.00 m**, KSC legacy reference **+3.00 m**,
  Papua **+3.00 m**; Kourou **+9.19 m**, KSC LC-39A **+13.76 m**, Wallops
  **+13.26 m**, Mahia **+37.59 m**, Vandenberg **+104.31 m**, Baikonur
  **+113.55 m**.
- **Performance:** 125 extra terrain samples per site at startup, once, plus the
  nine preloaded site payload tiles (TD-10).
- **Scientific validation:** the grade is a terrain-authority property, so the
  rendered mesh and collision cannot disagree. The pad anchor is the resident
  payload (or its TD-11 floor), not the coarse global fallback.
- **Risks:** grading reclassifies slope-derived material (see TM-01); a site whose
  DEM is pathological could produce a steep blend.
- **Completion evidence:** grading mechanism and tests at `5901c82`; the
  per-site capture set and the two fixing slices at `17bc6f5`.

### TD-10 — The pad anchor and the surrounding field must not depend on the erosion cache
- **Status:** VERIFIED
- **Priority:** P0
- **Objective:** Stop the launch-site elevation and the visible terrain from
  depending on which regional payload tiles happen to be resident when the
  erosion field is baked, and stop the disk erosion cache from changing the
  terrain.
- **Dependencies:** none.
- **Subsystems:** terrain composition, erosion, elevation payload residency.
- **Expected files:** `src/domain/services/erosion/source.rs`,
  `src/application/rocket_spawning.rs`, `src/domain/services/terrain_source/catalog.rs`.
- **Acceptance:** the pad anchor sampled at startup equals the resident payload
  (or its TD-11 floor); the same site renders the same terrain with
  `COSMIC_SYSTEMS_EROSION_CACHE` on, off, and cold.
- **Pad anchor (`5a7eb1d`):** `TerrainSource::resident_measured_height_m`
  exposes the resident payload sample, `ErodedTerrainSource` returns it instead
  of the erosion refinement, and `EarthTerrainSource` preloads every predefined
  site's covering payload tile before the grade anchors are sampled. The pad
  anchor now equals the package probe (before the TD-11 floor) at every site.
- **Surrounding field (`5134ed2`):** the erosion rasters used to be baked from
  the *sampling* base — the streamed `TileElevationSource` — so height, moisture,
  flow and river channels outside a resident payload tile depended on which
  payload tiles had loaded first, and on whether an old raster came from disk.
  `ErodedTerrainSource` now holds a separate immutable `bake_base`
  (`with_store_over_bake_base`); Earth passes the coarse measured global DEM,
  which is also what the offline `erosion_bake` already used. The feather blends
  back to that same base. The bake namespace gained a
  `#bake-base=global-measured-v1` suffix, so every raster baked under the old
  semantics is invalidated rather than silently reused.
- **Tests:** `resident_measured_height_is_absent_until_a_payload_tile_installs`,
  `a_resident_measured_payload_suppresses_the_erosion_refinement`,
  `erosion_bakes_from_the_stable_base_not_the_sampling_overlay`,
  `default_earth_source_grades_launch_pads_above_the_sea_datum`.
- **Measured coupling before the pad fix** (cold composition probe; resident
  payload `D` against composed `height_m` `H`): Vandenberg `D=68.10, H=149.22`
  (**+81.12 m**); Mahia `58.94 → 28.65` (−30.29); KSC LC-39A `−15.96 → −29.87`
  (−13.90); Wallops `−24.43 → −51.31` (−26.88); Papua `71.98 → 77.59` (+5.61);
  Starbase `−22.49 → −23.37` (−0.88). The pattern was the difference between
  the regional NASADEM and the coarse global fallback.
- **Cache A/B/C after the fix** (identical pose, `COSMIC_SYSTEMS_CAPTURE_TIME=0`):
  Starbase cold-vs-warm mean **2.53/255** (2.59 % > 8), cold-vs-off
  **2.37/255** (1.43 %), warm-vs-off **2.40/255** (2.02 %); Baikonur
  cold-vs-warm **2.96/255** (4.00 %), cold-vs-off **3.16/255** (4.67 %),
  warm-vs-off **3.06/255** (4.45 %). These are the same magnitude as two
  identical warm runs (2.4–3.1/255), so the disk cache no longer selects a
  different field. The off runs need **1,200 frames** to converge; at 240 frames
  the runtime bake is still streaming and shows ungenerated water
  (`2026-11-05-td10-*-off-240.jpg`). The pad state is identical in every frame.
- **Visual evidence:** `2026-11-05-td10-{starbase,baikonur}-{cold,warm,off-1200}.jpg`
  and the incomplete `2026-11-05-td10-starbase-off-240.jpg`; the TD-02 `-fixed`
  frames still show the corrected pads.
- **Repeatability:** two identical warm-cache Starbase runs differ by mean
  2.4/255 (2.3 % above 8/255) and Baikonur by 3.1/255 (4.8 %); both are inside
  the documented near-ground streaming variance and neither changes the pad
  state.
- **Scientific validation:** the regional package's own site probe is the
  reference the runtime now matches. Erosion refines only the coarse measured
  base; a resident payload is the finer measurement and outranks it.
- **Risks:** preloading whole payload regions would defeat the residency budget;
  the fix loads one tile per site, not a region. The global startup bake still
  touches thousands of 2 ° erosion tiles, which the P0-04 performance work must
  revisit.
- **Baseline decoupling:** the ascent fixture pins its launch altitude and uses a
  flat pad (`c886205`, `17bc6f5`), so neither TD-10 nor TD-11 moves the committed
  trajectories.
- **Completion evidence:** `5a7eb1d` (payload precedence and preload),
  `5134ed2` (stable bake base), and the A/B/C captures above.

### TD-11 — A graded coastal pad must sit above the local sea datum
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** A launch platform is never underwater. Where the measured
  ground at a site is at or below the local sea surface, the grade must place the
  pad above it by a documented clearance instead of rendering a tower standing in
  the ocean.
- **Dependencies:** TD-10 (satisfied).
- **Subsystems:** terrain composition, site grading, water cap.
- **Expected files:** `src/domain/services/terrain_source/layered.rs`,
  `catalog.rs`.
- **Acceptance:** for every predefined site, the graded pad height is at least
  the local sea datum plus the declared clearance; the change is recorded per
  site.
- **Tests:** `coastal_platform_clearance_floors_a_pad_at_the_waterline`,
  `default_earth_source_grades_launch_pads_above_the_sea_datum`.
- **Implementation:** `LayeredTerrainSource::with_coastal_platform_clearance`
  floors the *platform* at `sea_level + 3.0 m`
  (`EARTH_SITE_PLATFORM_CLEARANCE_M`) after the measured composition; the
  surrounding measured ground is untouched. Kourou's preset also moved from
  Kourou town (`5.159, −52.649`, a marsh 4 m below the sea surface) to the
  Guiana Space Centre ELA-3 pad (`5.2361, −52.7686`).
- **Visual evidence:** `2026-11-05-td02-*-fixed.jpg` at `17bc6f5`; Starbase and
  Wallops are dry where they previously stood in water.
- **Measured before the floor:** Starbase +0.10 m, Papua +0.54 m, KSC legacy
  reference +0.94 m; Kourou town −3.96 m. After the floor every site is at least
  +3.00 m (see TD-02).
- **Baselines:** the ascent fixture pins its launch altitude at `c886205`, so
  flooring the `ksc-default-geodetic` pad does not move the committed
  trajectories.
- **Completion evidence:** `17bc6f5`; tests plus the seven re-captured pads.

### TD-03 — Regional imagery colour match to the global overview
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** Remove the pale inset created by substituting a Sentinel-2
  surface-reflectance mosaic for a Blue Marble overview at a patch edge.
- **Dependencies:** none.
- **Subsystems:** imagery manifest, imagery streaming, terrain material.
- **Expected files:** `assets/configs/terrain/earth_imagery_v1.ron`,
  `src/domain/value_objects/imagery_manifest.rs`,
  `src/domain/services/imagery_package.rs`,
  `src/infrastructure/bevy_adapters/terrain/imagery.rs`,
  `docs/datasets/earth_imagery_v1.md`.
- **Acceptance:** a declared per-channel linear gain is applied before mip
  generation; tiles outside a region are untouched.
- **Tests:** `checked_in_color_match_is_the_measured_mean_ratio`,
  `color_match_gain_scales_linear_light`, `identity_color_match_is_a_no_op`,
  `color_gain_is_scoped_to_the_region`,
  `the_colour_match_switch_only_disables_on_an_explicit_off_value`.
- **Measurement facility:** `COSMIC_SYSTEMS_IMAGERY_COLOR_MATCH=off` installs
  regional tiles without the measured gain, so a before/after pair differs by
  exactly the correction. The default is on; any unrecognised value keeps it on,
  and the switch is presentation-only.
- **Visual evidence:** `2026-11-05-td03-albedo-40km-{before,after}.jpg` at
  `-8.0,139.9,40000`, 400 frames, `COSMIC_SYSTEMS_CAPTURE_TIME=0`, diagnostic
  `albedo`. The linear-light means over the same pixel blocks move from
  **3.87×** region/overview before to **1.16×** after. The median per-pixel
  before/after ratio is **4.264, 2.470, 4.705** against the declared inverse gain
  **4.269, 2.464, 4.705** (worst channel 0.2 %). Outside the region the frame is
  unchanged: mean difference 0.007/255, 3 of 180,000 pixels above 2/255. The
  16.6 km cross-fade is the gradual rise between x ≈ 400 and 560, so the colour
  correction and the fade compose rather than step. In-context production frames
  at 80 km are `2026-11-05-td03-production-80km-{before,after}.jpg`.
- **Caveat recorded, not hidden:** at 80 km the outer patches are coarser than
  the package's `min_level: 8`, so they bind the global overview and the
  mosaic/overview *resolution* boundary is quantised to a patch edge. The tone
  now matches across it, but the edge remains a coverage limit of the package's
  LOD range, visible in `2026-11-05-td03-albedo-80km-*` and identified by
  `2026-11-05-td03-lod-80km.jpg`. Fixing it needs regional tiles below level 8
  (a package-production change), not another shader term.
- **Performance:** one pass over 256×256 texels per tile install; unmeasured, but
  bounded by the 4-tile-per-frame admission limit.
- **Scientific validation:** gain = overview mean / regional mean in linear light,
  both means recorded in the manifest with the method and source checksums.
- **Risks:** matching the mean is not matching the distribution; a mosaic with a
  different contrast still reads slightly different. The measured band matches
  the luminance but not every per-channel mean in every sub-block, because the
  measurement footprint and a given frame sample different terrain.
- **Completion evidence:** `aca26dd` (gain and tests); `2026-11-05-td03-*`
  frames and this A/B.

### TD-04 — Imagery edge feather
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** Fade the regional inset into the overview across the region edge
  so the last patch boundary stops stepping between 10 m and 2 km imagery.
- **Dependencies:** P0-03 (satisfied), TD-03.
- **Subsystems:** terrain material uniform, imagery manifest regions, imagery
  tile resolution.
- **Expected files:** `assets/shaders/terrain_surface.wgsl`,
  `src/infrastructure/bevy_adapters/terrain/render.rs`,
  `src/infrastructure/bevy_adapters/terrain/imagery.rs`,
  `src/domain/value_objects/imagery_manifest.rs`,
  `src/domain/services/imagery_package.rs`,
  `assets/configs/terrain/earth_imagery_v1.ron`.
- **Acceptance:** `imagery_weight` is multiplied by a `smoothstep` over a band
  inside the region bounds, evaluated from the fragment's body position; the
  transition is independent of where patch boundaries fall.
- **What had to change besides the shader:** the tile had to be *bound* for
  patches that straddle the region edge. Resolution was centre containment, so a
  straddling patch bound no tile and the edge fell on the next patch boundary
  instead — measured at x = 459 in the 40 km frame, 59 px inside the region bound.
  Resolution and colour matching now use footprint overlap.
- **Tests:** `terrain_uniform_layout_matches_the_shader_struct` (the new
  `imagery_bounds`/`imagery_feather_deg` fields), `regional_imagery_is_tapered_across_the_region_edge`
  (the weight is multiplied by the band and the coordinate is the body position),
  `feather_band_is_zero_at_the_edge_and_one_a_band_inside` (edge, centre, outside,
  midpoint, monotonicity), `imagery_blend_carries_the_region_footprint_only_when_one_applies`,
  `checked_in_feather_band_spans_several_overview_texels`,
  `a_missing_or_impossible_feather_band_is_rejected`,
  `patch_windows_handle_longitude_wrap_and_poles`,
  `longitude_span_is_the_complement_of_the_largest_gap`,
  `a_patch_straddling_the_region_edge_still_resolves_to_the_tile`.
- **Visual evidence:** `2026-11-01-td04-pair-40km.jpg` (left: a hard patch-shaped
  corner before; right: the same ground after), the full `albedo` frames at 40 km
  before and after, the 80 km `albedo` frame, and the `patch-boundary` view at the
  same pose that shows which columns are patch edges. Measured with
  `scripts/imagery_region_mask.py` + `scripts/measure_tone_edge.py`:
  * worst first difference within ±40 px of the region bound: **4.23/255 before,
    0.72/255 after** (p90); after it equals the frame's own texture level and the
    frame's worst column anywhere falls from **16.14** to **0.93/255**;
  * the before/after tonal change is confined to x = 381..737, **16.4 km**, which
    is the declared 0.15° band (16.6 km) — nothing outside it changes.
- **No 400 km frame, and why:** the manifest publishes levels 8–12 and the visible
  cover at 400 km is coarser, so no regional tile is bound there at all — the
  400 km `albedo` frame is unchanged within 1/255. The defect lives where the
  inset is drawn, roughly below 100 km; a 40 km and an 80 km frame are the honest
  evidence.
- **Performance:** two normalisations, an `asin`, an `atan2`, a `min` over four
  bounds and a `smoothstep`, per fragment, **only** for patches that bind a
  regional tile (the branch is on uniform per-patch values, so every other patch
  skips it). No frame-time A/B was run: the change is confined to the handful of
  patches inside one 1°×1.2° region and the available frame-time metrics cannot
  resolve it. Claiming "< 1 %" would be a guess, so it is not claimed.
- **Scientific validation:** the feather is presentation only. It does not move
  terrain height, collision, the region's data, or its checksums; the manifest
  change adds a presentation band, and the elevation and imagery checksums are
  unchanged.
- **Risks:** the uniform layout gained a `vec4` and an `f32`, so the material
  breaks silently if the parity test is ever skipped. The band is declared in
  degrees, so it is a different width in metres at different latitudes — correct
  for a geographic box, but it means the transition is wider near the poles.
- **Completion evidence:** MET. The imagery edge now follows the region bound as
  a 16.6 km cross-fade, the frame's worst step at either the bound or a patch edge
  is the frame's own texture level, and the change is confined to the band.

### TD-05 — Rectangular tone blocks at altitude
- **Status:** VERIFIED
- **Priority:** P0 (largest visible defect)
- **Objective:** Attribute the axis-aligned tone blocks at 300–1,200 km to a
  specific term and remove them.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** terrain material (per-vertex roughness), elevation data, water
  material (diagnostics).
- **Expected files:** `src/domain/services/terrain_source/appearance.rs`,
  `src/infrastructure/bevy_adapters/terrain/surface/mod.rs`,
  `src/infrastructure/bevy_adapters/terrain/render.rs`,
  `src/infrastructure/bevy_adapters/terrain/diagnostics.rs`,
  `src/infrastructure/bevy_adapters/terrain/water.rs`,
  `assets/shaders/terrain_surface.wgsl`, `assets/shaders/water.wgsl`.
- **Acceptance:** at the P0-01 pose no block boundary is visible that does not
  correspond to a real albedo change within ±1/255 of the surrounding trend.
- **Tests:** `surface_roughness_is_the_appearance_law` (the per-vertex function
  and the material law cannot drift), `surface_roughness_is_continuous_in_height_and_slope`,
  `per_vertex_roughness_agrees_across_a_shared_patch_edge` (shared edge vertices
  are bit-identical), `mesh_carries_roughness_and_morph_offsets_in_separate_channels`,
  `shader_takes_roughness_from_the_vertex_field`; for the water views,
  `each_material_family_receives_only_its_own_codes` (a terrain code must never
  reach the sea shader or the reverse), `cloud_deck_is_hidden_by_every_mode_and_water_only_by_terrain_modes`,
  `diagnostic_mode_codes_match_the_shader_constants` (now pins each family's code
  to the shader that decodes it), and the existing capture-pair comparison.
- **Visual evidence:** the land component is attributed **and fixed**.
  * Attribution (2026-11-01): `roughness` rendered `surface.roughness`, one scalar
    per patch sampled at the patch centre. At 300 km that turned the coastline
    into a patch-sized staircase between 0.25 (shallow seafloor) and 0.85 (land);
    at 1,200 km it was a quilt of ~300 px blocks.
  * A/B: rebuilding only `patch_material` with a constant `(0.85, 0.0)` reduced a
    measured patch-boundary step from 12.2/255 to 4.1/255
    (`2026-11-01-ab-constant-roughness-1200km.jpg`).
  * Fix: roughness is now `surface_roughness(elevation, zone_lat, slope)` per
    vertex — the same law `surface_appearance` calls — carried in the green
    channel of the terrain vertex colour beside the morph offset in red. The
    measurement repeats: **12.3/255 before, 4.2/255 after**, against 4.3/255 for
    the A/B (`2026-11-01-production-1200km-continuous.jpg`). The 300 km view now
    follows the coastline instead of one patch edge
    (`2026-11-01-diag-roughness-300km-continuous.jpg`).
  * Ruled out with a frame: world normals, UV0/UV1, baked occlusion, per-patch
    input differences, LOD layout, and the CDLOD morph
    (`2026-11-01-ab-morph-off-height-1200km.jpg`).
  * **The ocean half does not exist (2026-11-01).** Six water views were added
    (modes 18–23: depth, UV1, body colour, shadow, mirror, opacity) and applied
    at 300 km and 1,200 km. Every term is continuous, and a column-step test
    cannot separate the known patch-edge columns from random columns
    (`p = 0.12 … 1.00`, production included). The sea's tone is the depth ramp
    over real bathymetry — production luminance correlates with the depth view at
    `-0.78` across 1,127 columns — plus cloud shadow. The earlier inference that
    "the ocean blocks are not roughness, therefore they are something else" had a
    false premise: no water term carries a patch signal at all.
  * The large "datum rectangle" is genuine near-sea-level lowland, not a renderer
    defect: `terrain_authority_transect` reproduces the in-engine values exactly
    and every mesh vertex matches the source to within the 5 m skirt.
  * Acceptance measured against the surrounding trend (2026-11-01) with
    `scripts/measure_patch_seam.py`, which fits each side of a seam quadratically
    and reports the discontinuity left at the boundary, with a control 40 px along
    the seam:
    * the seam used for the original box measurement measures **+4.14/255 before
      and −0.04/255 after** (18 samples, identical settings), and the paired crop
      `2026-11-01-td05-pair-1200km.jpg` shows the wedge disappear;
    * frame-wide median excess at 300 km **+0.84 → +0.25/255** (162/161 samples)
      and at 1,200 km **+0.37 → +0.47/255** (118/119 samples); both post-fix
      figures are inside the ±1/255 acceptance;
    * every per-patch input measured the same way is continuous at seams:
      `albedo` −0.30, `world-normals` −0.00, `height` −0.49, `detail-weight`
      +0.00, `layer-blend` 0.00, `morph-offset` +0.10, while `roughness` measured
      +25.63 before the fix. Terrain either side of a boundary is what the seam
      jump still measures, and that is what the control measures too.
- **Performance:** one extra `surface_appearance`-equivalent evaluation per
  vertex when a patch surface is prepared (pure arithmetic, no source query, no
  texture). No per-frame cost. Production frames changed by a mean of 1.54/255 at
  300 km and 2.53/255 at 1,200 km, concentrated at patch edges.
- **Scientific validation:** not applicable; all terms here are presentation.
- **Risks:** the acceptance is established on the median seam excess and on the
  one seam that was visibly wrong, not as a per-pixel worst case; the seam
  measure's per-sample tail is terrain relief, which no extrapolation can separate
  from a defect. The 1,200 km frame-wide median is unchanged between builds
  because only some neighbouring patch pairs differed in roughness, so the
  targeted region is the load-bearing evidence there. The waterline remains a real
  discontinuity in the appearance law that the mesh now interpolates across.
- **Completion evidence:** MET. The land term was attributed to a per-patch
  roughness constant and replaced with a per-vertex field; the ocean half was
  measured with six new water views and refuted; the residual at the seam that
  motivated the task is −0.04/255 against its own trend, and the frame-wide
  median is +0.25/255 at 300 km and +0.47/255 at 1,200 km, both inside ±1/255.
  Paired crops: `2026-11-01-td05-pair-300km.jpg`,
  `2026-11-01-td05-pair-1200km.jpg`.

### TD-06 — Synthetic detail must not fabricate coastline
- **Status:** BLOCKED
- **Priority:** P1
- **Objective:** Stop the signed ±24 m detail octave from pushing near-sea-level
  ground below the datum and having the ocean cap flood it.
- **Dependencies:** isolation of the LEO baseline shift.
- **Subsystems:** terrain composition, water.
- **Expected files:** `src/domain/services/terrain_source/layered.rs`,
  `catalog.rs`.
- **Acceptance:** near the datum the detail contribution may raise ground but not
  lower it, identically in `height_m` and `mesh_height_m`.
- **Tests:** `synthetic_detail_cannot_create_sea_near_the_datum` (passes with the
  guard enabled in the fixture).
- **Visual evidence:** the defect is visible in the ground frames as pale-blue
  flooded patches on the coastal plain.
- **Performance:** none.
- **Scientific validation:** the guard is opt-in per detail layer because only a
  body with an ocean can be flooded by it.
- **Risks:** **enabling it moves the committed LEO insertion baseline**, and the
  cause is not isolated — the site reference sits inside a pad flat zone where the
  guard should be a no-op. Shipping a baseline shift without understanding it is
  forbidden by the project rules, so the mechanism ships inert.
- **Completion evidence:** mechanism + test at `5901c82`; activation pending.

### TD-07 — Coastal DEM resolution for the Papua site
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Replace the 900 m global base with the regional package where one
  exists, so the 3 km view stops showing angular water polygons.
- **Dependencies:** TD-01.
- **Subsystems:** elevation payload, terrain source.
- **Expected files:** `scripts/ingest_regional_dem.py`,
  `docs/datasets/earth_regional_papua_v1.md`, `terrain_source/catalog.rs`.
- **Acceptance:** the site's elevation comes from the regional package at its
  declared resolution; the global base remains the fallback; provenance and
  checksums recorded.
- **Tests:** payload selection; fallback when the package is absent.
- **Visual evidence:** 3 km and 300 km frames at the P0-01 pose.
- **Performance:** resident tile budget unchanged.
- **Scientific validation:** source, version, licence, datum, epoch and
  uncertainty recorded in the dataset document.
- **Risks:** mixing vertical datums between the global and regional packages.
- **Completion evidence:** pending.

### TD-08 — Vertical datum audit
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Prove the geoid/ellipsoid correction is applied exactly once
  across DEM, imagery, water and collision.
- **Dependencies:** none.
- **Subsystems:** DEM, geoid, terrain source, water.
- **Expected files:** `src/domain/services/geoid.rs`,
  `dem_terrain_source.rs`, `terrain_source/mod.rs`.
- **Acceptance:** a known coastal control point reports the same height (within
  the documented tolerance) from every consumer.
- **Tests:** control-point test with published geoid undulation.
- **Visual evidence:** not applicable.
- **Performance:** none.
- **Scientific validation:** EGM2008 undulation documented with source and version.
- **Risks:** a second correction hidden in a package producer.
- **Completion evidence:** pending.

### TD-09 — Cube-face and antimeridian continuity tests
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Pin terrain and imagery continuity across cube-face seams and the
  antimeridian.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** terrain source, imagery tiles.
- **Expected files:** `src/domain/services/terrain_source/`,
  `src/domain/services/imagery_tiles.rs`.
- **Acceptance:** sampling the same great-circle point from either side of a seam
  agrees within the source tolerance.
- **Tests:** face-seam pair, antimeridian pair, polar clamp.
- **Visual evidence:** diagnostic UV/tile-identity frames at a seam.
- **Performance:** none.
- **Scientific validation:** continuity is a property of the authoritative source.
- **Risks:** tests that only re-derive the same formula prove nothing; they must
  compare independent paths.
- **Completion evidence:** pending.

---

## Phase P2 — Launch facility integration

### LF-01 — Graded platform footprint
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** Keep the prepared level area to the facility's own footprint so
  the site reads as a clearing, not a lighting artifact.
- **Dependencies:** TD-02.
- **Subsystems:** terrain composition, launch presentation.
- **Expected files:** `src/domain/services/terrain_source/catalog.rs`.
- **Acceptance:** flat radius 110 m, blend 280 m; the pad and its apron sit inside
  the flat area; no visible disc at ground level.
- **Tests:** the TD-02 grading tests still pass; full suite green.
- **Visual evidence:** `2026-10-31-pad-250m-footprint.jpg` (disc present) vs
  `2026-10-31-pad-110m-footprint.jpg` (disc gone).
- **Performance:** unchanged.
- **Scientific validation:** the footprint is a presentation-scale grade; it does
  not move the site's own elevation.
- **Risks:** a larger facility would need a larger footprint and the disc returns.
- **Completion evidence:** commit `d89e7d6`.

### LF-02 — Facility hardware grounded on the pad plane
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Verify tanks, blockhouse, floodlights and hold-downs sit on the
  pad plane with no float or interpenetration.
- **Dependencies:** LF-01.
- **Subsystems:** launch presentation.
- **Expected files:** `src/application/rocket_spawning.rs`.
- **Acceptance:** every structure's base is within 0.05 m of the surface under it.
- **Tests:** a spawned-scene test asserting each structure's minimum Y against the
  pad plane.
- **Visual evidence:** ground frame with the structures in profile.
- **Performance:** none.
- **Scientific validation:** presentation only.
- **Risks:** items placed by eye rather than derived from the pad plane.
- **Completion evidence:** pending.

### LF-03 — Flame trench and deflector visible and grounded
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Make the trench and deflector read as cut into the pad rather than
  buried under it.
- **Dependencies:** LF-02.
- **Subsystems:** launch presentation.
- **Expected files:** `src/application/rocket_spawning.rs`.
- **Acceptance:** the trench opening is visible from the chase camera; nothing
  floats below the pad plane.
- **Tests:** geometry placement test.
- **Visual evidence:** ground frame from two headings.
- **Performance:** none.
- **Scientific validation:** presentation only; not claimed as a surveyed model.
- **Risks:** overlapping the deck and z-fighting.
- **Completion evidence:** pending.

### LF-04 — Facility scale and structural detail
- **Status:** PLANNED
- **Priority:** P3
- **Objective:** Bring tower, arms and supporting structures to a believable scale
  and level of detail for the chase camera.
- **Dependencies:** LF-02.
- **Subsystems:** launch presentation.
- **Expected files:** `src/application/launch_tower.rs`,
  `src/application/rocket_spawning.rs`.
- **Acceptance:** tower height and platform spacing scale with the vehicle; no
  member floats without an attachment.
- **Tests:** existing attachment tests extended for the vehicle families.
- **Visual evidence:** ground frame per vehicle family.
- **Performance:** structure draw distance respected.
- **Scientific validation:** not a surveyed model; documented as an approximation.
- **Risks:** more geometry against the presentation budget.
- **Completion evidence:** pending.

---

## Phase P3 — Terrain materials

### TM-01 — Material classification stable under grading
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Stop levelling ground from reclassifying its surface material over
  the whole graded footprint.
- **Dependencies:** LF-01.
- **Subsystems:** terrain appearance, layer weights.
- **Expected files:** `src/domain/services/terrain_source/appearance.rs`,
  `catalog.rs`, `assets/shaders/terrain_surface.wgsl`.
- **Acceptance:** the material blend across the graded area is continuous with the
  surrounding landscape within the normal noise band.
- **Tests:** slope-classification invariant with a synthetic grade.
- **Visual evidence:** ground frame with a larger synthetic footprint.
- **Performance:** none.
- **Scientific validation:** classification must follow the surface being shown.
- **Risks:** freezing classification could hide real slope changes.
- **Completion evidence:** pending; LF-01 shrank the footprint to mitigate.

### TM-02 — Geometric detail band at distance
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Give terrain geometry beyond LOD 11 so distant ground is not flat.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** terrain source, LOD, shader.
- **Expected files:** `terrain_source/procedural.rs`, `layered.rs`,
  `terrain/render.rs`, `assets/shaders/terrain_surface.wgsl`.
- **Acceptance:** relief remains visible at 5–50 km without shimmer; no LOD pop.
- **Tests:** LOD continuity; detail weight monotonic in level.
- **Visual evidence:** 3 km, 40 km and 300 km frames.
- **Performance:** must stay inside the patch budget; measured before/after.
- **Scientific validation:** presentation scale relief, labelled as such.
- **Risks:** shimmer and aliasing; cost per patch.
- **Completion evidence:** pending.

### TM-04 — Local and layered albedo tone match to the measured overview
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Stop the ground brightening about 2–2.5× when the camera descends
  from orbit to the surface, by calibrating the local source surfaces and the
  layered ground albedo against the measured overview and imagery — the same
  measured-gain method TD-03 used on the regional mosaic.
- **Dependencies:** AT-03 (attribution complete), TD-03.
- **Subsystems:** terrain material composition (local surfaces, layered ground).
- **What was wrong (measured):** at `-4.0,138.0` the 3 km production frame was
  **4.36×** its own `albedo` view (per-pixel median over the same forest tone);
  the 40, 300 and 400 km frames were 2.03×, 1.84× and 1.85×. The extra factor is
  the local surface map's *absolute* albedo, mixed at weight
  `0.4 × detail_weight`: it is about **4×** the overview's luminance on Borneo
  forest. The layered material was already a unit-luminance tint and contributed
  nothing to the step.
- **Causal control:** a build that replaced that mix with the base — leaving
  normals, roughness, the layered material and every variation term untouched —
  returned the 3 km frame to **1.98×**, i.e. the orbital level. Solving
  `4.36 / 1.98 = (1 − w) + w·g` with `w = 0.4` gives `g = 4.01`.
- **Fix:** the local map is now a *tint* anchored to the base. Its sample is
  scaled by `base_luma / local_mean_luma`, where the mean is the map's own
  coarsest mip (level 7 of 128 texels, its single-texel mean), and mixed at the
  same `0.4 × detail_weight`. `LUMA_REC709` became one shader constant shared
  with the layered tint. Scaling by the *patch* mean rather than by the
  fragment's own texel keeps the map's relative material contrast — rock stays
  brighter than forest — while fixing the mean.
- **Why not retune `appearance.rs`:** its values are physically plausible and
  feed every other material; a single global gain would have darkened deserts
  and snow, which already agree with the overview, and a per-biome gain would
  have needed its own global measurement. The mismatch is one of *tone
  authority*, so it is fixed where the two sources meet, in the composition
  step, and the source values stay the appearance law's.
- **Acceptance:** at a fixed site the production land's linear luminance changes
  by no more than the atmosphere and Sun angle explain between 400 km and 3 km,
  while the `albedo` view stays identical; the calibration is a measured ratio
  recorded with its derivation, not a look. **MET:** the 3 km frame is 2.01× its
  albedo against 2.03× at 40 km; the centred crop of the same ground moves
  0.0783 → 0.0351 linear against 0.0344 at 40 km, i.e. **+2 % where the defect
  was +128 %**. The `albedo` view is unchanged (centre p50 0.0169 at 3 km and
  40 km).
- **Tests:** `local_detail_albedo_is_tinted_to_the_base_luminance`,
  `local_albedo_mean_mip_matches_the_patch_map_resolution`,
  `measured_local_albedo_gain_and_the_tint_that_removes_it`,
  `every_ground_layer_albedo_can_carry_a_unit_luminance_tint`.
- **Visual evidence:** `2026-11-03-tm04-pair-3km.jpg` (left: the 2.2× brighter
  near ground; right: the same ground at the measured tone),
  `2026-11-03-tm04-3km-before.jpg`, `2026-11-03-tm04-3km-after.jpg`,
  `2026-11-03-tm04-albedo-3km.jpg`, `2026-11-03-tm04-400km-after.jpg`.
- **Performance:** one extra texture fetch at an explicit mip level per fragment,
  taken only by patches that bind a local surface map. No frame-time A/B was run
  (the same limitation as TD-04), so no percentage is claimed.
- **Scientific validation:** the reference is the measured overview, whose mean
  reflectance the Sentinel mosaic agrees with after TD-03's gain (0.019 linear
  against 0.017). The local map is now a modulation of that measurement rather
  than an independent reflectance estimate; its absolute values are deliberately
  no longer used for tone.
- **Risks:** the map's absolute reflectance is no longer used, so a locally
  distinct material inside one overview texel is carried by chromaticity and the
  patch mean rather than by a brightness ratio; clamping at 1 is the only
  departure from exact mean matching; the mean depends on the mip chain, and a
  parity test pins the sampled level to the map resolution.
- **Completion evidence:** MET. The near-ground step is gone (4.36 → 2.01
  against 2.03 at 40 km), the `albedo` view did not move, and the remaining
  altitude difference (1.84–1.85 at 300–400 km) is the longer air column.

### TM-03 — Land/sea mask and imagery reconciliation
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Ensure the water mask, coastline and imagery agree at the same
  place.
- **Dependencies:** TD-07.
- **Subsystems:** terrain source, imagery, water.
- **Expected files:** `imagery_package.rs`, `terrain_source/catalog.rs`.
- **Acceptance:** a control transect crossing the coast shows water exactly where
  the DEM is below the datum within one texel of the imagery coastline.
- **Tests:** transect test on a fixed site.
- **Visual evidence:** 3 km coastal frame.
- **Performance:** none.
- **Scientific validation:** each dataset's datum and resolution documented.
- **Risks:** matching a 2 km overview to a 900 m DEM is approximate by nature.
- **Completion evidence:** pending.

---

## Phase P4 — Procedural vegetation

### VG-01 — Vegetation distribution audit
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Establish where vegetation should exist and whether the current
  density signal produces it.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** vegetation placement, land cover.
- **Expected files:** `src/domain/services/vegetation.rs`,
  `terrain/surface/scatter.rs`.
- **Acceptance:** density is derived from land/sea, slope, elevation, biome and
  moisture, with a deterministic seed and generator version.
- **Tests:** determinism across runs and LODs; coastal exclusion.
- **Visual evidence:** ground and 3 km frames with density overlay.
- **Performance:** instance budget respected.
- **Scientific validation:** measured land cover where available, procedural
  fallback labelled as such.
- **Risks:** sparse coverage reading as bare ground.
- **Completion evidence:** pending; active OpenSpec change
  `stabilize-far-field-tree-placement` is nearly complete.

### VG-02 — Near-field canopy geometry
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Replace flat tree cards with volumetric canopies at close range.
- **Dependencies:** VG-01.
- **Subsystems:** vegetation mesh, instancing.
- **Expected files:** `terrain/surface/mod.rs`, `assets/shaders/vegetation.wgsl`.
- **Acceptance:** canopy silhouette and internal shading read as volume at 50 m;
  impostors take over beyond the near band.
- **Tests:** LOD handover; instance counts within budget.
- **Visual evidence:** ground frame at 50 m and 300 m.
- **Performance:** near-band instance cap measured.
- **Scientific validation:** species shapes documented as procedural.
- **Risks:** instance explosion; shader cost.
- **Completion evidence:** pending.

### VG-03 — Foliage shading and GPU wind
- **Status:** PLANNED
- **Priority:** P3
- **Objective:** Give foliage translucency and wind response driven by the same
  wind field as the clouds.
- **Dependencies:** VG-02, CL-02.
- **Subsystems:** vegetation shader, wind field.
- **Expected files:** `assets/shaders/vegetation.wgsl`, cloud/weather assets.
- **Acceptance:** wind direction and speed match the deck's advection; foliage
  shading shows back-lit translucency.
- **Tests:** wind phase determinism; no per-frame instance rebuild.
- **Visual evidence:** short sequence at fixed pose.
- **Performance:** vertex cost measured.
- **Scientific validation:** wind source labelled measured or procedural.
- **Risks:** visual wind disagreeing with cloud motion.
- **Completion evidence:** pending.

---

## Phase P5 — Clouds and wind

### CL-01 — Coverage is a share of sky
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** Make the deck cover the share of sky the measured climatology
  states, instead of a blanket.
- **Dependencies:** none.
- **Subsystems:** cloud density, weather map.
- **Expected files:** `assets/shaders/cloud_density.wgsl`,
  `src/domain/services/cloud_noise.rs`.
- **Acceptance:** rank-equalised weather channels thresholded at `1 - fraction`;
  measured cloud share matches the climate coverage within tolerance.
- **Tests:** weather-map uniformity; coverage invariant.
- **Visual evidence:** 1,200 km frame; measured cloud share 0.261 at that pose.
- **Performance:** unchanged.
- **Scientific validation:** coverage source documented (measured climate package).
- **Risks:** per-regime scaling broke the invariant once and was rejected.
- **Completion evidence:** commit `309ac09`.

### CL-02 — Layer-scaled cells and wind streets
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Size convective cells from the layer depth and stretch them along
  the measured wind.
- **Dependencies:** CL-01.
- **Subsystems:** cloud density, wind.
- **Expected files:** `assets/shaders/cloud_density.wgsl`,
  `src/infrastructure/bevy_adapters/rocket/planet.rs`.
- **Acceptance:** `noise_scale = 1/(depth*6)`; stretch 2.4 along the wind; the deck
  and its shadows use the same axis.
- **Tests:** shader parity guard for the wind axis.
- **Visual evidence:** 1,200 km frame; cells read as cumulus rather than popcorn.
- **Performance:** unchanged.
- **Scientific validation:** wind from the measured climate package.
- **Risks:** cell size tied to layer depth breaks if the layer is retuned.
- **Completion evidence:** commit `7fac7df`.

### CL-03 — Sun path length through the deck
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** March the sun path far enough to leave the slab at any elevation.
- **Dependencies:** none.
- **Subsystems:** cloud scattering.
- **Expected files:** `assets/shaders/clouds.wgsl`.
- **Acceptance:** path = `layer / clamp(sin(elevation), 1/6, 1)`.
- **Tests:** `cloud_sun_path_reaches_the_top_of_the_slab`.
- **Visual evidence:** 1,200 km A/B; 11 % of deck pixels moved 1–5 % (mean
  0.78/255) at the noon capture.
- **Performance:** unchanged at zenith; up to 6× samples at 10° elevation.
- **Scientific validation:** the correct single-scatter path, not a look.
- **Risks:** cost grows at low sun.
- **Completion evidence:** commit `1734f1a`.

### CL-04 — Aerial perspective composite
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Composite sky haze into the deck instead of attenuating radiance.
- **Dependencies:** CL-03.
- **Subsystems:** cloud shader, sky.
- **Expected files:** `assets/shaders/clouds.wgsl`,
  `assets/shaders/sky_radiance.wgsl`.
- **Acceptance:** `T·L_cloud + (1−T)·opacity·L_haze`, haze sampled in the mirrored
  sky direction, optical depth vertical τ ÷ view cosine capped at 12.5 air masses.
- **Tests:** `cloud_shader_composites_the_shared_sky_into_the_deck`.
- **Visual evidence:** limb frames.
- **Performance:** one extra sky sample per fragment.
- **Scientific validation:** the composite is the physical form; the earlier
  radiance-only attenuation reddened the limb and was reverted.
- **Risks:** sky model has no clouds, so a haze of a partly cloudy sky is a mean.
- **Completion evidence:** commit `23fe1a4`.

### CL-05 — Cloud shape and type diversity
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Give the deck more than one cell size and type within a regime.
- **Dependencies:** CL-01, CL-02.
- **Subsystems:** cloud density.
- **Expected files:** `assets/shaders/cloud_density.wgsl`,
  `src/infrastructure/bevy_adapters/rocket/planet.rs`.
- **Acceptance:** a size spectrum blended by a noise selector, seamless at tile
  edges; coverage invariant unchanged (0.394 ± 0.02 bright-cloud share on the
  chase view).
- **Tests:** coverage invariant; tiling continuity.
- **Visual evidence:** 1,200 km and chase frames.
- **Performance:** must not add a noise fetch in the density loop.
- **Scientific validation:** procedural, labelled procedural.
- **Risks:** the earlier per-sample coordinate shear produced swirls and was
  rejected; the selector must change amplitude, not sample position.
- **Completion evidence:** pending.

### CL-06 — Edge structure
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Replace the salt-and-pepper cloud edges with resolved erosion.
- **Dependencies:** CL-05.
- **Subsystems:** cloud density.
- **Expected files:** `assets/shaders/cloud_density.wgsl`,
  `assets/shaders/clouds.wgsl`.
- **Acceptance:** edge ramp ≈ `coverage * 0.25`; bright-cloud share held.
- **Tests:** coverage invariant.
- **Visual evidence:** 400–1,200 km frames.
- **Performance:** measured; edge work must stay inside the march budget.
- **Scientific validation:** none; presentation.
- **Risks:** sharpening without a top-height field was already rejected once.
- **Completion evidence:** pending.

---

## Phase P6 — Atmosphere

### AT-01 — Single-scattering sky
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Physically informed Rayleigh/Mie sky with optical depth and Sun
  angle shared by the ground, sea and deck.
- **Dependencies:** none.
- **Subsystems:** sky, lighting.
- **Expected files:** `assets/shaders/sky.wgsl`,
  `sky_radiance.wgsl`, `src/infrastructure/bevy_adapters/rendering/sky.rs`.
- **Acceptance:** one `SkyParams` authority; the sea reflects the sky it is drawn
  with.
- **Tests:** sky parameter defaults; airless-body guard; sea reflection parity.
- **Visual evidence:** the 10/30/70/200/400 km altitude sequence and the 200/400
  km limb frames from AT-02 (`2026-11-05-at02-*`), which exercise the same
  `SkyParams` from sea level to vacuum.
- **Performance:** full-screen march; cost not isolated.
- **Scientific validation:** coefficients from the documented atmosphere model.
- **Risks:** treated as verified on sight rather than measured.
- **Completion evidence:** the altitude sequence that was missing now exists;
  one parameter block still drives the dome and the sea (parity test).

### AT-02 — Altitude transition and limb
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Continuous ground → stratosphere → space transition with a thin,
  correctly lit limb at orbital altitude.
- **Dependencies:** AT-01, P0-03.
- **Subsystems:** sky, atmosphere model.
- **Expected files:** `assets/shaders/sky.wgsl`,
  `src/domain/services/atmosphere.rs`.
- **Acceptance:** density/pressure/temperature drive the visual; no altitude colour
  switch; limb thickness agrees with the exponential scale height.
- **Tests:** continuity at 10/30/70/200 km sample points
  (`atmospheric_presentation_follows_the_table_across_the_atmosphere`) and
  pitch/FOV parsing plus a 90° pitch-is-horizontal test in `capture_view.rs`.
- **Visual evidence:** `2026-11-05-at02-sky2-10/30/70/200/400km.jpg` (same +2°
  view at each altitude), `2026-11-05-at02-limb-200/400km-fov6.jpg`, and the
  linear-light `2026-11-05-at02-limb-200/400km-linear.jpg`.
- **Measurement:** the capture path now accepts
  `COSMIC_SYSTEMS_CAPTURE_PITCH_DEG` (0 = nadir, 90 = horizon) and
  `COSMIC_SYSTEMS_CAPTURE_FOV_DEG`, so the sub-degree limb can be framed. The
  6° limb captures were mapped row → tangent altitude with
  `f = (H/2)/tan(FOV/2)`, `α = pitch + atan((H/2 − y)/f)`,
  `t = (R + h)·sin α − R`, and the linear-light luminance was fitted against `t`.
  **Measured in linear light** (a `height` diagnostic capture switches the camera
  to `Tonemapping::None`; only the terrain is false-coloured, the sky keeps its
  production shading), the two limbs e-fold at **8.30 km (200 km)** and
  **8.54 km (400 km)** over 24–32 km tangent altitude — **4–7 % above the 8.0 km
  Rayleigh scale height** and within 3 % of each other. Over 20–30 km the fit is
  9.7–9.9 km, and over 28–36 km it steepens to 5.2–5.8 km: exactly the curvature
  the same optics model predicts once the ozone band and Mie term are included.
  A Rayleigh-only run of that model e-folds at **8.00 km**. The displayable limb
  ends at 35 km (200 km camera) and 38 km (400 km camera), about 4.4–4.8 scale
  heights: a thin band, not a shell. The tonemapped captures fit 8.76/8.87 km
  over the same band — the same profile through the filmic transform.
- **Continuity evidence:** the +2° horizon luminance falls monotonically
  0.138 → 0.073 → 0.026 → below the 8-bit floor at 10/30/70/200 km; no step.
- **Nadir-darkness attribution (measured, no code change):** at `-4.0,138.0`
  linear-luminance p50 of the ground is 0.0286 at 30 km, 0.0304 at 200 km and
  0.0333 at 400 km against 0.0337 at 3 km (TM-04 after). The 30 km centre crop
  is 0.0337 against 0.0358 at 3 km (−6 %). The 10–100 km "veil" is therefore a
  framing/albedo impression — a frame filled with dark tropical forest and no
  bright sky — not an altitude-dependent atmospheric layer.
- **Open appearance note:** the low-luminance limb tail at 30–70 km reads
  pink/teal because the model is single-scattering only; the orbital limb itself
  is blue. Tracked as AT-04, not hidden here.
- **Performance:** the sky march is 16 view × 8 sun samples per pixel at every
  altitude; the cost is altitude-independent by construction. It was not
  separately timed because no window/telemetry path is available on this
  machine (P0-04 blocked); this is a reasoning statement, not a measurement.
- **Scientific validation:** atmosphere table documented with source in
  `src/domain/services/atmosphere.rs`; Rayleigh 8 km / Mie 1.2 km / ozone 25 km
  in `atmospheric_optics.rs`.
- **Risks:** a large blue shell or a flat plate.
- **Completion evidence:** captures, profile fits and the continuity test above.

### AT-03 — High-altitude land darkness
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Explain why land reads nearly black at 400 km while the deck is
  white, and fix whichever term is wrong.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** terrain albedo, lighting, atmosphere.
- **Acceptance:** land albedo at 400 km is consistent with the same surface at
  3 km after atmosphere and Sun angle are accounted for.
- **Attribution (measured, no code change):** the altitude term is not wrong.
  At `-4.0,138.0` the same ground renders at the **same** albedo at 400 km and at
  3 km — linear luminance p50 **0.0169 both** — so mip filtering, atmosphere and
  the lit path preserve the surface tone. Baked occlusion is neutral (self-shadow
  and sky visibility both exactly 1.000, p5…p95) on that ground, and the
  production/albedo ratio at altitude (1.77 at 300 km, 2.07 at 400 km) is the
  lighting factor, consistent between the two poses.
  Land reads dark because tropical-forest albedo in the Blue Marble overview is
  dark: 0.017 linear, about 4 % reflectance. A white deck at ~0.9 is genuinely
  ~50× brighter per unit area; the frame is not underexposed.
  The *remaining* observation is at the near end, not the far end. At the same
  pose, production land is 0.0742 linear at 3 km against 0.0333 at 400 km — a
  2.2× step on descent — while the albedo view is identical (0.0169 at both). The `inputs` and
  `layer-blend` views at 3 km confirm what is different there: local source
  surfaces and the layered ground material are both active (`(255,255,171)`,
  i.e. LOCAL_SURFACES + LAYER_WEIGHTS + OCCLUSION, no imagery) and both fade out
  with view distance, so the near-ground material replaces the overview with
  albedo about **2–2.5×** brighter on the same ground. That is a material-tone
  calibration question, scoped as **TM-04** below rather than fixed here; TM-04
  later measured the local map at 4× the overview's luminance and closed it.
- **Tests:** diagnostic capture comparison (`albedo`, `occlusion`, `inputs`,
  `layer-blend` at the same pose).
- **Visual evidence:** `2026-11-01-at03-prod-400km.jpg`,
  `2026-11-01-at03-albedo-400km.jpg`, `2026-11-01-at03-albedo-3km.jpg`,
  `2026-11-01-at03-prod-3km.jpg`, `2026-11-01-at03-inputs-3km.jpg`.
- **Performance:** none.
- **Scientific validation:** the albedo comparison is between two renders of the
  same measured overview, not a claim about the surface's true reflectance.
- **Risks:** the near/far ratio is an inference from two poses at different
  latitudes (≈4° of Sun elevation) and different air columns, so it is quoted as
  a range, not a calibrated constant.
- **Completion evidence:** attribution complete and the acceptance met: the
  altitude-side albedo is identical at 400 km and 3 km. The near-ground
  brightness step is a separate, newly scoped task with its own measurement.

### AT-04 — Multiple-scattering horizon hue
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** The 10–70 km sky reddens too much near the horizon (tan at
  10 km, pink at 30 km) and the faint upper limb tail reads teal because the
  model integrates single scattering only. Add a bounded multiple-scattering
  term so the noon horizon stays pale blue-white and the limb tail follows,
  without changing the 8 km limb thickness.
- **Dependencies:** AT-02 (verified).
- **Subsystems:** sky, atmosphere optics.
- **Expected files:** `assets/shaders/sky_radiance.wgsl`,
  `src/infrastructure/bevy_adapters/rendering/sky.rs`.
- **Acceptance:** at 10/30/70 km the +2° horizon hue is consistent with a
  multiple-scattering daylight sky, and the 24–32 km limb e-folding stays within
  8 ± 1 km.
- **Tests:** an analytic energy bound for the approximation; reuse the AT-02
  limb profile measurement as the regression.
- **Visual evidence:** repeat `2026-11-05-at02-sky2-*` and `*-fov6`.
- **Performance:** bounded additional samples in the existing march, counted.
- **Scientific validation:** cite the approximation and its assumptions.
- **Risks:** a second unofficial sky model; cost.
- **Completion evidence:** pending.

---

## Phase P7 — Rocket attitude and VFX

### RK-01 — Attitude, staging and model-axis audit
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Prove the rendered vehicle follows the authoritative attitude and
  that a detached stage keeps its own body frame.
- **Dependencies:** none.
- **Subsystems:** rocket dynamics, presentation, separation.
- **Expected files:** `src/application/rocket_spawning.rs`,
  `rocket/separation.rs`, `rocket/presentation.rs`.
- **Acceptance:** body → render transform is a pure frame conversion; no late
  overwrite; detached stages inherit position and attitude, not the parent.
- **Tests:** attitude mapping and separation transform tests.
- **Visual evidence:** prelaunch, ascent and staging frames with a body-axis
  overlay.
- **Performance:** none.
- **Scientific validation:** the physical attitude is the source of truth.
- **Risks:** a visual correction hiding a physical error.
- **Completion evidence:** pending.

### RK-02 — Plume driven by propulsion state
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Drive plume shape from thrust, mass flow, throttle, ambient
  pressure and altitude instead of a static cone.
- **Dependencies:** RK-01.
- **Subsystems:** propulsion, effects.
- **Expected files:** `src/infrastructure/bevy_adapters/rocket/effects.rs`,
  `propulsion.rs`, `assets/shaders/`.
- **Acceptance:** plume expands with altitude, separates by stage, and disappears
  at zero throttle.
- **Tests:** plume parameter mapping from propulsion state.
- **Visual evidence:** sea-level and altitude burn frames.
- **Performance:** effect count and update cost measured (existing telemetry
  reports both).
- **Scientific validation:** not CFD; documented as a real-time approximation.
- **Risks:** matching a look instead of the state.
- **Completion evidence:** pending.

---

## Phase P8 — Rendering optimisation

### RD-01 — Resource invalidation audit
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Stop recreating meshes, materials and buffers for unchanged
  inputs.
- **Dependencies:** P0-04.
- **Subsystems:** terrain render, materials, streaming.
- **Expected files:** `terrain/render.rs`, `terrain/streaming.rs`,
  `performance_systems.rs`.
- **Acceptance:** a stationary frame performs zero mesh, material and image
  creation; measured over 600 frames.
- **Tests:** counter-based regression test.
- **Visual evidence:** not applicable.
- **Performance:** per-frame allocation counters before/after.
- **Scientific validation:** none.
- **Risks:** caching a stale resource and showing the wrong thing.
- **Completion evidence:** pending.

### RD-02 — Streaming and culling measurement
- **Status:** IMPLEMENTED
- **Priority:** P2
- **Objective:** Quantify horizon/frustum culling, LOD reconciliation and upload
  cost separately.
- **Dependencies:** P0-04.
- **Subsystems:** streaming, performance.
- **Expected files:** `terrain/streaming.rs`, `performance_systems.rs`.
- **Acceptance:** the three costs are reported as separate timings.
- **Tests:** telemetry fields present and bounded.
- **Visual evidence:** not applicable.
- **Performance:** this task produces the numbers.
- **Scientific validation:** none.
- **Risks:** nested spans reported as additive.
- **Completion evidence:** bounded streaming exists and holds its budget in the
  2026-09-11 audit; the split is not implemented.

### RD-03 — Shadow resource lifecycle
- **Status:** VERIFIED
- **Priority:** P2
- **Objective:** Remove the repeated missing-shadow-map warning and confirm
  stability.
- **Dependencies:** none.
- **Subsystems:** terrain lighting, occlusion.
- **Expected files:** `terrain/render.rs`,
  `docs/shadow_stability_investigation.md`.
- **Acceptance:** occlusion images stay main-world resident; no warning spam; a
  stationary scene is pixel-identical across shadow toggles.
- **Tests:** `refreshable_occlusion_images_stay_main_world_resident`,
  `evicted_occlusion_map_rebinds_materials_to_neutral`.
- **Visual evidence:** 90-pose sweep; repeated poses pixel-identical.
- **Performance:** no per-frame regeneration.
- **Scientific validation:** none.
- **Risks:** cascade blending softness remains a quality issue, not a lifecycle
  bug.
- **Completion evidence:** investigation document + tests.

---

## Phase P9 — Final validation

### VL-01 — Three-mode smoke at the current commit
- **Status:** BLOCKED
- **Priority:** P0
- **Objective:** `cargo run`, `cargo run -- craft`, `cargo run -- rocket` each run
  for 25 s with zero shader/panic/wgpu errors.
- **Dependencies:** a presentable window (environment).
- **Subsystems:** all.
- **Expected files:** none.
- **Acceptance:** three exit codes 124 (timeout), zero error matches in each log.
- **Tests:** not applicable.
- **Visual evidence:** one frame per mode.
- **Performance:** not applicable.
- **Scientific validation:** not applicable.
- **Risks:** the presentation failure on this machine masks a real regression.
- **Completion evidence:** last successful run at `fb686ba`; **not** re-run at
  `d89e7d6`.

### VL-02 — OpenSpec strict validation
- **Status:** VERIFIED
- **Priority:** P1
- **Objective:** Keep every spec and change valid and consistent.
- **Dependencies:** none.
- **Subsystems:** OpenSpec.
- **Expected files:** `openspec/`.
- **Acceptance:** `openspec validate --all --strict` passes.
- **Tests:** 42/42 items at `d89e7d6`.
- **Visual evidence:** not applicable.
- **Performance:** not applicable.
- **Scientific validation:** not applicable.
- **Risks:** specs drifting from behaviour; the roadmap is not the requirement.
- **Completion evidence:** 2026-11-01 run, 42 passed / 0 failed.

### VL-03 — In-repo visual acceptance suite
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** One reproducible frame per required scene, committed with its
  measurement, so a regression is visible in the diff.
- **Dependencies:** P0-02.
- **Subsystems:** measurement.
- **Expected files:** `docs/progress/evidence/`, `VISUAL_EVIDENCE.md`.
- **Acceptance:** the ten required scenes each have a current frame at a stated
  commit and pose, with a pass criterion.
- **Tests:** a script that re-captures and compares.
- **Visual evidence:** the matrix in `VISUAL_EVIDENCE.md`.
- **Performance:** not applicable.
- **Scientific validation:** not applicable.
- **Risks:** committing stale frames as if current — every row must carry its
  commit.
- **Completion evidence:** 7 rows present, 3 scenes missing.
