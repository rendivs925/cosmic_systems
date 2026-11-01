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
- **Status:** IMPLEMENTED
- **Priority:** P0
- **Objective:** Make the ground under a facility genuinely level for mesh,
  collision and radar, instead of drawing a flat apron over a slope.
- **Dependencies:** none.
- **Subsystems:** terrain composition, launch site presentation.
- **Expected files:** `src/domain/services/terrain_source/layered.rs`,
  `catalog.rs`.
- **Acceptance:** inside the flat radius the composed height equals the site's own
  composed height for `height_m` and `mesh_height_m`; outside the blend it equals
  the ungraded surface; the site's own elevation is unchanged.
- **Tests:** `graded_site_is_level_under_the_pad_and_returns_to_natural_terrain`,
  `a_source_without_sites_is_unchanged`.
- **Visual evidence:** Papua ground frame at `d89e7d6` shows the pad flush with no
  floating or buried edge. **Only Papua was captured**; Starbase, Baikonur,
  Vandenberg, Wallops, Kourou, Mahia and KSC were not.
- **Performance:** 125 extra terrain samples per site at startup, once.
- **Scientific validation:** the grade is a terrain-authority property, so the
  rendered mesh and collision cannot disagree.
- **Risks:** grading reclassifies slope-derived material (see TM-01); a site whose
  DEM is pathological could produce a steep blend.
- **Completion evidence:** commit `5901c82`; RTLS landing assertion corrected to
  the actual landing geometry in the same commit. **Remaining for VERIFIED:** one
  capture per predefined site.

### TD-03 — Regional imagery colour match to the global overview
- **Status:** IMPLEMENTED
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
  `color_gain_is_scoped_to_the_region`.
- **Visual evidence:** **none yet.** The gain is validated numerically (means
  match by construction) but no before/after frame was captured.
- **Performance:** one pass over 256×256 texels per tile install; unmeasured, but
  bounded by the 4-tile-per-frame admission limit.
- **Scientific validation:** gain = overview mean / regional mean in linear light,
  both means recorded in the manifest with the method and source checksums.
- **Risks:** matching the mean is not matching the distribution; a mosaic with a
  different contrast still reads slightly different.
- **Completion evidence:** commit `aca26dd`.

### TD-04 — Imagery edge feather
- **Status:** PLANNED
- **Priority:** P1
- **Objective:** Fade the regional inset into the overview across the region edge
  so the last patch boundary stops stepping between 10 m and 2 km imagery.
- **Dependencies:** P0-03 (satisfied), TD-03.
- **Subsystems:** terrain material uniform, imagery manifest regions.
- **Expected files:** `assets/shaders/terrain_surface.wgsl`,
  `src/infrastructure/bevy_adapters/terrain/render.rs`,
  `src/domain/value_objects/imagery_manifest.rs`.
- **Acceptance:** `imagery_weight` is multiplied by a `smoothstep` over a band
  inside the region bounds, evaluated from the fragment's body position; the
  transition is independent of where patch boundaries fall.
- **Tests:** uniform parity; feather math at the boundary, centre and outside.
- **Visual evidence:** 40 km and 400 km frames at the P0-01 pose, before/after.
- **Performance:** two dot products per fragment; measured delta < 1 %.
- **Scientific validation:** the feather is presentation only; it must not move
  the region's data or its checksums.
- **Risks:** adding a uniform field can break the material if the layout drifts —
  never land this without the parity test.
- **Completion evidence:** pending.

### TD-05 — Rectangular albedo blocks at altitude
- **Status:** IN_PROGRESS
- **Priority:** P0 (largest visible defect)
- **Objective:** Attribute the axis-aligned tone blocks at 300–1,200 km to a
  specific term and remove them.
- **Dependencies:** P0-03 (now satisfied).
- **Subsystems:** terrain material, imagery, LOD, elevation data, water material.
- **Expected files:** `assets/shaders/terrain_surface.wgsl`,
  `src/infrastructure/bevy_adapters/terrain/render.rs`,
  `src/domain/services/terrain_source/*` or the elevation payload, depending on
  the outcome.
- **Acceptance:** at the P0-01 pose no block boundary is visible that does not
  correspond to a real albedo change within ±1/255 of the surrounding trend.
- **Tests:** a capture-pair comparison harness that reports the maximum
  adjacent-pixel step across patch boundaries.
- **Visual evidence:** five differentials. The earlier three — the blocks survive
  a seven-hour Sun change, 90 vs 400 frames, and appear at 300 km and 1,200 km —
  came before diagnostics existed. P0-03 added two more at 1,200 km:
  * `2026-11-01-diag-albedo-1200km.jpg` contains **no rectangular block**, so the
    geographic base albedo is not the source at that pose.
  * `2026-11-01-diag-land-sea-1200km.jpg` and `2026-11-01-diag-height-1200km.jpg`
    show a rectangular region (screen x ≈ 680–780, y ≈ 140–390 at that pose)
    sitting within ±2 m of the sea-level datum while the surrounding shelf is
    tens of metres below it. Its edges are straight and image-axis aligned, and
    `inputs` reports no imagery, layers or local surfaces at that altitude.
  The leading hypothesis is therefore a **rectangular region of near-zero
  elevation data**, not a material or shader term: a water cap drawn over a bed
  recorded as ~0 m instead of ~−50 m is a shallower, brighter rectangle. This is a
  hypothesis supported by three frames, not a confirmed cause.
- **Performance:** not applicable until attributed.
- **Scientific validation:** not applicable yet.
- **Risks:** attributing it to the wrong term and shipping a cosmetic noise layer
  over a data bug; the standing rule is that seams are fixed, never hidden. A
  second contributor is **not diagnosable with the current modes**: at 1,200 km
  most of the frame is ocean, drawn by `WaterMaterial`, which the terrain
  diagnostics do not cover. Ocean blocks therefore need a water-side view.
- **Completion evidence:** pending. Next step: sample the terrain authority along
  a transect crossing the rectangle's edge and compare it with the surrounding
  shelf, then locate the tile that supplies it.

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
- **Status:** IMPLEMENTED
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
- **Visual evidence:** present in every frame; no dedicated altitude A/B yet.
- **Performance:** full-screen march; cost not isolated.
- **Scientific validation:** coefficients from the documented atmosphere model.
- **Risks:** treated as verified on sight rather than measured.
- **Completion evidence:** code and tests exist; **needs an altitude sequence for
  VERIFIED**.

### AT-02 — Altitude transition and limb
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Continuous ground → stratosphere → space transition with a thin,
  correctly lit limb at orbital altitude.
- **Dependencies:** AT-01, P0-03.
- **Subsystems:** sky, atmosphere model.
- **Expected files:** `assets/shaders/sky.wgsl`,
  `src/domain/services/atmosphere.rs`.
- **Acceptance:** density/pressure/temperature drive the visual; no altitude colour
  switch; limb thickness agrees with the exponential scale height.
- **Tests:** continuity at 10/30/70/200 km sample points.
- **Visual evidence:** 10, 30, 70, 200 km and a limb view.
- **Performance:** measured per altitude band.
- **Scientific validation:** atmosphere table documented with source.
- **Risks:** a large blue shell or a flat plate.
- **Completion evidence:** pending.

### AT-03 — High-altitude land darkness
- **Status:** PLANNED
- **Priority:** P2
- **Objective:** Explain why land reads nearly black at 400 km while the deck is
  white, and fix whichever term is wrong.
- **Dependencies:** P0-03 (satisfied).
- **Subsystems:** terrain albedo, lighting, atmosphere.
- **Expected files:** `assets/shaders/terrain_surface.wgsl`,
  `rendering/sky.rs`.
- **Acceptance:** land albedo at 400 km is consistent with the same surface at
  3 km after atmosphere and Sun angle are accounted for.
- **Tests:** diagnostic capture comparison.
- **Visual evidence:** 300/400 km frames (already captured; the symptom is in
  `2026-10-31-300km-papua.jpg`).
- **Performance:** none.
- **Scientific validation:** not applicable until attributed.
- **Risks:** guessing between albedo, mip and lighting.
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
