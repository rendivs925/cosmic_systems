# Terrain diagnostic render modes

Developer-only false-colour views of the terrain material, one per input, so a
visible defect can be attributed to a term instead of guessed at.

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

* **Only streamed terrain patches are diagnosed.** The far-field planet proxy,
  the ocean caps, the cloud deck, vegetation, rock shells and rivers keep their
  own materials. If a diagnostic capture shows a surface in production colours,
  that surface is not streamed terrain — which is itself the answer when a defect
  only appears at altitude.
* **Water caps and the cloud deck are hidden** while a mode is active: a cap
  covers exactly the submerged terrain `land-sea` classifies, and a deck covers
  continents at altitude. Both return to `Inherited` on the way back to
  production.
* **There is no terrain-source identity mode.** The fragment shader has no
  access to the terrain authority, and a mode that guessed the contributing
  source would be a confident picture of something nobody measured. `inputs`
  reports what the material actually binds, which is the part a shader can know.
* **There is no "shadow visibility" mode beyond `occlusion`.** The directional
  shadow map is a render-graph product, not a material input.
* **Diagnostics are not a performance harness.** Switching is one `u32` per
  material; capture runs remain pinned to 60 Hz by the offscreen runner.

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
