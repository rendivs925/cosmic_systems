# Visual evidence registry

Every frame here was produced by the deterministic capture path
(`COSMIC_SYSTEMS_CAPTURE_POSE` + `COSMIC_SYSTEMS_CAPTURE_TIME`, and after
`b7dbb2d` the offscreen writer `COSMIC_SYSTEMS_OFFSCREEN_CAPTURE`). Each row
carries the code commit it was taken at. **A frame without a commit is not
evidence.**

Reproduce any row with:

```bash
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=/tmp/frame.png \
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES=<frames> \
COSMIC_SYSTEMS_CAPTURE_POSE="<lat>,<lon>,<alt_m>" \
COSMIC_SYSTEMS_CAPTURE_TIME=<seconds> \
[COSMIC_SYSTEMS_DIAGNOSTIC_MODE=<mode>] \
  ./target/debug/cosmic_systems rocket --site papua
```

The offscreen path does not draw the HUD, so HUD text is not part of any
measurement. Diagnostic rows add `COSMIC_SYSTEMS_DIAGNOSTIC_MODE`; the mode's
values and its linear-to-byte encoding are in
`docs/measurement/terrain_diagnostics.md`.

## Required scene matrix

| # | scene | state | frame | note |
| --- | --- | --- | --- | --- |
| 1 | prelaunch | PARTIAL | `pad-110m-footprint` | pad and tower, no HUD, chase angle only |
| 2 | launch site | CURRENT | `pad-110m-footprint` | shows the pad flush after grading |
| 3 | terrain close-up | CURRENT | `terrain-600m` | 600 m nadir; no geometric relief visible |
| 4 | coastline | MISSING | `diag-land-sea-1200km` | production coastline still has no usable frame; the diagnostic land/sea view now shows where the classification line falls |
| 5 | clouds | CURRENT | `1200km-90f`, `production-1200km` | deck, edges and shadows visible |
| 6 | atmosphere | MISSING | — | no altitude sequence or limb capture yet |
| 7 | vegetation | PARTIAL | `terrain-600m` | trees read as isolated specks only |
| 8 | rocket ascent | MISSING | — | no offscreen path for firing the vehicle yet |
| 9 | high altitude | CURRENT | `300km-papua`, `production-1200km` | 300 km and 1,200 km nadir |
| 10 | orbit | MISSING | — | 1,200 km is the highest captured |

Five of ten required scenes have no usable frame. That is the point of the
matrix: it is the most honest measure of how much of the visual programme is
actually evidenced. The diagnostic views added below do not fill these scenes —
they are false colours, not production frames — but they do make two of the
missing ones explainable.

## Registry

| file | commit | pose | time | frames | sha256 (first 16) | demonstrates | pass criterion |
| --- | --- | --- | --- | ---: | --- | --- | --- |
| `2026-10-31-1200km-90f.jpg` | `aca26dd` | `-8.0,139.5,1200000` | 42 s | 90 | `43d2ce8908721545` | 1,200 km nadir: deck, albedo blocks, ocean | blocks present (failing) |
| `2026-10-31-1200km-400f.jpg` | `aca26dd` | same | 42 s | 400 | `0ea744150cb6d206` | streaming settled: blocks unchanged | identical block pattern to 90f |
| `2026-10-31-1200km-sun-plus7h.jpg` | `aca26dd` | same | 25,242 s | 90 | `9c98092f2c182e6b` | seven hours later: glint moves, blocks do not | block pattern identical to 90f |
| `2026-10-31-1200km-lon120.jpg` | `aca26dd` | `-8.0,120.0,1200000` | 42 s | 200 | `025cb0039782b1ed` | different geography, same artifact class | blocks present (failing) |
| `2026-10-31-300km-papua.jpg` | `aca26dd` | `-8.0,139.5,300000` | 42 s | 200 | `aa4ad2e504204d96` | 300 km: blocks at a second scale, land dark | blocks present (failing) |
| `2026-10-31-pad-250m-footprint.jpg` | `b7dbb2d` | default chase | 0 s | 240 | `3db6d6622b477d23` | 250 m graded footprint: pale disc around pad | disc visible (failing) |
| `2026-10-31-pad-110m-footprint.jpg` | `d89e7d6` | default chase | 0 s | 240 | `1fa53406f293f25c` | 110 m footprint: pad flush, tone continuous | no disc, no floating edge (passing) |
| `2026-11-01-terrain-600m.jpg` | `d89e7d6` | `-8.0,139.5,600` | 42 s | 220 | `3c7b470c5d3bceec` | coastal plain, water bodies, tree specks | relief and canopy visible (failing) |
| `2026-11-01-coast-40km-attempt.jpg` | `d89e7d6` | `-8.3,138.6,40000` | 42 s | 220 | `76a9513193f5ade4` | unsuccessful coastline attempt: featureless field | coastline visible (failing, unusable) |

### Diagnostic render modes (`P0-03`, commit `f3b1af6`)

All rows are 1,280×720 offscreen captures with `COSMIC_SYSTEMS_DIAGNOSTIC_MODE`
set, the same pose and clock as the frame beside them, and camera tonemapping
and bloom bypassed. Read them as linear values: `linear = srgb_to_linear(byte /
255)`. Values are documented in `docs/measurement/terrain_diagnostics.md`.

Two rows are marked **modified build**. They are controlled A/B measurements of
one term, not shipping behaviour, and are reproduced by changing exactly one
place and re-capturing the same pose, clock and frame count:

- `ab-constant-roughness`: `patch_material(surface.roughness, surface.metallic)`
  replaced with `patch_material(0.85, 0.0)` in `terrain/render.rs`.
- `ab-morph-off-height`: the CDLOD `morph_factor` forced to `0.0` in
  `assets/shaders/terrain_surface.wgsl`.

Neither change is committed; the frames exist so the attribution can be checked
without repeating the experiment.

| file | pose | frames | sha256 (first 16) | mode | pass criterion |
| --- | --- | ---: | --- | --- | --- |
| `2026-11-01-diag-normal-300km.jpg` | `-8.0,139.5,300000` | 200 | `f7b56bc1c5c6df39` | normal | production rendering; mean abs diff 0.92/255 against `2026-10-31-300km-papua.jpg` |
| `2026-11-01-diag-albedo-300km.jpg` | same | 200 | `ae7a63be01004fba` | albedo | continuous coastline; no patch-shaped albedo seam |
| `2026-11-01-diag-world-normals-300km.jpg` | same | 200 | `c25807da2abc57b2` | world normals | smooth gradient: R 0.591–0.651 linear, no discontinuity |
| `2026-11-01-diag-uv0-300km.jpg` | same | 200 | `536fd4e7d3cd0ab5` | uv0 | single continuous ramps field; grid unbroken |
| `2026-11-01-diag-uv1-300km.jpg` | same | 200 | `7cdb32d525925dec` | uv1 | per-patch ramps tile with no gap or overlap |
| `2026-11-01-diag-height-300km.jpg` | same | 200 | `7f8d4ec8075ccd1f` | height | datum exactly mid-grey; contours follow the shelf |
| `2026-11-01-diag-land-sea-300km.jpg` | same | 200 | `f7e2cc25bffd3fcd` | land/sea | classification follows the albedo coastline |
| `2026-11-01-diag-surface-class-300km.jpg` | same | 200 | `3a776f7ed53a19f5` | surface class | **uniform magenta**: no classification data at this altitude, reported rather than faked |
| `2026-11-01-diag-lod-300km.jpg` | same | 200 | `a696ba21abfbac3d` | lod | two levels visible (4 and 5), seams match patch edges |
| `2026-11-01-diag-patch-boundary-300km.jpg` | same | 200 | `8f3521a2dc228c94` | patch boundary | seams lie exactly on the LOD-view patch edges |
| `2026-11-01-diag-patch-identity-300km.jpg` | same | 200 | `222481f42e4db057` | patch identity | distinct colour per patch, no duplicates in frame |
| `2026-11-01-diag-inputs-300km.jpg` | same | 200 | `e5508d8734b2208b` | inputs | **occlusion only**: no imagery, layers or local surfaces at 300 km |
| `2026-11-01-diag-occlusion-300km.jpg` | same | 200 | `c43c97cc8da51d72` | occlusion | uniform (1,1): no baked occlusion on coarse patches |
| `2026-11-01-diag-roughness-300km.jpg` | same | 200 | `8c51fed419b3e658` | roughness | **pre-fix**: the coastline is one patch edge between 0.25 and 0.85 |
| `2026-11-01-diag-metallic-300km.jpg` | same | 200 | `289340d25e43a1bc` | metallic | black by construction: terrain metallic is 0 |
| `2026-11-01-diag-layer-blend-300km.jpg` | same | 200 | `3a776f7ed53a19f5` | layer blend | magenta everywhere: single-layer fallback at this altitude |
| `2026-11-01-diag-morph-offset-300km.jpg` | same | 200 | `d357f8b4ab9af478` | morph offset | mid grey with small deviations; no patch-sized step |
| `2026-11-01-diag-detail-weight-300km.jpg` | same | 200 | `289340d25e43a1bc` | detail weight | black: source detail fades out beyond 6 km |
| `2026-11-01-production-1200km.jpg` | `-8.0,139.5,1200000` | 200 | `122b0a550d10ea19` | normal | production at the failing pose: blocks visible |
| `2026-11-01-diag-albedo-1200km.jpg` | same | 200 | `46c5a1f2c54bff79` | albedo | **no rectangular block**: the base albedo is clean |
| `2026-11-01-diag-height-1200km.jpg` | same | 200 | `e04b7da051e73cb0` | height | rectangular region at the datum, straight edges |
| `2026-11-01-diag-land-sea-1200km.jpg` | same | 200 | `ffe5ec908ef2e50b` | land/sea | same rectangle as a ±2 m datum band on the shelf |
| `2026-11-01-diag-inputs-1200km.jpg` | same | 200 | `1474fd4d8337cd5b` | inputs | occlusion only, as at 300 km |
| `2026-11-01-diag-patch-identity-1200km.jpg` | same | 200 | `62ac731951f93972` | patch identity | the datum rectangle is **not** a patch: it cuts across one |
| `2026-11-01-diag-inputs-3km.jpg` | `-8.3,138.6,3000` | 220 | `2bec60b7ac1250f3` | inputs | local surfaces + layers, no imagery (that longitude is outside the region) |
| `2026-11-01-diag-inputs-10km.jpg` | same | 220 | `cabd790e3b83c85a` | inputs | data still present at 10 km |
| `2026-11-01-diag-inputs-30km.jpg` | same | 220 | `2f100d165cc9cb39` | inputs | detail data thinning out |
| `2026-11-01-diag-inputs-70km.jpg` | same | 220 | `735441272b8376d6` | inputs | occlusion only: detail data gone by 70 km |
| `2026-11-01-diag-surface-class-3km.jpg` | same | 220 | `07682493e3972262` | surface class | grass and soil classified from the weight map |
| `2026-11-01-diag-layer-blend-3km.jpg` | same | 220 | `c3512b0bf80d1338` | layer blend | layered path active at ground level |
| `2026-11-01-diag-height-3km.jpg` | same | 220 | `0616a9c706455eab` | height | coastal plain, rivers below the datum |
| `2026-11-01-diag-world-normals-1200km.jpg` | `-8.0,139.5,1200000` | 200 | `40362a3366908b19` | world normals | continuous gradient, no face seam or step |
| `2026-11-01-diag-uv0-1200km.jpg` | same | 200 | `14b8766b4a76005b` | uv0 | continuous geographic ramp; grid unbroken |
| `2026-11-01-diag-uv1-1200km.jpg` | same | 200 | `69866bdc5e5719a0` | uv1 | per-patch ramp tiles with no gap or overlap |
| `2026-11-01-diag-lod-1200km.jpg` | same | 200 | `a604b04eacd8d9f7` | lod | levels 4–6 only; seams match the patch grid |
| `2026-11-01-diag-patch-boundary-1200km.jpg` | same | 200 | `d393e67ea93eae4a` | patch boundary | seams identical to the LOD view, no extra edges |
| `2026-11-01-diag-occlusion-1200km.jpg` | same | 200 | `85a615342641b359` | occlusion | uniform (1,1): no baked occlusion at this altitude |
| `2026-11-01-diag-roughness-1200km.jpg` | same | 200 | `ce7937ec3e61ed21` | roughness | **pre-fix**: patch-shaped blocks, 0.25 seafloor vs 0.85 land sampled once per patch |
| `2026-11-01-diag-layer-blend-1200km.jpg` | same | 200 | `4c45f62af5b2e918` | layer blend | magenta: single-layer fallback everywhere |
| `2026-11-01-diag-metallic-1200km.jpg` | same | 200 | `278eb37660dcf9a2` | metallic | black: terrain metallic is 0 |
| `2026-11-01-diag-detail-weight-1200km.jpg` | same | 200 | `73376043021193e6` | detail weight | black: source detail faded out at LOD ≤ 11 |
| `2026-11-01-diag-morph-offset-1200km.jpg` | same | 200 | `d1cd98110b251d31` | morph offset | large deltas at coastlines only; lowland near 0 |
| `2026-11-01-diag-surface-class-1200km.jpg` | same | 200 | `40c73a7072714ef9` | surface class | magenta: no classification data at this altitude |
| `2026-11-01-ab-constant-roughness-1200km.jpg` | same | 200 | `5b20c6f40af847fc` | production, **modified build** | roughness forced to a constant: patch step 12.2 → 4.1/255 |
| `2026-11-01-diag-roughness-1200km-continuous.jpg` | same | 200 | `b547028dca0d5228` | roughness | **after**: the field follows the coastline and the depth ramp, no patch-shaped blocks |
| `2026-11-01-diag-roughness-300km-continuous.jpg` | `-8.0,139.5,300000` | 200 | `91985641eb20b3b3` | roughness | **after**: one continuous field across the coast; the edge is mesh resolution, not a patch |
| `2026-11-01-production-1200km-continuous.jpg` | `-8.0,139.5,1200000` | 200 | `f628c5150f195a7d` | production | patch step 12.3 → 4.2/255, matching the constant-roughness A/B |
| `2026-11-01-production-300km-continuous.jpg` | `-8.0,139.5,300000` | 200 | `d3049dbbb67e9c65` | production | mean 1.54/255 from `2026-10-31-300km-papua.jpg`, concentrated at patch edges |
| `2026-11-01-ab-morph-off-height-1200km.jpg` | same | 200 | `8423bdabcdd17c4f` | height, **modified build** | LOD morph disabled: the near-datum pond field is unchanged |

## Negative results worth keeping

* A 7 km attempt at the same coastal pose produced an almost identical featureless
  field, so the 40 km frame is not a one-off. Either the location is under a
  uniform overcast at this epoch or something between camera and ground is filling
  the frame. Recorded as an open observation, not a diagnosis.
* **Both coastline attempts were at longitude 138.6, which is outside the regional
  imagery region (139.0–140.0 E).** The diagnostic `inputs` views show no imagery
  bit at that longitude at 3, 10, 30 or 70 km, so those frames could never have
  shown the 10 m mosaic. The featureless field is not explained by that alone —
  the global overview is still bound — but it does mean the attempt was made
  outside the only region with detailed imagery.
* An earlier attempt to give cloud tops relief via a top-height field measured
  0.03/255 and was removed; the penetration probes also revealed two real bugs
  (probe origin at the hit point, inertial frame where body-fixed was expected).
* A cut-and-fill apron mesh sampled from the terrain source interpenetrated the
  rendered terrain and was reverted in favour of grading the authority.
* **`metallic` renders black for every terrain patch**, because the material never
  sets it above zero. The frame is included as a passing observation of that
  fact, not as an empty view: a non-black pixel would mean the material changed.
* **`detail-weight` is black and `layer-blend` is magenta at 300 km**, and their
  checksums equal `metallic` and `surface-class` respectively. That identity is
  the proof that both modes report "absent" uniformly rather than shading one
  patch differently.

## Rules

1. Do not delete a failing frame. A frame that fails its criterion is the record
   that the defect exists.
2. Never relabel a frame from an older commit as current. Re-capture instead.
3. Report the measured statistic where one exists; a picture alone is not a
   measurement.
4. Keep the image small (JPEG q85, 1280x720). The registry is about reproducibility,
   not archival quality.
