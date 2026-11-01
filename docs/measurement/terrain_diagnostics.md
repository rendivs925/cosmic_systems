# Terrain and water diagnostic render modes

Developer-only false-colour views of the terrain and water materials, one per
input, so a visible defect can be attributed to a term instead of guessed at.

## Why they exist

The 1,200 km and 300 km frames show rectangular tone blocks that no single frame
could explain: they survive a seven-hour Sun change, they are unchanged between a
90-frame and a 400-frame capture, and they appear at two altitudes. Those
differentials narrowed the defect to "albedo or geometry" and then stopped.
Narrowing further needed a view of *each input on its own*.

These modes are that view. They answer:

* is the geographic albedo correct, and does the step already exist in it?
* are the UV coordinates continuous?
* are the normals correct and continuous across cube faces?
* is the rendered surface the authoritative height field?
* does the land/sea classification line up with the datum?
* which surface class does each patch carry, and where is there none?
* which LOD is each patch drawn at, and do neighbours disagree?
* where do patch boundaries fall, and which patch owns a pixel?
* which per-patch material inputs exist at all?

## Usage

```bash
COSMIC_SYSTEMS_DIAGNOSTIC_MODE=<mode> \
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=/tmp/mode.png \
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES=200 \
COSMIC_SYSTEMS_CAPTURE_POSE="-8.0,139.5,300000" \
COSMIC_SYSTEMS_CAPTURE_TIME=42 \
  ./target/debug/cosmic_systems rocket --site papua
```

`<mode>` is a name or a numeric code. Names are case-insensitive and `_` is
accepted for `-`. An unknown value renders production and logs a warning naming
every accepted value; it never fails the run.

In a debug build, `Ctrl+U` steps through the modes and `Ctrl+I` returns to
production rendering. The release binary registers no diagnostic key binding at
all. The environment variable is the authoritative selector because the offscreen
capture path has no window, no focus, and no HUD.

## Modes

| code | name | what it shows |
| ---: | --- | --- |
| 0 | `normal` | production rendering; the only mode that ships |
| 1 | `albedo` | geographic base colour: the global overview mixed with the regional imagery tile by `imagery_weight` |
| 2 | `world-normals` | world-space geometric normal as `n * 0.5 + 0.5` |
| 3 | `uv0` | geographic equirectangular UV0 with a 16-division grid |
| 4 | `uv1` | patch-local UV1 with an 8-division grid |
| 5 | `height` | height above the sea-level datum on a documented ramp |
| 6 | `land-sea` | land/sea classification, with the datum band drawn white |
| 7 | `surface-class` | dominant ground layer from the patch weight map; magenta where none exists |
| 8 | `lod` | patch LOD level as a hue ramp, seams darkened |
| 9 | `patch-boundary` | patch seams over mid grey |
| 10 | `patch-identity` | a stable colour per patch (FNV-1a of face, level, tile x/y) |
| 11 | `inputs` | which per-patch material inputs exist (additive colour) |
| 12 | `occlusion` | baked landscape occlusion: red is self-shadow, green is sky visibility |
| 13 | `roughness` | final perceptual roughness as grey |
| 14 | `metallic` | the patch material's metallic value as grey |
| 15 | `layer-blend` | whether the layered ground material is active; magenta is the single-layer fallback |
| 16 | `morph-offset` | per-vertex LOD morph displacement in metres |
| 17 | `detail-weight` | source-derived local detail weight after the view-distance fade |
| 18 | `water-depth` | sea-cap normalized depth, the input of the colour and opacity ramps |
| 19 | `water-uv1` | the water pass's own patch-local UV1 with a grid |
| 20 | `water-body` | the shallow/deep ramp, crest scatter and foam before lighting |
| 21 | `water-shadow` | baked landscape shadow (red), cloud shadow (green), product (blue) |
| 22 | `water-reflection` | the sky/deck mirror contribution added over the lit sea |
| 23 | `water-opacity` | final alpha, i.e. how much seafloor shows through |

Modes 1–17 are decoded by the terrain material, 18–23 by the water material. Both
families share one selector and one code namespace, but each material is given
only the codes its own shader decodes and production otherwise, so an unknown
code stays a visible no-data magenta instead of a silent fallback.

Code order is a contract: codes are never reordered, only appended. A test pins
each code to its shader constant.

## Reading the values

The shader writes **linear** values. While a mode is active, the camera's
tonemapping and bloom are bypassed and the previous values are restored on
return to production rendering. The PNG therefore holds the linear value encoded
as sRGB, so:

```python
linear = ((byte / 255 + 0.055) / 1.055) ** 2.4   # for byte/255 > 0.04045
linear = (byte / 255) / 12.92                     # otherwise
```

Documented ranges:

| mode | encoding |
| --- | --- |
| `height` | `t = 0.5 + 0.5 * sign(h) * sqrt(min(abs(h), 2000) / 2000)`, `h` in metres above the datum, then clamped to `[0, 1]`; every 100 m contour darkened to 0.75 |
| `land-sea` | water `(0.05, 0.20, 0.85)`, land `(0.15, 0.70, 0.20)`, datum band `abs(h) < 2 m` white |
| `morph-offset` | `t = (offset + 100) / 200` clamped, offset in metres |
| `lod` | hue = `level * 0.083` turns, brightness `0.55 + 0.45 * fract(level * 0.083)` |
| `inputs` | `+0.9` red local surfaces, `+0.9` green layer weights, `+0.9` blue imagery, `+0.35` all channels for a baked occlusion map, floor `0.06` |
| `surface-class` | grass green, soil brown, rock grey, sand yellow, snow white, gravel dark grey, mud dark brown; magenta means no data |
| `patch-boundary` | mid grey fill, white seam; the seam half-width is 0.0035 of the patch in UV1 |
| `uv0` / `uv1` | `(u, v, 0.25)` with dark grid lines; UV0 divides the sphere into 16, UV1 the patch into 8 |
| `water-depth` | `depth` as grey, with every 0.05 normalized-depth contour darkened to 0.7 (about 3 m on the 60 m visible-depth scale) |
| `water-uv1` | `(u, v, 0.25)` of the cap's patch-local UV1 with an 8-division grid, lines darkened to 0.3 |
| `water-body` | the water colour after the depth ramp, crest scatter and foam, before any lighting |
| `water-shadow` | `(terrain visibility, cloud visibility, product)`; white is fully lit |
| `water-reflection` | luminance of the mirror contribution, clamped to `[0, 1]` |
| `water-opacity` | final alpha as grey, every 0.05 contour darkened to 0.7 |

The height datum is the terrain authority's own sea level at the **patch
centre** plus the renderer's water offset, i.e. the same value the ocean cap is
built from. It is one number per patch, so a patch spanning a large geoid
gradient is approximate by up to that gradient. At LOD 12 (a few tens of metres)
the error is far below a pixel; at LOD 0 it is not, and the mode is not meant for
that scale. Height itself is measured as `length(fragment - planet_centre)`, an
`f32` at ~6.37e6 m, so its resolution is about 0.5 m.

The square-root ramp is deliberate. A linear planetary range makes a metre-scale
seam step unmeasurable: at a 5,000 m span one 8-bit sRGB step is about 5 m. The
square root gives the coastal band roughly 13/255 per 10 m while still showing
2 km relief, and everything beyond ±2,000 m clamps.

## What these modes do not cover

* **Only the material family a mode belongs to is diagnosed.** A terrain mode
  false-colours streamed terrain; a water mode false-colours the sea. The
  far-field planet proxy, vegetation, rock shells and rivers keep their own
  materials. If a capture shows a surface in production colours, that surface is
  not the one being measured — which is itself the answer when a defect only
  appears at altitude.
* **The surface that would hide the measured one is hidden.** Terrain modes hide
  the sea caps, because a cap covers exactly the submerged terrain `land-sea`
  classifies; the cloud deck is hidden for both families because it covers
  continents at altitude. The caps stay visible in a water mode — they are the
  surface being measured. Everything returns to `Inherited` on the way back to
  production.
* **A water view shows the ground in production colours.** That is deliberate:
  the coastline is the frame's most useful reference, and a false-coloured land
  would hide it.
* **There is no terrain-source identity mode.** The fragment shader has no
  access to the terrain authority, and a mode that guessed the contributing
  source would be a confident picture of something nobody measured. `inputs`
  reports what the material actually binds, which is the part a shader can know.
* **There is no "shadow visibility" mode beyond `occlusion`.** The directional
  shadow map is a render-graph product, not a material input.
* **Diagnostics are not a performance harness.** Switching is one `u32` per
  material; capture runs remain pinned to 60 Hz by the offscreen runner.

## What the modes have found

The first complete matrix at the 1,200 km pose (`-8.0,139.5,1200000`) separates
the rectangular tone blocks into two different defects, which is what the modes
were built for:

| view | result | reading |
| --- | --- | --- |
| `world-normals` | continuous gradient | normals are not the cause |
| `uv0`, `uv1` | continuous ramps, no gap or overlap | mapping is not the cause |
| `occlusion` | uniform (1,1) | no baked occlusion at this altitude |
| `inputs` | occlusion bit only | no imagery, layers or local surfaces to differ |
| `lod`, `patch-boundary` | levels 4–6, seams coincide | patch layout is correct |
| `roughness` | patch-shaped blocks, 0.25 seafloor vs 0.85 land | **per-patch constant roughness** |
| `metallic` | black | metallic is zero, so only roughness varies |
| `height`, `land-sea` | a pond field within ±2 m of the datum | genuine near-sea-level lowland, not a renderer defect |

The roughness blocks are the land component of the defect. A controlled A/B —
`patch_material` rebuilt with a constant `(0.85, 0.0)` — reduces a measured patch
boundary step from 12.2/255 to 4.1/255 at the same pose, clock and frame count.
The height/land-sea pond field was checked the other way: the offline
`terrain_authority_transect` tool reproduces the in-engine authority values
exactly, and every mesh vertex matches the source to within the 5 m skirt, so
that pattern is data, not rendering. Disabling the CDLOD morph leaves it
unchanged, which rules the morph out as well.

The ocean half of the defect is **not** roughness: the same A/B leaves deep-water
variation unchanged. The sea is drawn by `WaterMaterial`, which these modes do not
instrument; the water modes below were added for it, and they close that question
by refuting it rather than fixing it.

### The roughness fix, seen through the same view

Roughness is no longer a per-patch scalar. It is evaluated **per vertex** from the
same appearance law (`surface_roughness`, which `surface_appearance` itself
calls), using only inputs a vertex already has: its height above the datum, its
latitude zone and its normal. Shared boundary vertices therefore receive identical
values at every LOD and across cube faces, and the fragment shader interpolates
the field. Moisture is not an input because it drives albedo only, so no source
query is needed per vertex.

The before/after is visible in the mode itself: `diag-roughness-300km.jpg` splits
the frame on a single patch edge, while `diag-roughness-300km-continuous.jpg`
follows the coastline. At 1,200 km the same production frame measures a patch
step of **12.3/255 before and 4.2/255 after**, against 4.3/255 for the
forced-constant A/B — the fix reproduces the experiment's result without giving
up the continuous depth and shore variation. The residual is ordinary terrain
shading on either side of a real terrain edge, not a seam.

The waterline stays a real discontinuity in the law (glossy seafloor against rough
land); the mesh interpolates across it instead of stepping at a patch boundary,
and the constant-roughness A/B shows the visible cost of removing it entirely.

### The sea has no patch-boundary signal

The land fix left one open question: the sea showed its own tone structure, and
nothing so far had looked at the water material. The water modes (18–23) answer
it directly. At both poses every term that can shade the sea — depth, body
colour, opacity, baked landscape shadow, cloud shadow, sky mirror — is
continuous, and none of them steps at a patch edge:

| view | 1,200 km | 300 km |
| --- | --- | --- |
| `water-depth` | contours follow the shelf break, no straight edge | same |
| `water-body` | organic shelf/abyss ramp, no straight edge | same |
| `water-opacity` | smooth ramp, contours follow depth | same |
| `water-shadow` | exactly 1.0 across the open sea; only cloud shadows darken it | n/a |
| `water-reflection` | near zero at nadir; the visible structure is cloud shadow | n/a |

The numbers come from a column-step statistic: for every image column, the
median absolute luminance step across it over sea pixels, using 8-pixel boxes on
either side. The patch-edge columns are known independently — `water-uv1` and the
terrain `patch-boundary` view agree on them (175, 340/353, 528, 694, 700, 991,
1170 px). Comparing the mean step at those columns against 20,000 random draws of
the same number of columns from the same frame:

| frame | patch-edge mean step | random 5–95 % | p |
| --- | ---: | ---: | ---: |
| `water-body` 1,200 km | 2.43 | 0.55 – 2.89 | 0.12 |
| `water-depth` 1,200 km | 5.14 | 4.71 – 6.85 | 0.83 |
| `water-depth` 300 km | 1.86 | 1.61 – 3.53 | 0.87 |
| `water-opacity` 1,200 km | 1.35 | 0.44 – 2.04 | 0.35 |
| `water-opacity` 300 km | 0.43 | 0.56 – 1.67 | 1.00 |
| `water-shadow` 1,200 km | 0.00 | 0.00 – 0.10 | 1.00 |
| production 1,200 km | 1.01 | 0.35 – 1.95 | 0.28 |
| production 300 km | 0.33 | 0.16 – 0.63 | 0.63 |
| production 1,200 km, **pre-fix** | 0.90 | 0.34 – 2.01 | 0.39 |

No water term, and no production frame, distinguishes a patch edge from an
arbitrary column. The sea's tone at these poses is the depth ramp over real
bathymetry (`-0.78` correlation between production luminance and the depth view
across 1,127 columns) plus cloud shadow. The earlier inference that "the ocean
blocks are not roughness, therefore they are something else" had the wrong
premise: the sea was never carrying a patch-shaped term. What looked like
rectangles in the water are the shelf break, which is north–south at this
location and therefore vertical in a nadir frame.

## Cost

* One `u32` comparison per fragment in production rendering; the diagnostic
  branch is uniform per patch and is not taken.
* On a switch: one uniform write per terrain material. No mesh, image, material
  asset, terrain cache entry, or shadow map is recreated. A test asserts the
  material asset id survives every mode change and that only one material exists.
* On a switch: camera `Tonemapping` and `Bloom` are saved and replaced, then
  restored exactly. That is a pipeline-specialization change, so the first
  diagnostic frame after a toggle may compile shader variants.
* The material sync loop compares one `u32` per material every frame so a patch
  that streams in during a diagnostic view converges immediately instead of
  flashing production colours.

## Verification

* `terrain_uniform_layout_matches_the_shader_struct` compares every
  `#[uniform(104)]` field, in order, against the WGSL
  `TerrainSurfaceExtension` struct. A mismatched layout silently kills the
  terrain material, so this test is the one that must never be skipped.
* `diagnostic_mode_codes_match_the_shader_constants` pins each mode code and the
  documented visualization ranges.
* `diagnostic_input_flags_match_the_shader_bits` pins the input bitfield.
* `presentation::diagnostics::tests` covers parsing, defaults, uniqueness and
  cycling.
* `terrain::diagnostics::tests` covers mode propagation, in-place material
  updates, materials created mid-view, occluder visibility, and the developer
  chord.
* `docs/progress/VISUAL_EVIDENCE.md` records one capture per mode at a stated
  pose and commit.
