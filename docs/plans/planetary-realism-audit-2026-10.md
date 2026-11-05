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

### 6. Terrain lighting at altitude is very dark — CLOSED (albedo), with one appearance note

At 400 km the land is nearly black while the deck is white. The same surface
looks correctly lit at 3 km. The albedo diagnosis (AT-03) found the altitude path
correct: the same ground measures the same albedo at 400 km and 3 km, tropical
forest really is ~4 % reflectance, and the near-ground material was the 2.2×
outlier because it carried absolute local albedo; TM-04 replaced that with a tint
anchored to the measured overview (descent +2 % instead of +128 %).

AT-02 then measured the residual "10–100 km veil" at `-4.0,138.0`: ground
linear-luminance p50 is 0.0286 at 30 km, 0.0304 at 200 km and 0.0333 at 400 km
against 0.0337 at 3 km, and the 30 km centre crop is 0.0337 against 0.0358
(−6 %). There is no altitude-dependent layer; the dark impression is a nadir
frame filled with dark forest and no bright sky.

One appearance note remains and is tracked as **AT-04**: at 10–30 km the ground
hue shifts from the local green to the far-field brown-olive at similar
luminance, and the single-scattering sky reddens the horizon (tan at 10 km,
pink at 30 km). Neither is an altitude *lighting* failure; both are material and
sky-model fidelity, recorded rather than hidden.

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
  attributed. *(Superseded 2026-11-01: the water views exist and refute the
  ocean half — see "Water-side diagnostics" below.)*

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
*(Superseded 2026-11-01: the depth ramp is indeed where the sea's tone comes
from, but it is continuous — no water term carries a patch signal. The deep-water
A/B above only ever measured opaque water.)*

Ruled out along the way, all with a frame: normals (continuous), UV0/UV1
(continuous), baked occlusion (uniform 1,1), per-patch input differences (none at
this altitude), LOD layout (levels 4–6, seams coincide with the boundary view),
and the LOD morph (A/B above).

Method note for the next reader: the A/B frames are marked *modified build* in
`docs/progress/VISUAL_EVIDENCE.md` and the exact one-line change is recorded
next to them. They are measurements, not shipping behaviour.

## 2026-11-01 — Per-vertex roughness (TD-05, land half)

The attribution said the land tone blocks were a per-patch constant, so the fix
removes the constant rather than hiding it. Roughness is now evaluated **per
vertex** by `surface_roughness(elevation, zone_lat, slope_deg)`, the same law
`surface_appearance` calls, and carried to the fragment shader in the green
channel of the terrain vertex colour — beside the LOD morph offset in red, which
the mesh builder already owned. The fragment uses the interpolated value as the
base that the detail and layer roughness still blend over, so the close-range path
is unchanged.

Why this shape:

* The law needs only a vertex's height, latitude zone and normal, which the
  geometry already has. No source query, no extra texture, no CPU bake loop.
* Shared boundary vertices have the same height, latitude and normal, so they get
  **bit-identical** roughness at every LOD and across cube faces. A test asserts
  exactly that across two adjacent patches, which is the property a patch-shaped
  seam violates.
* Moisture is deliberately not an input; it drives albedo only. That is what makes
  the per-vertex evaluation free.

Verification, same pose, clock and frame count as the attribution runs: the
1,200 km patch-boundary step measured **12.3/255 before and 4.2/255 after**,
against **4.3/255** for the forced-constant A/B. The fix reproduces the
experiment's number without discarding the continuous depth and shore variation.
At 300 km the roughness view now follows the coastline instead of splitting the
frame on one patch edge; the production frame differs from the pre-fix reference
by a mean 1.54/255, concentrated at patch edges.

Recorded limits:

* The residual 4.2/255 is terrain shading on either side of a real edge, not a
  seam, but it is above the ±1/255 acceptance threshold, so TD-05 cannot be
  closed on it.
* The waterline is still a real discontinuity in the law (glossy seafloor against
  rough land). The mesh interpolates across it; the constant-roughness A/B shows
  what removing it entirely would cost.
* The ocean blocks remain unattributed at this point. `WaterMaterial` has no
  diagnostic view. *(Resolved 2026-11-01: they are refuted, not fixed — see
  "Water-side diagnostics" below.)*
* Two of the four captures in this slice died with a core dump **after** writing
  their PNG and with no panic in the log; the identical command exited 0 on a
  retry and produced the same frame to within a handful of pixels. This is the
  same intermittent shutdown failure recorded earlier in this log, not a
  regression from this change, but it stays on the list.

## 2026-11-01 — Water-side diagnostics (TD-05, ocean half)

The land fix left a hypothesis standing: that the sea carried its own rectangular
tone blocks, since a constant-roughness A/B changed nothing in deep water. That
A/B only ever measured deep water, where the cap is opaque and the seafloor is
invisible, so it could not have seen anything. The hypothesis was untested rather
than supported.

To test it the same way as the land, the terrain diagnostic namespace grew a
second family: six water views decoded by the water material (depth, patch-local
UV1, body colour, baked landscape shadow, cloud shadow, sky mirror, opacity).
Both materials share one selector and one code namespace, but each is given only
the codes its own shader decodes and production otherwise, so an unknown code
stays a visible no-data magenta. A water view keeps the caps visible — they are
the surface being measured — while the cloud deck is hidden for every mode, and
terrain modes still hide the caps. That split is why the water caps and the cloud
deck now carry different markers instead of one occluder tag.

The result is a refutation rather than a fix. At 300 km and 1,200 km every water
term is continuous, and none of them steps at a patch edge. Measured with a
column-step statistic (median absolute luminance step across each column over sea
pixels, 8-pixel boxes either side), the known patch-edge columns — 175, 340/353,
528, 694, 700, 991, 1170 px, cross-checked between `water-uv1` and the terrain
`patch-boundary` view — are indistinguishable from random column sets:

* `water-body` 1,200 km: 2.43 vs a 5–95 % null band of 0.55–2.89, p = 0.12
* `water-depth` 1,200 km: 5.14 vs 4.71–6.85, p = 0.83
* `water-depth` 300 km: 1.86 vs 1.61–3.53, p = 0.87
* `water-opacity` 1,200 km: 1.35 vs 0.44–2.04, p = 0.35
* `water-opacity` 300 km: 0.43 vs 0.56–1.67, p = 1.00
* `water-shadow` 1,200 km: 0.00 vs 0.00–0.10, p = 1.00
* production 1,200 km: 1.01 vs 0.35–1.95, p = 0.28
* production 300 km: 0.33 vs 0.16–0.63, p = 0.63
* production 1,200 km, pre-fix: 0.90 vs 0.34–2.01, p = 0.39

The sea's tone is the depth ramp over real bathymetry — production luminance
correlates with the depth view at −0.78 across 1,127 columns — plus cloud shadow.
What reads as a rectangle in the water is the shelf break, which runs north–south
here and is therefore a vertical line in a nadir frame. The earlier claim that
"the ocean blocks are not roughness, therefore they are something else" had a
false premise: the sea never carried a patch-shaped term, before or after the
land fix.

Recorded limits:

* These are presentation-term views. `bathymetry` is the terrain authority, and
  the depth ramp clamps at the 60 m visible depth, so everything deeper than 60 m
  renders identically; the views cannot say anything about the deep seafloor.
* The column-step test is a distribution comparison, not a paired measurement. It
  can say a patch edge is not special; it cannot put a per-edge number on a
  1/255 threshold. That is what the land residual still needs, and it is the next
  task.
* One of the four 300 km captures exited 134 after writing a valid PNG — the same
  intermittent shutdown fault already recorded, not a new one.

## 2026-11-01 — Measuring a seam against its own trend (TD-05 acceptance)

The last open question for the tone blocks was a number: 4.2/255 at a patch edge
against a ±1/255 acceptance. That comparison was not sound. The 4.2 came from two
boxes either side of a boundary, and two boxes can legitimately differ — a real
30 % slope over 8 px is already 2.4/255, so the measurement could not distinguish
a seam from terrain.

`scripts/measure_patch_seam.py` measures a **discontinuity** instead. The seam
mask comes from the `patch-boundary` diagnostic, the normal from the gradient of
that mask, and each side is sampled at 3–8 px and fitted with a quadratic, whose
value at the seam is compared with the other side. A continuous field with any
slope yields zero; a block yields its step. The control runs the same estimator
40 px along the seam, inside the neighbouring patch, so "surrounding trend" is
measured rather than assumed. Surface class comes from the `land-sea` frame, which
is what stops bright shallow water from passing as land.

Results, all with the same settings (quadratic fit, offsets 6–14 px scaled to the
seam band, control 40 px):

* The seam that motivated the task — the region used for the original box
  measurement at 1,200 km — measures **+4.14/255 before and −0.04/255 after** on
  18 paired samples, and the paired crop shows the pale wedge disappear.
* Frame-wide land seams: 300 km **+0.84 → +0.25/255** (162/161 samples); 1,200 km
  **+0.37 → +0.47/255** (118/119 samples). Both post-fix medians are inside
  ±1/255.
* The same estimator applied to the diagnostic views localises what is left:
  `albedo` −0.30, `world-normals` −0.00, `height` −0.49, `detail-weight` +0.00,
  `layer-blend` 0.00, `morph-offset` +0.10 — while `roughness` measured +25.63
  before the fix. Every per-patch input is continuous at seams, so the residual
  seam jump is the terrain either side of the boundary, which is what the control
  measures too.

Recorded limits:

* The estimator needs a sample whose whole span, and whose control 40 px along the
  seam, stay in one surface class and out of the cloud mask. That leaves about 160
  samples at 300 km and 120 at 1,200 km, and it cannot judge a land seam at 300 km
  in a frame where the deck covers most of the ground.
* The per-sample tail is not meaningful: a seam sitting on a ridge crest defeats
  any extrapolation. The median and its control are the reported quantities, so
  the acceptance is established on the median plus the one seam that was visibly
  wrong — not as a per-pixel worst case.
* At 1,200 km the frame-wide median is unchanged between builds, because only a
  minority of neighbouring patch pairs differed in roughness there. The targeted
  region is the load-bearing evidence at that pose; saying otherwise would be
  cherry-picking the frame-wide number in the other direction.

## 2026-11-01 — Regional imagery cross-fade (TD-04)

TD-03 matched the regional mosaic's *mean* tone to the Blue Marble overview, which
removed the pale rectangle but not the line around it. The remaining edge was
still a patch boundary, because `imagery_weight` was a per-patch scalar: a patch
either bound the Sentinel tile or did not.

Looking at the 40 km `albedo` frame at `-8.0,139.9,40000` showed a second cause
that no shader-only fix would have addressed. The region's east bound is at
longitude 140.0, which projects to x = 400. The imagery edge in that frame was at
x = 459 — 59 px *inside* the bound — because the patch that straddles the bound
has its centre outside the region, and `resolve` tested centre containment. So
the patch bound no tile at all, and the next patch edge to the east started the
imagery. There was nothing to feather at the region bound because nothing changed
there.

The fix therefore has two parts:

1. **Resolution by overlap.** `EarthImageryPackage::region_for_patch` tests the
   patch's geographic footprint (its four corner directions, with the circular
   longitude interval taken as the complement of the largest gap, and a pole flag)
   against the region bounds. `resolve` and `linear_color_gain` both use it, so a
   straddling patch binds the tile and gets the region's measured gain. Without
   the gain change the straddling half of a tile would have been left uncorrected,
   which would have put a bright rim exactly where the fade runs.
2. **A per-fragment band.** The material carries the region footprint and the band
   width; the fragment converts its body-fixed position to latitude/longitude with
   the terrain lat/lon authority's mapping, takes the signed depth inside the box
   and multiplies the tile weight by `smoothstep(0, feather_deg, depth)`. The
   branch is on uniform per-patch values, so only patches that bind a regional
   tile pay for it.

The band is declared in the manifest (`feather_deg: 0.15`) rather than in code,
because it is a property of the two products: the runtime overview is 8192 px
across 360°, i.e. 0.0439° per texel, and 0.15° is 3.4 of the overview's own
texels — about 16.6 km here. The manifest validator rejects a band that is not
positive or that exceeds half the region's narrowest extent, and a test pins the
checked-in value to the overview's runtime resolution.

Measurement needed a harness of its own, because a region bound is not a mesh
seam. `measure_patch_seam.py` answers "is this seam a discontinuity": for the
imagery edge that is the wrong question twice over — before the fix the edge was
not at the bound, and after the fix it is deliberately a soft ramp hundreds of
pixels wide. So `scripts/imagery_region_mask.py` projects the region's bounds into
a capture frame with the capture camera's own conventions, and
`scripts/measure_tone_edge.py` reports the frame's first-difference profile
against the bound and against the frame's own texture level. The same tool with
`--against` reports how far from the bound two builds actually differ, which is
how the band's extent is checked rather than assumed.

At the 40 km pose, `albedo` view (deck hidden by design, so the albedo mix is
alone):

| frame | worst column within ±40 px of the bound (p90) | worst column anywhere (p90) |
| --- | ---: | ---: |
| before | 4.23/255 at x = 380 (a patch edge) | 16.14/255 at x = 459 (a patch edge) |
| after | 0.72/255 at x = 424 | 0.93/255 — the frame's texture level |

and before-minus-after is above 0.1/255 only over x = 381..737, i.e. **16.4 km**,
against the declared 16.6 km band. Nothing outside the band changes.

Recorded limits:

* At 400 km no regional tile is bound at all: the manifest publishes levels 8–12
  and the visible cover there is coarser. The 400 km `albedo` frame is unchanged
  within 1/255, so TD-04 has no 400 km case; the inset lives below roughly 100 km.
  At 80 km there is neither an inset rectangle nor a LOD-boundary rectangle.
* The band is declared in degrees, so its width in metres grows toward the poles.
  That is right for a geographic box and wrong for nothing here, but a second
  region at a high latitude would need its band chosen with that in mind.
* No frame-time A/B was run. The added work is confined to patches that bind a
  regional tile and the available frame-time metrics cannot resolve it, so the
  "< 1 %" in the task's acceptance is not claimed.
* The production frame at 40 km is dominated by the cloud deck seen from above,
  with vertical streak artifacts that are a separate (cloud) defect. The imagery
  edge is therefore measured on the `albedo` view, which is the mode that exists
  to remove exactly that kind of obscuration.

## 2026-11-01 — Why land reads dark from orbit (AT-03 attribution)

The complaint was that land is nearly black at 400 km while the cloud deck is
white. The honest first step is to separate the two ends of the same surface: the
albedo the material uses at altitude, and the albedo it uses near the ground.

At `-4.0,138.0` (inland Borneo, no regional imagery, so the overview is the only
albedo source at both altitudes):

| view | 400 km | 3 km |
| --- | ---: | ---: |
| `albedo` linear luminance p50 | **0.0169** | **0.0169** |
| production linear luminance p50 | 0.0333 | 0.0742 |

The albedo view is *identical* to four decimals, so mip filtering, the atmosphere
and the lit path preserve the surface tone with altitude. Baked occlusion on that
ground is neutral — self-shadow and sky visibility are exactly 1.000 at p5, p50
and p95 — so no shadow term is subtracting light. The production-to-albedo ratio
at altitude (1.77 at the 300 km Papua pose, 2.07 at the 400 km Borneo pose) is
the lighting factor, and it is consistent between those two poses.

So land is dark from orbit because the measured overview over tropical forest is
dark: 0.017 linear is about 4 % reflectance. A white deck at ~0.9 is genuinely
around 50× brighter per unit area. Nothing is underexposed, and no altitude term
is wrong.

What is visible is the *other* end. On descent the same ground becomes 2.2×
brighter in production (0.0333 → 0.0742) while its albedo does not change, so the
extra brightness comes from material inputs that only exist near the ground. The
`inputs` view at 3 km reports `(255,255,171)`, i.e. LOCAL_SURFACES + LAYER_WEIGHTS
+ OCCLUSION and no imagery, and `layer-blend` reports the layered path active;
both fade out with view distance. Their albedo is therefore about 2–2.5× the
overview's on the same ground.

That is a tone-calibration question of exactly the kind TD-03 solved for the
regional mosaic, and it is scoped as TM-04 rather than bolted onto this task. The
reference for the calibration is already measured: the Sentinel mosaic after its
gain has a linear mean of 0.019, which agrees with the overview's 0.017.

Recorded limits:

* The near/far brightness ratio compares two poses that differ by about 4° of
  latitude and by the air column above the camera, so it is quoted as a range
  (2–2.5×) rather than a calibrated constant. A clean calibration needs the same
  pose at several altitudes with the Sun and sky held fixed, which TM-04 must do.
* "Identical albedo" is a statement about the rendered overview, not about the
  surface's true reflectance; the overview is a composited product and its
  absolute calibration is not what this measurement establishes.
* The 3 km frame is a 4 km-wide patch inside a single overview texel, which is why
  the two albedo values agree so exactly. A wider near-ground frame would average
  across texels and could not have been compared this way.

## 2026-11-03 — The near ground was calibrated to a look, not to the measurement (TM-04)

AT-03 left one number behind: at `-4.0,138.0` the production frame was 2.2×
brighter at 3 km than at 400 km while the `albedo` view was identical at both.
The first question was whether that was lighting or albedo, and the `albedo`
view answers it directly — the base albedo does not change with altitude, so the
extra brightness enters between the base and the pixel.

The shader composes the local map like this:

```wgsl
let detail_albedo = mix(base_albedo, local_albedo.rgb, 0.4 * detail_weight);
```

`local_albedo` is the source-derived per-patch map from `surface_appearance`
(forest `[0.025, 0.10, 0.015]`, blended toward wet soil inside the Papua profile).
Its luminance is about 0.078 linear; the measured overview over that ground is
0.0169. At the 3 km pose the detail weight is 1, so the mix evaluates to
`0.6 + 0.4 × 4.6 ≈ 2.4` times the base. That is the step, and it is not a
lighting bug, an exposure bug, or a mip bug.

**The control.** To make that attribution a measurement rather than an
arithmetic guess, one build replaced the local albedo term with the base:

```wgsl
let detail_albedo = mix(base_albedo, base_albedo, 0.4 * detail_weight); // control
```

Normals, roughness, the layered material and every variation term still ran. The
3 km frame's per-pixel `production / albedo` over the same forest tone fell from
**4.36 to 1.98** — the orbital level (2.03 at 40 km) — and nothing else moved.
Solving `4.36 / 1.98 = (1 − w) + w·g` with `w = 0.4` gives `g = 4.01`: the local
map carried four times the overview's luminance on that ground. The layered
material was already a unit-luminance tint and contributed nothing, which the
control confirms by leaving it in.

**The fix.** The local map is now a tint anchored to the base:

```wgsl
let base_luma = max(dot(base_albedo, LUMA_REC709), 0.0001);
let local_mean = textureSampleLevel(terrain_local_albedo, sampler, in.uv_b, LOCAL_ALBEDO_MEAN_LOD);
let local_mean_luma = max(dot(local_mean.rgb, LUMA_REC709), 0.0001);
let local_tinted = min(local_albedo.rgb * (base_luma / local_mean_luma), vec3<f32>(1.0));
let detail_albedo = mix(base_albedo, local_tinted, 0.4 * detail_weight);
```

The reference is the map's own mean, sampled from its coarsest mip (the map is
128 texels square, so level 7 is a single texel), not the fragment's own texel.
That distinction matters: scaling by the fragment's own luminance would flatten
every material to the base's brightness, while scaling by the patch mean leaves
rock brighter than forest and still puts the *mean* on the overview. A texture
with a shorter mip chain clamps to its smallest level, which degrades to the
per-fragment match rather than to an artefact.

The result at the same pose, same clock, same 200 frames:

| statistic | before | after |
| --- | ---: | ---: |
| 3 km production / albedo, same forest tone | 4.36 | **2.01** |
| 40 km production / albedo, same forest tone | 2.03 | 2.03 |
| 3 km land production, linear p50 | 0.0738 | 0.0340 |
| centred crop, 3 km vs 40 km | +128 % | **+2 %** |
| 300 / 400 km frames | 1.84 / 1.85 | measured identically (detail weight is zero there) |

The 40, 300 and 400 km frames measure identically before and after because the
detail weight is zero there; the fix can only act where a local map is
visible. The remaining 2.01 versus 1.84–1.85 at 300–400 km is the longer air
column above the camera, which is the atmosphere's job (AT-02).

**What this gives up, stated plainly.** The local map's absolute reflectance is
no longer used. A material that is physically brighter than the ground around it
now appears at the base's brightness, tinted by its own chromaticity, with the
patch mean — not the fragment — carrying the scale. That is deliberate: the
overview is the measured authority for this surface, and the local map is a
refinement of *its* structure, not a second, independently calibrated estimate of
absolute reflectance. The alternative — per-biome gains against the overview —
would need a global classification of the overview and would still leave a step
wherever the classification is wrong.

Recorded limits and follow-ups:

* One extra texture fetch at an explicit mip level per fragment, only for patches
  that bind a local map. No frame-time A/B was run, so no cost percentage is
  claimed.
* Clamping at 1 is the only departure from exact mean matching and only trips for
  saturated tints over a bright base.
* The near-ground frame still varies ~4 % between runs at frame 200, because the
  closest patch may or may not have finished its material upload by then. The
  before/after difference (a factor of 2.2) is far outside that, but the docs
  quote the run and the statistic rather than a single pixel.
* With the tone matched, the near ground is visibly darker than it used to be —
  it now looks like the orbital view. That is the intended result, and it makes
  the tropical-forest albedo of the overview the thing to question next if the
  surface reads too dark, not the material.

## 2026-11-05 — Site grading at every launch site (TD-02) exposed the erosion coupling

The capture path now pins the simulation start instant
(`COSMIC_SYSTEMS_CAPTURE_UTC`), because the shared default (`2000-01-01
03:00 UTC`) is local noon only at Papua. Every predefined site was captured at
the same solar hour angle with the default chase camera and 240 frames.

| site | composed ground − local sea datum | outcome |
| --- | ---: | --- |
| Baikonur | +98.60 m | pad flush |
| Vandenberg | +185.42 m | pad flush |
| Mahia | +7.30 m | pad flush |
| Kourou | +4.46 m | pad flush |
| Papua | +6.16 m | pad flush (existing frame) |
| KSC LC-39A | −0.14 m | **tower in water** |
| Starbase | −0.78 m | **tower in water** |
| Wallops | −13.61 m | **tower in open water** |
| KSC legacy reference | −0.94 m | used by the physics baselines |

Two causes, both measured by sampling the composition directly (the probe loads
each site's covering payload tile first, so it sees what the launch setup sees):

1. **The erosion field decouples the pad from its payload.** The resident
   payload sample `D` and the composed `height_m` `H` disagree:
   Vandenberg `68.10 → 149.22` (**+81.12 m**), Mahia `58.94 → 28.65`
   (−30.29 m), Wallops `−24.43 → −51.31` (−26.88 m), KSC LC-39A
   `−15.96 → −29.87` (−13.90 m), Baikonur `80.03 → 65.09` (−14.94 m),
   Kourou `−38.21 → −29.80` (+8.41 m), Papua `71.98 → 77.59` (+5.61 m),
   Starbase `−22.49 → −23.37` (−0.88 m). The deltas track the difference
   between the regional NASADEM and the coarse global fallback, which is the
   tell: the erosion tile is baked while only the site's finest payload tile is
   resident, so most of the tile is built from the global base and the erosion
   transport then smooths the small payload island into it.
2. **The erosion cache is not a cache of the same field.** Two identical runs
   with `COSMIC_SYSTEMS_EROSION_CACHE=off` agree to render jitter (mean
   0.82/255, 0.01 % above 8/255). A warm and a cold cache differ by 90 % of
   pixels; Baikonur changes from green steppe to brown desert and Starbase loses
   its surrounding land entirely. The cold frames are also incomplete at frame
   240, because a from-scratch erosion bake is slower than streaming.

Starbase is additionally marginal on its own data: the package probe is
`−22.485 m` ellipsoidal against the runtime EGM2008 sea surface of `−22.59 m`,
so only **0.1 m** separates the pad from the sea before erosion. The "site probe
is approximately 3 m above the EGM2008 sea surface" line in
`docs/datasets/earth_regional_starbase_v1.md` does not match those numbers; the
0.5° geoid grid gives the smaller clearance. That is recorded rather than
silently adjusted.

Consequences for the roadmap:

* **TD-02 is BLOCKED** on TD-10 and TD-11: the capture set is complete, but
  three of the pads are not flush and the terrain under them is wrong.
* **TD-10** owns the erosion/payload coupling and the cache dependence.
* **TD-11** owns the datum floor. The committed ascent, gravity-turn and LEO
  baselines launch from `ksc-default-geodetic`, which the same probe puts 0.94 m
  under the sea surface, so flooring that pad would have moved them. The ascent
  fixture now pins its launch altitude at the exact pre-fix value (`c886205`),
  and all three baselines still pass bit-identically; the terrain fix can
  proceed without touching a recorded trajectory.

This is the same class of seam as TD-06 and the reason it is written down rather
than patched: a baseline shift nobody has agreed to is not a fix.
