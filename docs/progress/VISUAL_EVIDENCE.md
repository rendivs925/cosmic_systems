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
[COSMIC_SYSTEMS_CAPTURE_PITCH_DEG=<degrees>] \
[COSMIC_SYSTEMS_CAPTURE_FOV_DEG=<degrees>] \
[COSMIC_SYSTEMS_CAPTURE_UTC=YYYY-MM-DDTHH:MM:SS] \
[COSMIC_SYSTEMS_EROSION_CACHE=<on|off>] \
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
| 2 | launch site | CURRENT | `pad-110m-footprint`, `2026-11-05-td02-*` | one graded pad per predefined site; four flush, three submerged (TD-11) |
| 3 | terrain close-up | CURRENT | `terrain-600m` | 600 m nadir; no geometric relief visible |
| 4 | coastline | MISSING | `diag-land-sea-1200km` | production coastline still has no usable frame; the diagnostic land/sea view now shows where the classification line falls |
| 5 | clouds | CURRENT | `1200km-90f`, `production-1200km` | deck, edges and shadows visible |
| 6 | atmosphere | CURRENT | `2026-11-05-at02-sky2-10/30/70/200/400km`, `2026-11-05-at02-limb-200/400km-fov6` | altitude sequence plus a 6° limb that measures 8.30/8.54 km e-folding in linear light |
| 7 | vegetation | PARTIAL | `terrain-600m` | trees read as isolated specks only |
| 8 | rocket ascent | MISSING | — | no offscreen path for firing the vehicle yet |
| 9 | high altitude | CURRENT | `300km-papua`, `production-1200km` | 300 km and 1,200 km nadir |
| 10 | orbit | MISSING | — | 1,200 km is the highest captured |

Three of ten required scenes have no usable frame. That is the point of the
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
| `2026-11-01-diag-water-depth-1200km.jpg` | `-8.0,139.5,1200000` | 200 | `676b710ce8eb9cec` | water-depth | continuous contours; no straight patch edge |
| `2026-11-01-diag-water-body-1200km.jpg` | same | 200 | `3016c3b18c53d142` | water-body | organic shelf/abyss ramp, no rectangular structure |
| `2026-11-01-diag-water-opacity-1200km.jpg` | same | 200 | `30cf59901479ae0e` | water-opacity | smooth alpha ramp, no patch step |
| `2026-11-01-diag-water-shadow-1200km.jpg` | same | 200 | `cce7ec945fe63642` | water-shadow | exactly (1,1,1) over the open sea; salmon is cloud shadow only |
| `2026-11-01-diag-water-reflection-1200km.jpg` | same | 200 | `3ca7ca4fc6f8657f` | water-reflection | near zero at nadir; the grey shapes are cloud shadow |
| `2026-11-01-diag-water-uv1-1200km.jpg` | same | 200 | `690c04f211e650c4` | water-uv1 | the cap's own patch layout, and it agrees with the terrain `patch-boundary` view |
| `2026-11-01-diag-water-depth-300km.jpg` | `-8.0,139.5,300000` | 200 | `9fee428d49c37716` | water-depth | continuous contours across the shelf |
| `2026-11-01-diag-water-body-300km.jpg` | same | 200 | `d34f11b1a6e1f4ff` | water-body | continuous shallow shelf; land drawn in production colours |
| `2026-11-01-diag-water-opacity-300km.jpg` | same | 200 | `7d04e2f9dee60f20` | water-opacity | opaque offshore, ramping only at the shore |
| `2026-11-01-td05-pair-300km.jpg` | `-8.0,139.5,300000` | 200 | `6f155973acb77dc5` | paired crop, 3× nearest | **left pre-fix, right post-fix**: the pale wedge across the lowland disappears |
| `2026-11-01-td05-pair-1200km.jpg` | `-8.0,139.5,1200000` | 200 | `79db3fd10a43bc52` | paired crop, 3× nearest | **left pre-fix, right post-fix**: the lighter patch-shaped wedge is gone; seam excess +4.14 → −0.04/255 |
| `2026-11-01-td04-albedo-40km-before.jpg` | `-8.0,139.9,40000` | 200 | `71f9188683c9f414` | albedo | the imagery inset ends on a hard patch-shaped corner at x ≈ 381/459, not on the region bound at x = 400 |
| `2026-11-01-td04-albedo-40km-after.jpg` | same | 200 | `441afe4aa4535d4a` | albedo | the same edge is now a soft ramp centred on the region bound; no column exceeds the frame's texture level |
| `2026-11-01-td04-pair-40km.jpg` | same | 200 | `e145fe546300b135` | paired crop, 2× nearest | **left before, right after**: the hard corner is gone; the change is confined to the declared band |
| `2026-11-01-td04-albedo-80km-after.jpg` | `-8.0,139.9,80000` | 200 | `e9d6c5ac99fe3218` | albedo | no inset rectangle and no LOD-boundary rectangle at 80 km |
| `2026-11-01-td04-production-40km-after.jpg` | `-8.0,139.9,40000` | 200 | `258748115c5b497e` | production | context only: the cloud deck covers most of the ground at 40 km, so the imagery edge is measured on the albedo view |
| `2026-11-01-diag-patch-boundary-40km.jpg` | same | 200 | `3478f268b8a8ec97` | patch-boundary | patch edges at x = 145, 302, 381, 459, 617, 774, 930, 1086, 1242; the before frame's imagery edge is on two of them |
| `2026-11-03-tm04-3km-before.jpg` | `-4.0,138.0,3000` | 200 | `5f00031c86c43945` | production | the near ground at the old absolute local albedo: 4.36x its own albedo view, i.e. 2.2x the same ground from orbit |
| `2026-11-03-tm04-3km-after.jpg` | same | 200 | `fb73a2bbeb0b918e` | production | the same ground with the local map tinted to the measured overview: 2.01x its albedo, against 2.03x at 40 km |
| `2026-11-03-tm04-pair-3km.jpg` | same | 200 | `e4514390b944b733` | paired crop, 2x nearest | **left before, right after**: the tone the camera saw from 3 km now matches what it sees from orbit; material contrast is carried by the map's chromaticity and patch mean |
| `2026-11-03-tm04-albedo-3km.jpg` | same | 200 | `d4339b9f58458724` | albedo | the measured overview beneath the frame (0.0169 linear p50); identical at 40 km, and unchanged by the fix |
| `2026-11-03-tm04-400km-after.jpg` | `-4.0,138.0,400000` | 200 | `fe7d5a8b4c5045b1` | production | the orbital reference: the detail weight is zero here, and this frame is unchanged by TM-04 (the 40/300/400 km frames measure identically before and after) |
| `2026-11-01-at03-prod-400km.jpg` | `-4.0,138.0,400000` | 200 | `338a2cdd4f96b47f` | production | land dark, deck white: the AT-03 symptom |
| `2026-11-01-at03-albedo-400km.jpg` | same | 200 | `64854b29621aca66` | albedo | the albedo itself is dark: ~4 % linear reflectance over Borneo forest |
| `2026-11-01-at03-prod-3km.jpg` | `-4.0,138.0,3000` | 200 | `3794352ca3a8631d` | production | the same ground at 3 km: bright green, 2.1× the 300 km production luminance |
| `2026-11-01-at03-albedo-3km.jpg` | same | 200 | `464c36804dad5d17` | albedo | **identical to the 400 km albedo** (0.0169 linear p50), so the altitude path is right |
| `2026-11-01-at03-inputs-3km.jpg` | same | 200 | `5bd54c265bef0691` | inputs | `(255,255,171)` = LOCAL_SURFACES + LAYER_WEIGHTS + OCCLUSION, no imagery: the near-ground material inputs that replace the overview |
| `2026-11-01-ab-morph-off-height-1200km.jpg` | same | 200 | `8423bdabcdd17c4f` | height, **modified build** | LOD morph disabled: the near-datum pond field is unchanged |

### Atmosphere altitude sequence and limb (`AT-01`/`AT-02`, commit `b46ee1b`)

All frames were captured at `-4.0,138.0`, time 42 s, 200 frames, with the new
measurement-only `COSMIC_SYSTEMS_CAPTURE_PITCH_DEG` (0 = nadir, 90 = local
horizon) and `COSMIC_SYSTEMS_CAPTURE_FOV_DEG`. The `sky2` rows look at the same
+2° elevation above the local horizon at each altitude; the `fov6` rows frame the
limb with one image row ≈ 0.25–0.33 km of tangent altitude (the 20° rows are
≈ 1 km). The `-linear` rows are the same pose with `DIAGNOSTIC_MODE=height`,
which switches the camera to `Tonemapping::None` so the profile is linear light;
it also false-colours the terrain and hides the deck, both below the sampled sky
rows, while the sky keeps its production shading. They
are JPEG q85 conversions
of the lossless capture, as the registry rules require; the profile fits were
repeated on the archived JPEGs.

| file | commit | pose, pitch / FOV | time | frames | sha256 (first 16) | demonstrates | pass criterion |
| --- | --- | --- | --- | ---: | --- | --- | --- |
| `2026-11-05-at02-sky2-10km.jpg` | `b46ee1b` | `-4.0,138.0,10000`, 89.2° / 6° | 42 s | 200 | `f413ac8c6c67fde3` | horizon +2° at 10 km: pale grey-blue sky, deck and sea below | horizon luminance 0.138 (linear) |
| `2026-11-05-at02-sky2-30km.jpg` | `b46ee1b` | `-4.0,138.0,30000`, 86.5° / 6° | 42 s | 200 | `b7e819c0490f2c92` | same view at 30 km: pink/tan low-luminance horizon | luminance 0.073; continuous with 10 km |
| `2026-11-05-at02-sky2-70km.jpg` | `b46ee1b` | `-4.0,138.0,70000`, 83.5° / 6° | 42 s | 200 | `ea62221e50da47a7` | same view at 70 km: dark blue band, teal tail above the deck | luminance 0.026; no step |
| `2026-11-05-at02-sky2-200km.jpg` | `b46ee1b` | `-4.0,138.0,200000`, 77.8° / 6° | 42 s | 200 | `c2f5f3cd78321408` | same elevation is black space; the limb sits lower in the frame | below the 8-bit floor as expected for 51 km tangent |
| `2026-11-05-at02-sky2-400km.jpg` | `b46ee1b` | `-4.0,138.0,400000`, 72.2° / 6° | 42 s | 200 | `febfb58229497856` | same elevation is black space; limb visible at 77 km tangent | below the 8-bit floor |
| `2026-11-05-at02-limb-10km.jpg` | `b46ee1b` | `-4.0,138.0,10000`, 87.2° / 20° | 42 s | 200 | `b6138961c5fd24ed` | inside the cloud slab: bright cloud tops, dark surface, horizon at mid-frame | qualitative: no shell, no switch |
| `2026-11-05-at02-limb-30km.jpg` | `b46ee1b` | `-4.0,138.0,30000`, 84.5° / 20° | 42 s | 200 | `ac84ac7ef76d064d` | deck below, horizon band; cloud edges are hard | qualitative |
| `2026-11-05-at02-limb-70km.jpg` | `b46ee1b` | `-4.0,138.0,70000`, 81.5° / 20° | 42 s | 200 | `233a4dcb9f14e7dd` | limb against black above the deck | thin band, not a shell |
| `2026-11-05-at02-limb-200km.jpg` | `b46ee1b` | `-4.0,138.0,200000`, 75.8° / 20° | 42 s | 200 | `a6f90c9eab48629f` | orbital limb: thin blue arc, black space above | arc visible; no flat plate |
| `2026-11-05-at02-limb-400km.jpg` | `b46ee1b` | `-4.0,138.0,400000`, 70.2° / 20° | 42 s | 200 | `26f500025f63ecb6` | orbital limb at 400 km | arc visible |
| `2026-11-05-at02-limb-200km-fov6.jpg` | `b46ee1b` | `-4.0,138.0,200000`, 78.8° / 6° | 42 s | 200 | `6a66e68fbea7a16b` | **measured** limb profile: display e-folding 8.76 km over 24–32 km tangent | 8 ± 1 km |
| `2026-11-05-at02-limb-400km-fov6.jpg` | `b46ee1b` | `-4.0,138.0,400000`, 73.2° / 6° | 42 s | 200 | `dbb6b3ec146b8632` | **measured** limb profile: display e-folding 8.87 km; displayable top 38 km | 8 ± 1 km |
| `2026-11-05-at02-limb-200km-linear.jpg` | `b46ee1b` | `-4.0,138.0,200000`, 78.8° / 6°, `DIAGNOSTIC_MODE=height` | 42 s | 200 | `6e927ee1be32dbb0` | **linear-light** profile (tonemapping bypassed): e-folding 8.30 km over 24–32 km | 8 ± 0.8 km |
| `2026-11-05-at02-limb-400km-linear.jpg` | `b46ee1b` | `-4.0,138.0,400000`, 73.2° / 6°, `DIAGNOSTIC_MODE=height` | 42 s | 200 | `177750fa1e23d880` | **linear-light** profile (tonemapping bypassed): e-folding 8.54 km | 8 ± 0.8 km |
| `2026-11-05-at02-nadir-30km.jpg` | `b46ee1b` | `-4.0,138.0,30000`, nadir / default | 42 s | 200 | `6c826e5b4c96cda1` | the "dark 30 km" view: dark forest, bright cloud puffs | ground p50 0.0286 vs 0.0337 at 3 km (−15 %) |
| `2026-11-05-at02-nadir-200km.jpg` | `b46ee1b` | `-4.0,138.0,200000`, nadir / default | 42 s | 200 | `41592245f5ef4a1b` | same forest at 200 km: dark green, white deck | ground p50 0.0304 (−10 %); no altitude veil |

### Launch-site grading per predefined site (`TD-02`, commit `c582708`)

The capture pinning the start instant is new: at the shared default epoch
(2000-01-01 03:00 UTC) only Papua is in daylight. Each frame below sets
`COSMIC_SYSTEMS_CAPTURE_UTC` so the site sees the same solar hour angle Papua
sees (`utc = 03:00 + (139.5 − longitude) / 15 h`), keeps the default chase
camera, and renders 240 frames at presentation time 0.

The terrain around these frames is read through the erosion field, which is the
subject of TD-10; treat the four clean rows as the site-grading result and the
three water rows as the defect they expose, not as a site-grading failure.

| file | commit | site, UTC | time | frames | sha256 (first 16) | demonstrates | criterion |
| --- | --- | --- | --- | ---: | --- | --- | --- |
| `2026-11-05-td02-baikonur.jpg` | `c582708` | `baikonur`, `2000-01-01T08:04:47` | 0 s | 240 | `1aaa39400047cffc` | steppe pad flush, no floating or buried edge | pass |
| `2026-11-05-td02-vandenberg.jpg` | `c582708` | `vandenberg`, `2000-01-01T20:20:17` | 0 s | 240 | `04fe9bf0102e354f` | coastal ridge pad flush; pale synthetic lakes in the distance are TD-06 | pass |
| `2026-11-05-td02-kourou.jpg` | `c582708` | `kourou`, `2000-01-01T15:48:36` | 0 s | 240 | `a2ca8dffd72ede64` | pad flush beside the estuary | pass |
| `2026-11-05-td02-mahia.jpg` | `c582708` | `mahia`, `2000-01-01T00:26:33` | 0 s | 240 | `c5b411e866446052` | peninsula pad flush; pale estuary patches are TD-06 | pass |
| `2026-11-05-td02-starbase.jpg` | `c582708` | `starbase`, `2000-01-01T18:46:37` | 0 s | 240 | `2b287f4ad37f14ee` | pad and tower standing in open water | **fail** — ground is −0.78 m against the sea datum |
| `2026-11-05-td02-ksc.jpg` | `c582708` | `ksc`, `2000-01-01T17:40:25` | 0 s | 240 | `3259fd2c738901fa` | pad and tower in water | **fail** — −0.14 m |
| `2026-11-05-td02-wallops.jpg` | `c582708` | `wallops`, `2000-01-01T17:19:52` | 0 s | 240 | `509abf95728c5a2a` | pad and tower in open water | **fail** — −13.61 m |
| `2026-11-05-td02-baikonur-cold.jpg` | `c582708` | same, `COSMIC_SYSTEMS_EROSION_CACHE=off` | 0 s | 240 | `fc86abde3a74f20f` | cold erosion cache: same pad, brown desert instead of green steppe | diagnostic for TD-10, not site evidence |
| `2026-11-05-td02-starbase-cold.jpg` | `c582708` | same, `COSMIC_SYSTEMS_EROSION_CACHE=off` | 0 s | 240 | `a7fd60edf982e15c` | cold erosion cache: surrounding land does not render at all | diagnostic for TD-10, not site evidence |

The cold frames are also incomplete at frame 240 because a from-scratch erosion
bake is slower than streaming, so they are evidence about cache dependence, not
about the site.

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
