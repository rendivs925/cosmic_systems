# Planetary realism audit — 2026-10-31

Status: **PHASE 0 (audit) plus the first correctness fix.** This document records
what was confirmed from source and deterministic captures, what was fixed and
measured, and what remains. It is deliberately explicit about which causes are
confirmed, which are hypotheses, and which were tested and rejected.

Baseline commit: `1734f1a` (the last validated cloud slice).

## Method

All visual comparisons use the deterministic capture harness rather than ad-hoc
screenshots, so a before/after pair is the same scene:

```text
COSMIC_SYSTEMS_CAPTURE_POSE="<lat>,<lon>,<altitude_m>" \
COSMIC_SYSTEMS_CAPTURE_TIME=<sim_s> \
  ./target/debug/cosmic_systems rocket --site papua
```

The pose fixes the observer over one body-fixed site and the time fixes the
cloud field, the planet rotation, the Sun direction and the measured climate
sample. Poses used here:

| id | pose | purpose |
| --- | --- | --- |
| ground | default chase camera, prelaunch | pad, tower, near terrain |
| 3 km | `-8.0,139.5,3000` | coastal lowland, water bodies, land cover |
| 400 km | `-4.0,138.0,400000` | coastline, regional imagery extent |
| 1200 km | `-8.0,139.5,1200000` | planet-scale albedo, tile boundaries |

Launch/chase captures that depend on a keypress are **not** reproducible across
runs because the key press lands at a different simulation time. Only the
pose-pinned site captures are valid for A/B work.

When the machine's Vulkan presentation path is broken (one present, then
`Couldn't get swap chain texture ... generic error`, or `No supported formats for
surface` on a virtual display) no frame can be captured at all. The offscreen
path removes presentation from the loop: it renders into an `Image` target and
writes a PNG with no window. See `docs/measurement/offscreen_capture.md`. Every
capture quoted below after the first two was taken through it; the UI/HUD is not
drawn in that path, so measure the world, not the overlay.

## Confirmed defects and their source-level causes

### 1. Polygonal coastlines and pale shoreline ribbons — CONFIRMED, FIXED

`water_mesh_for_patch` built the sea surface from the terrain mesh's own
vertices and placed every **dry** vertex on the terrain bed:

```rust
let depth = ocean_depth.max(lake_depth);
let water_radius = radius + depth;      // radius == bed radius when dry
```

So a triangle straddling the shore ramped the sea surface from the datum down
into the ground. Those ramps are what the eye read as the coastline: a hard
polygonal outline where the ramp ended, a pale band where the near-horizontal
ramp faced the sky at a grazing angle, and z-fighting where it met the bed. It
also explains why the "coastline" looked like a straight-edged polygon that did
not follow the imagery: it was a triangle boundary, not a waterline.

Fix: an ocean patch now presents a **sea-level sphere cap at the datum for every
vertex**. The opaque terrain in front of it cuts the visible water at the true
waterline, so the coast is resolved per pixel by the depth buffer instead of per
triangle by the mask. Inland lakes keep their own local surface, which is not at
the datum. Regression test:
`ocean_cap_stays_on_the_datum_across_a_sloped_shore`.

Measured on the deterministic 3 km capture, over the frame excluding the HUD:
pixels that are bright and desaturated (the pale ribbons: `lum > 0.62` and
`saturation < 0.14`) fell from **1.58 % to 0.40 %** of the frame while median
luminance stayed at 0.34, so the change removed the ribbons rather than dimming
the scene.

### 2. Rectangular albedo patches — CONFIRMED, HALF FIXED

`imagery_weight` is a **uniform per patch** value that switches from `0` to `1`
when a produced local imagery tile becomes ready:

```wgsl
let base_albedo = mix(global_albedo.rgb, imagery_albedo.rgb, imagery_weight);
```

The global overview is NASA Blue Marble (2 km, downsampled to 8k, a composited
true-colour product) and the regional detail is Sentinel-2 L2A TCI (10 m,
surface reflectance). Two different instruments, two different atmospheric
corrections, two different colour responses. A patch therefore *changes dataset*
at its boundary with no cross-fade and no colour matching, and the boundary is a
cube-sphere patch edge — a rectangle. The 1200 km capture shows this plainly as
darker and lighter rectangular blocks with straight edges over Borneo and the
Java Sea, and the 400 km capture shows one hard-edged rectangle near the bottom.

This is a data-fusion problem, not a shading bug. The fix has two parts:

1. **Colour match — done.** Measured over the region footprint, the Sentinel-2
   surface-reflectance mosaic is 2.5-4x the linear reflectance of the Blue Marble
   overview (per-channel means and method in
   `docs/datasets/earth_imagery_v1.md`). The manifest now declares the measured
   per-channel linear gain, and it is applied in linear light when a tile is
   installed, before its mip chain is built. A test asserts the declared gain is
   still the measured ratio, so reprocessing the mosaic without re-measuring
   fails the build.
2. **Spatially continuous feather — not done.** The last patch edge still steps
   between 10 m imagery and a 2 km overview. It needs the region footprint in the
   terrain-surface uniform and a `smoothstep` on the fragment's body position,
   because a per-patch uniform cannot feather across a patch boundary.

A caveat that matters: the measured brightness mismatch explains the *pale inset
around Papua*. It does **not** explain the large rectangular blocks seen at
1,200 km over Borneo and the Java Sea.

Three differential captures now constrain those blocks:

| test | result |
| --- | --- |
| same pose, Sun 7 h later | blocks unchanged; only the glint and shading move |
| 90 vs 400 rendered frames | blocks unchanged |
| 1,200 km vs 300 km over the site | blocks appear at both scales |

So they are albedo or geometry, not lighting, occlusion, or patches that have not
streamed in. They are not the regional imagery either: at 1,200 km no published
patch resolves to a regional tile (`detailed=0`), and at ground level the imagery
residency climbs to 70 tiles, so the regional path does work where it applies.
What is left is per-patch material state — the layered-versus-fallback ground
material, the per-patch occlusion map, or LOD-dependent texture sampling. The
diagnostic modes are the tool that will settle which; they are not built yet.

### 3. Launch facility reads as a detached slab — CONFIRMED, FIXED

The apron is a flat `Cuboid::new(64.0, 0.5, 64.0)` whose top is 0.05 m above the
sampled terrain height at the anchor, oriented to the terrain normal at the
anchor (`sync_launch_pad_presentation`). The terrain *sample* is smooth, but the
terrain *mesh* near the pad has ~38 m vertex spacing at the LODs the chase camera
uses, so over the 64 m footprint the rendered ground is two or three triangles
whose plane does not match the tangent at the centre. The slab ended up buried on
the uphill edge and floating on the downhill edge, with a visible shadow gap —
exactly what the ground capture showed. The water tanks, blockhouse and
floodlights inherited the mismatch because they are parented to the same plane.

`PadFlatZone` was not a fix: it suppresses only the *procedural detail* layer
(`ProceduralDetailSource::detail_m`), never the measured base, so the ground
under the pad stayed tilted.

**Rejected approach, measured.** The first fix built a cut-and-fill platform mesh
whose rim sampled `TerrainSource` and met the ground. It was written, tested and
captured, and it failed for a structural reason worth recording: the terrain
*mesh* interpolates the same source with much coarser triangles than the rim
sampling, so the platform and the rendered ground interpenetrated in irregular
wedges along the whole perimeter. A presentation mesh cannot agree with a coarse
terrain mesh, only with the source. Reverted.

**Chosen fix.** Grade the site in the terrain authority. `LayeredTerrainSource`
gained `with_graded_sites`: the *composed* height (base + macro + detail) is
pulled to the site's own composed height inside a flat radius and restored over a
blend, in both `height_m` and `mesh_height_m`. Mesh, collision, radar altitude
and the rendered surface therefore all see the same level platform, and the
apron sits flush because the ground under it is genuinely level. The anchor
height is sampled from the ungraded composition before grading is installed, so
the launch elevation and every physics baseline are unchanged.

All nine predefined sites are graded (KSC, LC-39A, Starbase, Vandenberg,
Wallops, Kourou, Baikonur, Mahia, Papua lowland), not just the three that had
detail zones. Grading pins a site to its own composed height, so adding a site
cannot move that site's launch elevation.

The flat/blend radii dropped from 750 m / 3000 m to **250 m / 700 m**. The old
750 m disc flattened a 1.5 km pancake around every site, which is why the chase
camera stood on a featureless wash; 250 m still contains the facility, its apron
and the ground the vehicle can ever touch, while restoring relief close by.
Regression tests:
`graded_site_is_level_under_the_pad_and_returns_to_natural_terrain`,
`a_source_without_sites_is_unchanged`.

**One regression assertion changed, and why.** `rtls_recovery_stage_hands_off_
and_lands_through_fixed_systems` asserted the landed stage was strictly `< 5.0 m`
from the pad point. `RocketPhysicsState.position_m` is the cylinder centre and
the stage's lower extent is 5 m, so a *perfectly vertical* touchdown is exactly
5.000 m from the pad. The old threshold only passed because the stage used to
settle a few centimetres off. With level graded ground it lands exactly on the
pad and reports 5.000 m. The assertion is now `<= 5.0 + 0.05` with the geometry
written down, and the failure message prints the measured miss. This is a
tightening of meaning, not a loosened tolerance.

### 3b. Synthetic relief was manufacturing coastline — CONFIRMED, MECHANISM READY, NOT ENABLED

Grading the site exposed a second defect that the large flat disc had been hiding:
the ±24 m / ~250 m-wavelength detail layer is signed, and on a coastal plain whose
measured base sits a metre or two above the sea datum it pushed half the plain
below sea level. The ocean cap then rendered those depressions as open water, so
the landscape grew angular lakes that were not in any dataset.

Mechanism: `TerrainDetailLayer::with_sea_level_guard`, applied identically in
`height_m` and `mesh_height_m` so collision and the rendered coast cannot
disagree. It is opt-in per layer, because only a body with an ocean can be
flooded. Unit test: `synthetic_detail_cannot_create_sea_near_the_datum`.

**Not enabled on Earth yet.** Turning it on perturbs
`safe_leo_insertion_matches_committed_baseline_within_tolerances`, and the cause
is not isolated: the KSC reference point is inside a pad flat zone, so the guard
should be a no-op there, yet the insertion baseline shifts. Enabling a change
that moves a committed physics baseline without understanding why is exactly
what the project forbids, so the mechanism ships inert and the isolation is the
first task of the next slice. The 250 m graded pad already suppresses the
fabricated lakes in the ground view; they can still appear 250 m out on a
near-sea-level plain.

### 3c. The graded pad leaves a visible pale disc — NEW, from the first clean ground frame

Grading the site (defect 3) is geometrically correct, but the first ground-level
frame from the offscreen path shows a **~700 m pale-green disc** centred on the
pad, with a soft edge, standing out against the surrounding darker green. It is
not a lighting artifact: flattening the ground changes the slope-derived ground
material blend and the baked occlusion, so the whole graded footprint reads as a
different surface.

That is a real side effect of the fix, not a camera or exposure problem.

Fixed by shrinking the footprint to what the facility needs: flat radius 250 m ->
110 m, blend 700 m -> 280 m. The first ground frame from the offscreen path shows
the disc gone, with consistent ground tone up to the pad edge and the site
reading as a clearing rather than a lighting artifact. The deeper cause — that
levelling ground reclassifies its slope-derived material — is still there and
would return for any larger graded area; keeping the classification stable is the
next step if the footprint ever grows again.

### 4. Near-site terrain is featureless — PARTLY FIXED

One cause was the 750 m / 3000 m pad flattening, now 90 m / 320 m (defect 3).
The remaining cause is structural: the detail layer is faded out entirely at
LOD ≤ 11 (`DetailLodFade::new(11, 14)`) and the terrain shader adds only *albedo*
patchiness below that, so geometric detail does not exist at distance. That is
the "distant terrain appears flat" observation and it is not yet addressed.

### 5. Clouds still read as painted cut-outs — CONFIRMED, partially addressed

At 400 km the deck is a set of white blobs with salt-and-pepper edges and no
internal shading; the Sun is high (elevation 0.97 at the pinned epoch) and the
visible tops are 93 % lit, so there is genuinely little top shading to recover.
The hard edges are the erosion threshold on a low-resolution density field, not a
lighting problem. This is the "shape/type diversity + edge structure" work that
was already queued before this audit; nothing in this slice changes it.

### 6. Terrain lighting at altitude is very dark — CONFIRMED (symptom), cause open

At 400 km the land is nearly black while the deck is white. The same surface
looks correctly lit at 3 km. Candidates, in order of suspicion: the albedo the
global overview supplies at those LODs, the 8k overview's own dark bias, and the
sun-angle term at that latitude. Not diagnosed yet; the diagnostic mode below is
the tool for it.

### 7. Repeated missing-shadow-map warnings — ALREADY FIXED, guarded

The warning came from a `RENDER_WORLD`-only occlusion image being released from
`Assets<Image>` after extraction, so the sun-driven refresh could never install
it. The bake is now main-world resident and
`refreshable_occlusion_images_stay_main_world_resident` pins it. A separate
stationary-scene study (`docs/shadow_stability_investigation.md`) found
pixel-identical captures across shadow toggles, so there is no flicker to fix;
what remains is cascade-transition softness, which is a quality item, not a
lifecycle bug.

### 8. Vegetation is sparse and flat — CONFIRMED (visual), not yet audited

Trees appear as isolated dark specks with no canopy volume. `vegetation_density`
is authoritative and defaults to bare, so distribution depends entirely on which
sources override it. Not audited this pass.

## Diagnostic modes requested by the brief

None of these exist as toggles yet. They are the right next investment because
several open questions above cannot be answered from a lit RGB frame — in
particular "is the dark high-altitude land albedo or lighting". The intended set
(developer-only, off by default): height, normal, UV, land/sea, surface class,
LOD level, tile identity, no-data, shadow visibility, terrain-source identity.

Until they exist, the false-colour fragment override used for the cloud probe is
the working technique: write a diagnostic value into `out.color`, capture at a
pinned pose, and read the statistics back. It is fragile (a value read before its
declaration silently kills the material) but it produced the sun-elevation and
visibility numbers this project now relies on.

## Build configuration defects found

`cargo check --no-default-features` did not compile: `plugins.rs` imported
`load_default_soil` and `load_default_surface_fields` unconditionally while both
are defined under `cfg(feature = "dem")`. The two imports are now gated like
`load_default_land_cover` already was. `cargo check` and `cargo test` now pass
without default features (1049 tests).

## What this slice changed

* `water_mesh_for_patch`: ocean cap stays on the datum; lakes keep their local
  surface.
* `LayeredTerrainSource::with_graded_sites`: launch sites are graded level in the
  composed surface, for mesh and collision alike.
* Pad flat/blend radii: 750/3000 m → 250/700 m.
* `TerrainDetailLayer::with_sea_level_guard`: mechanism and unit test, left
  inert pending the baseline isolation above.
* Imagery colour match: measured per-channel linear gain in the manifest, applied
  at tile install before mip generation.
* Offscreen capture path (`COSMIC_SYSTEMS_OFFSCREEN_CAPTURE`), which renders and
  writes a PNG with no window or swapchain.
* Seven regression tests plus this document.

## Known divergences that are deliberate, not bugs

* `mesh_height_m` drops the procedural detail at LOD ≤ 11 while `height_m` keeps
  it. The rendered patch is a representation of the physical surface, and the
  vehicle is always inside fine patches. The two paths agree wherever the
  vehicle can be, and the site grade is applied to both. This is the one place
  where "rendered height equals collision height" is a tolerance rather than an
  identity, and it is bounded by the detail envelope (±36/+24 m).

## Evidence limits of this slice

Deterministic captures were taken for the water fix, the launch-site grading and
the sea-level guard. The last capture after the radius change could not be taken:
the virtual display lost Vulkan presentability part-way through the session
(`MESA: vulkan: No DRI3 support detected - required for presentation`, then
`No supported formats for surface` from `bevy_render::view::window::
create_surfaces`). That is an environment failure, not a code failure — it
reproduces with a fresh `Xvfb` and with `xvfb-run`, and it fires before any
terrain or rendering code of this project runs — but it means the shipped
250/700 m radii are validated by unit tests and by the 90/320 m captures rather
than by a like-for-like final frame. No camera or scene parameter was changed to
work around it.

## Remaining work, in dependency order

1. Imagery edge feather (defect 2, remaining half), then diagnose the 1200 km
   blocks with the diagnostic modes; the colour match does not explain them.
2. Geographic detail at distance: a geometry detail band beyond LOD 11, or a
   normal-mapped far field, so the "flat distant terrain" reading improves
   (defect 4 remainder).
3. Diagnostic modes, then use them on the high-altitude darkness (defect 6).
4. Cloud shape/type diversity and edge structure (defect 5).
5. Vegetation audit and near-field canopy work (defect 8).
6. Regional DEM for the Papua coast, so the 900 m base stops producing angular
   water polygons at 3 km (defect 1's remaining data limit).
7. Performance measurement pass, which the brief requires and which has not been
   run against this baseline.

## 2026-11-01 — Diagnostic render modes (P0-03)

The audit above listed "diagnose the 1200 km blocks with the diagnostic modes" as
the next step, then later still put "diagnostic modes" in the queue. They now
exist: seventeen false-colour views, one per terrain input, selected by
`COSMIC_SYSTEMS_DIAGNOSTIC_MODE` or `Ctrl+U`/`Ctrl+I` in debug builds. Details and
the linear-to-byte encoding are in `docs/measurement/terrain_diagnostics.md`.

Design decisions worth recording:

* The mode is one `u32` in the **existing** terrain material uniform, and the
  per-patch inputs ride in the same uniform, so switching writes one value per
  material. Nothing is rebuilt — a test asserts the material asset id survives
  every mode change and that no second material appears.
* The diagnostic branch sits at the end of the fragment, after every material
  input is resolved, and returns before lighting. That makes each view show the
  value production actually used instead of a re-derived approximation, at the
  cost of paying for the PBR work in a diagnostic frame. That trade is deliberate:
  divergence between "what I measure" and "what I render" is the failure mode this
  tool exists to prevent.
* Water caps and the cloud deck are hidden while a mode is active (an
  opt-in marker on their spawn sites). A cap covers exactly the submerged terrain
  the land/sea view classifies; a deck covers continents at altitude.
* Camera tonemapping and bloom are saved, bypassed and restored, so the PNG holds
  the linear value encoded as sRGB rather than a display transform.
* The height view is a square-root ramp about the datum rather than a linear
  planetary ramp: on a 5,000 m linear span one 8-bit step is about 5 m, which
  cannot show the seam steps the mode exists to find.
* An unknown mode value renders production and logs the accepted values. A typo
  must not cost a capture run, and rendering diagnostic colours by accident would
  be worse.

First findings, recorded as observations rather than conclusions:

* At 300 km and 1,200 km, `inputs` reports **no imagery, no layer weights and no
  local surfaces** on any visible patch, and `occlusion` is neutral (1,1). The
  layered-material and occlusion-map hypotheses for the tone blocks are therefore
  ruled out at those altitudes, and `surface-class` is honestly magenta.
* At 1,200 km the `albedo` view carries **no rectangular block**, so the block is
  not in the sampled geographic albedo.
* At the same pose, `land-sea` and `height` show a rectangular region sitting
  within ±2 m of the datum while the surrounding shelf is tens of metres below
  it. `patch-identity` shows the rectangle cuts across a patch, so it is a data
  region bound, not a patch edge. Leading hypothesis: a rectangular area of
  near-zero elevation data, which a water cap then draws as a shallow, brighter
  rectangle.
* This does **not** cover the ocean blocks: at 1,200 km most of the frame is the
  water cap, drawn by `WaterMaterial`, which the terrain diagnostics do not
  instrument. A water-side view is needed before the ocean tone blocks can be
  attributed.

Limits, recorded so the next reader does not over-read a capture:

* Diagnostics cover streamed terrain patches only. The far-field planet proxy,
  ocean, clouds, vegetation, rock shells and rivers keep their own materials; a
  surface in production colours inside a diagnostic frame is a surface that is not
  streamed terrain, which is itself useful information.
* There is no terrain-source identity mode, because the fragment shader cannot see
  the terrain authority. `inputs` reports what the material actually binds.
* Native frame time with a mode enabled is unmeasured (P0-04 remains blocked).

## 2026-11-01 — Attributing the altitude tone blocks (TD-05)

The diagnostic modes were run as a complete matrix at the failing pose
(`-8.0,139.5,1200000`, 42 s, 200 frames), which is the first time every terrain
input was visible at the same instant. Three results matter.

**The land blocks are per-patch constant roughness.** `roughness` renders as a
quilt of patch-shaped blocks because `surface.roughness` is one scalar per patch,
sampled at the patch centre from `surface_appearance`. At 300 km the same view
turns the coastline into a patch-sized staircase between 0.25 (shallow seafloor)
and 0.85 (land). To confirm, `patch_material` was rebuilt with a constant
`(0.85, 0.0)` — one line, no other change — and the same pose, clock and frame
count re-captured. A 12.2/255 step across a patch boundary fell to 4.1/255
(median luminance across the edge: 50.4→62.6 became 50.5→54.6). That is the
land component of the defect, measured rather than argued.

**The "datum rectangle" is not a defect.** The earlier hypothesis — a rectangular
region of near-zero elevation data — was wrong. A transect tool
(`terrain_authority_transect`, committed with this change) samples every
composition layer of the Earth authority offline and reproduces the in-engine
values exactly at fixed points (−13.61 m at −6.0,138.0; +7.90 m at −6.0,138.6).
Every mesh vertex also matches the source to within the 5 m skirt, and forcing
the CDLOD `morph_factor` to zero leaves the pond field unchanged. The lowland
around the site genuinely sits within a few metres of the datum with shallow
depressions; `height` and `land-sea` are reporting it faithfully. Contour
darkening in the height view exaggerates it, which is why it looked like a
rectangle rather than an undulating coastal plain.

**The ocean blocks are not roughness.** The constant-roughness A/B leaves
deep-water large-scale variation unchanged (std 21.10 → 21.05 over 8 bits). The
sea is `WaterMaterial`, outside the terrain diagnostics. The `height` view does
show rectangular seafloor regions that the water's 0–60 m depth ramp would turn
into colour steps, which is the first place a water-side view should look.

Ruled out along the way, all with a frame: normals (continuous), UV0/UV1
(continuous), baked occlusion (uniform 1,1), per-patch input differences (none at
this altitude), LOD layout (levels 4–6, seams coincide with the boundary view),
and the LOD morph (A/B above).

Method note for the next reader: the A/B frames are marked *modified build* in
`docs/progress/VISUAL_EVIDENCE.md` and the exact one-line change is recorded
next to them. They are measurements, not shipping behaviour.
