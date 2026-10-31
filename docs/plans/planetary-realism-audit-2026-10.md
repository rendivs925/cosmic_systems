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

### 2. Rectangular albedo patches — CONFIRMED, NOT YET FIXED

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

This is a data-fusion problem, not a shading bug. The fix has two parts and both
are needed:

1. **Ingest-time colour match.** Fit a per-channel gain/offset (or a histogram
   match in a perceptual space) between the regional mosaic and the overview
   over the region's own footprint, and store it in the imagery manifest so the
   producer, not the renderer, owns the correction.
2. **Spatially continuous feather.** Blend `imagery_weight` against a band of the
   region's angular footprint (`smoothstep` over the last tile-width inside the
   edge), evaluated from the fragment's body position, so the transition is
   independent of where patch boundaries fall. A per-patch uniform cannot do
   this, which is why the rectangles are axis-aligned.

Not done in this slice: it needs the 34 MB tile package regenerated and its
checksums re-recorded.

### 3. Launch facility reads as a detached slab — CONFIRMED, NOT YET FIXED

The apron is a flat `Cuboid::new(64.0, 0.5, 64.0)` whose top is 0.05 m above the
sampled terrain height at the anchor, oriented to the terrain normal at the
anchor (`sync_launch_pad_presentation`). The terrain *sample* is smooth, but the
terrain *mesh* near the pad has ~38 m vertex spacing at the LODs the chase camera
uses, so over the 64 m footprint the rendered ground is two or three triangles
whose plane does not match the tangent at the centre. The slab ends up buried on
the uphill edge and floating on the downhill edge, with a visible shadow gap —
exactly what the ground capture shows. The water tanks, blockhouse and
floodlights inherit the mismatch because they are parented to the same plane.

Also confirmed: `PadFlatZone` suppresses only the *procedural detail* layer
(`ProceduralDetailSource::detail_m`), never the measured base, so it cannot make
the footprint level even in principle. It is a detail-fade, not a grading.

Chosen fix (deferred to its own slice because it needs new geometry): build the
foundation as a **cut-and-fill platform** — inner deck flat at the pad elevation,
outer ring sampled from the authoritative `TerrainSource` at each rim vertex and
joined by a graded slope. Presentation-only, so the physics baseline does not
move, and `TerrainSource` stays the single authority for the sampled rim.

### 4. Near-site terrain is featureless — CONFIRMED (config), NOT YET FIXED

Two independent causes:

* `EARTH_PAD_FLAT_RADIUS_M = 750` and `EARTH_PAD_BLEND_RADIUS_M = 3000` suppress
  the ±24 m / ~250 m-wavelength detail field over a 3 km disc around every
  predefined site. The chase camera stands inside that disc, which is why the
  ground reads as a smooth green wash. The anchor sample is inside the flat
  radius either way, so shrinking the radii does **not** move the launch
  elevation or any physics baseline.
* The detail layer is faded out entirely at LOD ≤ 11
  (`DetailLodFade::new(11, 14)`), and the terrain shader adds only *albedo*
  patchiness below that. Geometry detail therefore does not exist at distance,
  which is the "distant terrain appears flat" observation.

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

## What this slice changed

* `water_mesh_for_patch`: ocean cap stays on the datum; lakes keep their local
  surface.
* New regression test for a sloped shore.
* This document.

## Remaining work, in dependency order

1. Imagery tile colour match + spatial feather (defect 2). Needs the tile
   package regenerated; highest visual payoff at 400–1200 km.
2. Launch-site cut-and-fill platform and facility grounding (defect 3).
3. Shrink the pad detail-fade radii and add a geometry detail band at distance
   (defect 4).
4. Diagnostic modes, then use them on the high-altitude darkness (defect 6).
5. Cloud shape/type diversity and edge structure (defect 5).
6. Vegetation audit and near-field canopy work (defect 8).
7. Performance measurement pass, which the brief requires and which has not been
   run against this baseline.
