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
| 1 | prelaunch | CURRENT | `pad-110m-footprint`, `2026-11-06-rk01-prelaunch-axes` | pad and tower; the RK-01 frame adds the body tripod and the retained upper envelope |
| 2 | launch site | CURRENT | `pad-110m-footprint`, `2026-11-05-td02-*-fixed`, `2026-11-07-tm01-vandenberg-pad` | one graded pad per predefined site, all flush and dry; the TM-01 frame repeats Vandenberg after classification was decoupled from the grade |
| 3 | terrain close-up | CURRENT | `terrain-600m`, `2026-11-05-td06-papua-*`, `2026-11-07-td12-papua-after`, `2026-11-07-td07-papua-3km` | 600 m nadir with the guard A/B, the TD-12 repeat, and the 3 km regional-DEM view |
| 4 | coastline | MISSING | `diag-land-sea-1200km` | production coastline still has no usable frame; the diagnostic land/sea view now shows where the classification line falls |
| 5 | clouds | CURRENT | `1200km-90f`, `production-1200km` | deck, edges and shadows visible |
| 6 | atmosphere | CURRENT | `2026-11-05-at04-sky2-10/30/70/200/400km`, `2026-11-05-at04-limb-200/400km-{fov6,linear}` | altitude sequence plus a 6° limb; bounded multiple-scattering fill desaturates the horizon and teal tail, e-folding 8.30/8.33 km in linear light |
| 7 | vegetation | PARTIAL | `terrain-600m` | trees read as isolated specks only |
| 8 | rocket ascent | CURRENT | `2026-11-06-rk01-ascent-axes`, `2026-11-06-rk01-staging-axes`, `2026-11-07-rk02-*` | warm-started production flight; ascent at `T+17.8 s`, separation at `T+27.5 s`, staging frame at `T+29.9 s`; the RK-02 pair shows the plume at sea level and at 40 km |
| 9 | high altitude | CURRENT | `300km-papua`, `production-1200km` | 300 km and 1,200 km nadir |
| 10 | orbit | MISSING | — | 1,200 km is the highest captured |

One of ten required scenes (coastline) has no usable frame. That is the point
of the matrix: it is the most honest measure of how much of the visual programme
is actually evidenced. The diagnostic views added below do not fill these scenes —
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
| `2026-11-07-td07-papua-3km.jpg` | `85d4d88` | `-8.0,139.5,3000` | 42 s | 220 | `ff12f642cee147e9` | regional NASADEM base at 3 km: continuous plain, no angular water polygons | measured relief, no synthetic polygons (passing) |
| `2026-11-07-td07-papua-300km.jpg` | `85d4d88` | `-8.0,139.5,300000` | 42 s | 220 | `fcf403cac5198ff7` | regional base at 300 km: coast, offshore islands, inland water | coast and water bodies resolved (passing) |
| `2026-11-07-tm01-vandenberg-pad.jpg` | `18c78ea` | default chase | 0 s | 240 | `cbfa209bb2858319` | Vandenberg pad after classification was decoupled from the site grade | pad material continuous with the hillside (passing) |
| `2026-11-07-tm02-papua-3km.jpg` | `c1ecb0e` | `-8.0,139.5,3000` | 42 s | 220 | `4be2a7c90eafab44` | fine-LOD control: the far band is inactive | same as TD-07's 3 km view |
| `2026-11-07-tm02-papua-40km.jpg` | `c1ecb0e` | `-8.0,139.5,40000` | 42 s | 220 | `8a0c9c7b823e06e3` | 40 km lowland with the coarse relief band | mottled relief, no flat sheet |
| `2026-11-07-tm02-highlands-40km-height.jpg` | `c1ecb0e` | `-5.0,141.5,40000` | 42 s | 220 | `5af1ba3ac3a7feb5` | unlit height diagnostic over the highlands | continuous contours, no LOD steps |
| `2026-11-07-tm02-papua-300km.jpg` | `c1ecb0e` | `-8.0,139.5,300000` | 42 s | 220 | `6abc624df27e5233` | 300 km context with coast and lakes | relief and coast resolved |
| `2026-11-07-tm03-bight-coast-3km.jpg` | `15f9b63` | `-32.0,132.14,3000` | 42 s | 220 | `e748618a653c0d2c` | Great Australian Bight coast: water cap and albedo coast meet on one line | coastline agrees within one overview texel |
| `2026-11-07-vg01-papua-600m.jpg` | `c76c7be` | `-8.0,139.5,600` | 42 s | 220 | `22914e5c7ff597db` | ground view after the sea-datum tree exclusion | specks on land, none on water |
| `2026-11-07-vg01-papua-3km.jpg` | `c76c7be` | `-8.0,139.5,3000` | 42 s | 220 | `a14c8d8f52cafc4a` | 3 km view of the same distribution | trees only on land |
| `2026-11-07-lf02-starbase-pad-ground.jpg` | `1ea0895` | default chase | 0 s | 240 | `90e0855fe61cc288` | Starbase pad: tanks, blockhouse, floodlights and tower in profile | ground hardware on the pad plane |
| `2026-11-08-lf03-starbase-trench-oblique.jpg` | `aa7da40` | `25.846333,-97.1553,20` (pitch 50, fov 50) | 16:30Z | 240 | `be29f0bc99bbbd86` | Starbase from the north: the flame trench reads as a lined recess with the deflector on its floor | open excavation, rim on the pad plane |
| `2026-11-08-lf03-starbase-trench-nadir.jpg` | `aa7da40` | `25.845959,-97.1553,34` (fov 45) | 16:30Z | 240 | `13d360427176b016` | Nadir close-up of the same excavation | rectangular opening, lining grounded |
| `2026-11-08-lf04-electron-pad.jpg` | `cf3fd0a` | default chase | 16:30Z | 240 | `31c76abbc2b36d75` | Electron 18 m x 1.2 m: facility scale for this family | tower and hardware proportional, clamps clear of the hull |
| `2026-11-08-lf04-falcon9-pad.jpg` | `cf3fd0a` | default chase | 16:30Z | 240 | `ce69a0d8c257b037` | Falcon 9 70 x 3.7: facility scale for this family | tower and hardware proportional, clamps clear of the hull |
| `2026-11-08-lf04-sls-pad.jpg` | `cf3fd0a` | default chase | 16:30Z | 240 | `c9ddfa5ae9e87e38` | SLS 98 x 8.4: facility scale for this family | tower and hardware proportional, clamps clear of the hull |
| `2026-11-08-lf04-starship-pad.jpg` | `cf3fd0a` | default chase | 16:30Z | 240 | `e05ad4eda23d40b6` | Starship 121 x 9.0: facility scale for this family | tower and hardware proportional, clamps clear of the hull |
| `2026-11-08-vg02-papua-300m-crown.jpg` | `8d8d46b` | `-8.0,139.5,300` | 02:40Z | 400 | `b69a1b13fa4998fe` | Papua near field: crowns are closed lobed volumes, not card stacks | each crown has a silhouette and a lit side |
| `2026-11-08-vg02-papua-600m-crown.jpg` | `8d8d46b` | `-8.0,139.5,600` | 02:40Z | 300 | `2eaca1fcdb8fba32` | the same crowns at the edge of the near band | crowns read as volumes, not flat specks |
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

### Multiple-scattering horizon hue (`AT-04`, commit `6f99a0e`)

Same poses, start instant, time (42 s), frame count (200) and 6° field as the
AT-02 sequence, so each `at04` frame is directly comparable with the `at02`
frame above it. The bounded diffuse fill is the only change.

| file | commit | pose, pitch / FOV | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- | --- |
| `2026-11-05-at04-sky2-10km.jpg` | `6f99a0e` | `-4.0,138.0,10000`, 89.2° / 6° | `b3d7f4f5803ac6b9` | +2° horizon pale grey-blue; saturation 0.204 (was 0.252) |
| `2026-11-05-at04-sky2-30km.jpg` | `6f99a0e` | `-4.0,138.0,30000`, 86.5° / 6° | `5a9d00354c818192` | +2° horizon pale blue-grey; saturation 0.305 (was 0.467) |
| `2026-11-05-at04-sky2-70km.jpg` | `6f99a0e` | `-4.0,138.0,70000`, 83.5° / 6° | `c44e4ebd516fedd4` | teal tail desaturated; band saturation 0.339 (was 0.670) |
| `2026-11-05-at04-sky2-200km.jpg` | `6f99a0e` | `-4.0,138.0,200000`, 77.8° / 6° | `a84f898176a310de` | limb still below the +2° 8-bit floor, as before |
| `2026-11-05-at04-sky2-400km.jpg` | `6f99a0e` | `-4.0,138.0,400000`, 72.2° / 6° | `abdf084f602494af` | limb still below the +2° 8-bit floor |
| `2026-11-05-at04-limb-200km-fov6.jpg` | `6f99a0e` | `-4.0,138.0,200000`, 78.8° / 6° | `b94c0409187ce649` | orbital limb arc in production tonemapping |
| `2026-11-05-at04-limb-400km-fov6.jpg` | `6f99a0e` | `-4.0,138.0,400000`, 73.2° / 6° | `09aedb23c54e94d2` | 400 km limb arc |
| `2026-11-05-at04-limb-200km-linear.jpg` | `6f99a0e` | same, `DIAGNOSTIC_MODE=height` | `3f3041e97b16ddf6` | **linear-light** 24–32 km fit: 8.30 km (before 8.24 km on the same method) |
| `2026-11-05-at04-limb-400km-linear.jpg` | `6f99a0e` | same, `DIAGNOSTIC_MODE=height` | `e345201c9778ae92` | **linear-light** 24–32 km fit: 8.33 km (before 8.45 km) |

Measured +2° horizon (mean RGB over rows 350–370, columns 100–1180; saturation
`(max−min)/max`):

| altitude | saturation before | saturation after | B/R before | B/R after |
| --- | ---: | ---: | ---: | ---: |
| 10 km | 0.252 | 0.204 | 1.34 | 1.26 |
| 30 km | 0.467 | 0.305 | 1.88 | 1.44 |
| 70 km | 0.655 | 0.344 | 2.90 | 1.53 |

The 70 km teal band (rows 330–370) moves from 0.670 to 0.339 saturation. The limb
e-folding is unchanged within the fit: 200 km 8.24 → 8.30 km, 400 km
8.45 → 8.33 km, both inside 8 ± 1 km. The archived AT-02 values (8.30/8.54 km)
used a slightly different row sampling; the before/after pair above uses one
method for both.

**Recorded limit:** the exact-horizon tan/pink band at 30 km is the **cloud deck's
own** single-scattered radiance, not the sky dome. This sky-only fill does not
change it; it stays tracked with the cloud-edge work (CL-05/06). The +2° sky hue
and the upper limb tail are the quantities AT-04 changes.

### Engine plume from propulsion state (`RK-02`, commit `6209a73`)

Both frames use Starbase, `2000-01-01T18:46:37Z`, 1280x720, and the
capture-only side tracking camera (`COSMIC_SYSTEMS_CAPTURE_TRACK=side`). A chase
camera looks along the thrust axis, so a plume foreshortens to a dot; the side
view measures the same authoritative attitude in profile.

| file | capture | altitude | pressure | flow fraction | expansion | outer plume | sha256 (first 16) |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `2026-11-07-rk02-sealevel-side.jpg` | T+4.4 s | 386 m | 96,775 Pa | 1.000 | 1.09 | 6.6 m | `b32d088e0d594be0` |
| `2026-11-07-rk02-altitude-side.jpg` | T+43.8 s | 40,351 m | 274 Pa | 1.000 | 2.99 | 20.7 m | `e20a70db235faae8` |

The mass-flow fraction is 1.000 in both frames, so the 3.1x outer-plume length
ratio is the pressure response, not a throttle difference. The altitude frame is
the first capture with the upper stage still at full burn after staging; the
spent first stage appears separately in the RK-01 staging frame.

**Recorded limits:** the layer proportions are presentation constants, not a
gas-dynamic solution. The orange band at mid-body in the altitude frame is the
existing aerodynamic-heating shell, not the plume. Per-layer materials stay
shared, so brightness is not scaled per vehicle.

### Rocket body frame and staging (`RK-01`, commit `a9d51a2`)

All three frames use the same site and start instant (`starbase`,
`2000-01-01T18:46:37Z`), 1280x720, and the capture-only body tripod: `+X` red,
`+Y` green, `+Z` blue. The rods are children of the vehicle, so they inherit the
authoritative body→world quaternion; nothing in this path writes a pose.

| file | sim time | warm start | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- | --- |
| `2026-11-06-rk01-prelaunch-axes.jpg` | T+6.5 s | none | `8c0c1524ecfacf0f` | `+Y` on the pad normal, `+Z` along the pad heading, vehicle on the graded pad |
| `2026-11-06-rk01-ascent-axes.jpg` | T+17.8 s | 768 steps (12 s) | `6b9dd81a5177a0dd` | the tripod follows the gravity-turn attitude at 709.9 m/s at the end of the warm start |
| `2026-11-06-rk01-staging-axes.jpg` | T+29.9 s | 1,762 steps (27.5 s) | `788dfa2dce556715` | spent stage (left) flying on its own attitude and velocity, upper stage carrying the tripod |

Separation occurs at `T+27.5 s`: the log records the upper stage at 1,545.8 m/s
and the spent stage at 1,543.3 m/s, with inertial `+Y` `[0.509, 0.690, −0.515]`
and `[0.506, 0.692, −0.515]` — a 0.2° difference at the staging tick, as the
attitude-preservation requirement demands. Two seconds later the spent stage has
rotated independently, which is the point of the frame: it is no longer the
parent.

The warm start runs the production `FixedUpdate` schedule from a real launch
order. It is the first capture path that can show a firing vehicle without a
window, and it is inert when `COSMIC_SYSTEMS_CAPTURE_WARMUP_STEPS` is unset.

**Recorded limit:** the fairing halves remain cosmetic solids rather than true
half shells, and the 60 m tripod is presentation scale.

### Synthetic detail guard on the coastal plain (`TD-06`, commit `5c7585e`)

Same pose, start instant, time (42 s) and frame count (220) for both frames; the
only difference is the sea-level guard on Earth's detail layer. The pose is the
600 m nadir used by the earlier `terrain-600m` ground frame.

| file | commit | guard | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- | --- |
| `2026-11-05-td06-papua-before.jpg` | `5c7585e` | off | `90eff8dc8ec3274c` | signed detail on the near-datum plain; angular water bodies |
| `2026-11-05-td06-papua-after.jpg` | `5c7585e` | on | `cd1fd925c2601f72` | same scene with the land-only guard |
| `2026-11-05-td06-papua-diff.jpg` | `5c7585e` | — | `471f01c4f5a4bd00` | amplified absolute difference (×3) |

Measured: mean per-pixel difference **1.53/255**; **47,322 of 921,600** pixels
change by more than 8/255 (5.1 %). The change is concentrated on the near-datum
plain and in the depth shading inside the existing water bodies.

Measured context: ETOPO 2022 around the site is **73–85 m** on the WGS 84
ellipsoid and the EGM2008 sea datum there is **71.43 m**, so the plain is
**1.6–13.6 m** above the sea inside a detail octave of **−36..+24 m**. The guard
window (`4..60 m` above the datum) covers it.

**Recorded limit (closed by TD-12):** the two large angular water bodies kept
their extent after the guard (water-like pixels 15,343 → 15,296, with the mask
shifting only inside them). The guard provably removes every detail-only
crossing on land above the datum, so this water was below the datum in the base
surface. TD-12 identified the base as the measured NASADEM payload and the
over-extent as a rendering flood threshold; see the next section.

### Regional DEM at the Papua site (`TD-07`, commit `85d4d88`)

Both frames are the P0-01 pose, default start instant, presentation time 42 s,
220 frames and 1280x720. The regional NASADEM package is composed over the
global 30 arc-second base.

| file | pose | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- |
| `2026-11-07-td07-papua-3km.jpg` | `-8.0,139.5,3000` | `ff12f642cee147e9` | continuous measured plain; the angular water polygons of the global base are gone |
| `2026-11-07-td07-papua-300km.jpg` | `-8.0,139.5,300000` | `fcf403cac5198ff7` | coast, offshore islands and inland water resolved from the measured base |

Measured at the geodetic site (`-8.0, 139.5` → terrain latitude `−7.9471°`):

| quantity | value |
| --- | --- |
| covering payload | level 12, 64×64 grid (finest declared) |
| measured payload height | **71.978 m** (matches the bake probe) |
| global fallback, same point | **77.592 m** |
| refinement | **5.61 m** |

Provenance and checksums: `docs/datasets/earth_regional_papua_v1.md`. NASADEM's
vertical accuracy is metres; absolute survey control is still required for
survey-grade claims.

**Recorded limit:** the 3 km frame shows a 1–2 px intermittent bright line along
one patch boundary. It is a capture-path artefact of viewing 5 km from the
vehicle (the streaming focus), not the elevation data; it is recorded for a
future streaming/LOD audit.

### Near-datum water on the Papua plain (`TD-12`, commit `98dc788`)

Same pose (`-8.0,139.5,600`), default start instant, presentation time 42 s and
220 frames as `2026-11-05-td06-papua-after.jpg`; only the ocean-cap
classification changed.

| file | commit | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- |
| `2026-11-05-td06-papua-after.jpg` | `5c7585e` | `cd1fd925c2601f72` | before: the 0.5 m depth-test offset also acted as a flood threshold |
| `2026-11-07-td12-papua-after.jpg` | `98dc788` | `d59dd3c099cd9d24` | after: classification at the datum, surface offset 0.05 m |

Measured composition at the observed strip (finest resident payload):

| quantity | value |
| --- | --- |
| measured NASADEM height | 71.363 m ellipsoidal |
| EGM2008 sea datum | 71.406–71.408 m |
| measured minus datum | **−0.043 m** |
| source `surface_class` | **Ocean** |
| eroded base (stable global DEM) | +4.0 to +4.1 m above the datum |
| composed height | equals the measured payload |

So the water is measured, not eroded. The *over-extent* was rendering: the cap
tested the bed against `sea datum + 0.5 m`, so measured land at **0.28–0.46 m**
above the datum was drawn as sea wherever a triangle reached a wet vertex.
Classification now follows the datum; the 0.05 m offset only positions the
surface.

Measured: pale near-neutral pixels (channel mean > 110/255 and max−min < 35,
a documented proxy for the pale water surface) fall from **75,498 / 921,600
(8.19 %)** to **30,149 (3.27 %)** — a **60.1 %** reduction. The remaining strip
is the measured 4.3 cm below-datum payload sample.

**Recorded limit:** NASADEM's vertical accuracy is metres (see
`docs/datasets/earth_regional_papua_v1.md`), so the residual strip is at the
limit of the measured data, not a resolved water body. Removing it would require
cross-tile payload smoothing in the ingest pipeline, which would also smooth
real channels.

### Volumetric near-band crowns (`VG-02`, commit `8d8d46b`)

A crown was three stacked crossed billboards plus a crossed pair at every branch
tip. The stacked layers are now a closed jittered ellipsoid shell with outward
normals. The far band keeps its crossed impostor cards.

The first emission order wound the shell inward, so the crown rendered as a
hollow ring with the ground visible through the middle. That is why the outward
winding is documented at the index emission rather than left implicit.

Branch-tip lobes stay cards deliberately: nine shells per tree cost 50 % more of
the reservation and pushed the finest terrain patch out of the streaming budget.

### Open flame trench (`LF-03`, commit `aa7da40`)

Three defects hid the trench, and only the third explained the render.

1. The terrain is graded level across the whole 110 m pad, so the trench box sat
   *below grade*. Opening the deck alone would have exposed flat ground.
2. The height field was lowered, but the streamed mesh samples it. The finest
   Earth patch is 610.8 m across at 33 samples — 19.09 m between vertices — so a
   14 m trench receives at most one vertex. Measured on the real patch: 0 at
   level 13, 1 at level 14. The mesh interpolated back to grade.
3. The lining rim topped out at exactly grade, coplanar with the ground and the
   sea cap, so it z-fought.

The fix cuts the *rendered mesh* where a structure says it covers the ground
(`TerrainMeshCutout`), leaves the height field alone, and rebuilds the lining so
every part straddles its opening edge and meets the pad plane.

The 240-frame chase view is not usable evidence for this task: at ~200 m the
opening is about 20×27 px against near-black concrete. The two frames above are
deliberate close views from different headings.

Known residual: a ~0.4 m sliver of the sea cap is visible at one trench corner,
where the cut exposes ground close to sea level. The `height` diagnostic shows
solid ground there, so it is not a hole through the planet.

### Grounded facility hardware (`LF-02`, commit `1ea0895`)

Reading the spawned mesh bounds found the hold-down clamps floating 0.10 m and the
deluge rails buried 0.25 m. Both are grounded; the spawned-scene AABB test guards
every ground-based facility within 0.05 m.

| file | sha256 (first 16) | demonstrates |
| --- | --- | --- |
| `2026-11-07-lf02-starbase-pad-ground.jpg` | `90e0855fe61cc288` | the Starbase pad with tanks, blockhouse, floodlight poles and tower standing on the deck |

The frame is the default chase view at the TD-02 Starbase instant
(`2000-01-01T18:46:37Z`, 0 s, 240 frames). The clamps and rails are sub-pixel at
this distance; the test reads their live mesh AABBs, which is the authoritative
check.

**Recorded limit:** the flame trench and deflector are still covered by the deck
(LF-03).

### Vegetation distribution (`VG-01`, commit `c76c7be`)

The audit found that the land/sea gate compared the raw ellipsoidal height with
0.5 m. At Papua the EGM2008 undulation is about 71 m, so a shelf 30 m under the
sea passed; the test caught a tree **5.2 m below the sea**. The gate, species
call and treeline now subtract the local datum.

| file | pose | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- |
| `2026-11-07-vg01-papua-600m.jpg` | `-8.0,139.5,600` | `22914e5c7ff597db` | ground view: tree instances on land, none on the water strip |
| `2026-11-07-vg01-papua-3km.jpg` | `-8.0,139.5,3000` | `a14c8d8f52cafc4a` | 3 km view: distribution follows the land |

**Recorded limit:** the ground view still reads sparse. That is canopy geometry
and shading (VG-02/VG-03), not the density gate. There is no density-overlay
diagnostic; the domain tests and these placed-instance frames are the evidence.

### Coastal waterline reconciliation (`TM-03`, commit `15f9b63`)

The water cap is the DEM-below-datum surface; the albedo is a separate measured
product at its own resolution. The control transect is the Great Australian
Bight at 32°S, a clean single cliff crossing.

| file | pose | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- |
| `2026-11-07-tm03-bight-coast-3km.jpg` | `-32.0,132.14,3000` | `e748618a653c0d2c` | the water cap and the albedo coast meet along the same line at 3 km |

Measured along latitude −32.0 from longitude 132.0 to 132.2 at 0.001°:

| quantity | value |
| --- | --- |
| DEM sea-datum crossing | **≈132.138°** |
| albedo luminance-midpoint crossing | **≈132.136°** |
| offset | **0.002° (≈0.2 km)** |
| one 8192-wide overview texel | **0.04395° (≈4.3 km)** |

The albedo waterline is the midpoint between the median luminance of the first
and last twenty samples; open water is almost black and inland ground is bright,
so no per-frame colour threshold is involved.

**Recorded limit:** one clean control does not certify rias, barrier islands or
tidal flats, where sub-texel agreement is not expected. The test is a
reconciliation gate, not a global coastline survey.

### Distant terrain relief band (`TM-02`, commit `c1ecb0e`)

The fine detail octave is ~250 m and cannot be represented below LOD 11 without
aliasing, so distant ground lost its relief. The band is a deterministic ~4 km,
±30 m field added to the rendered mesh only, through a band-pass fade
(`rise 5→8`, `fall 9→11`) and a land-only sea guard.

| file | pose | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- |
| `2026-11-07-tm02-papua-3km.jpg` | `-8.0,139.5,3000` | `4be2a7c90eafab44` | fine LOD: band inactive, matches TD-07's 3 km frame |
| `2026-11-07-tm02-papua-40km.jpg` | `-8.0,139.5,40000` | `8a0c9c7b823e06e3` | 40 km lowland: mottled relief rather than a flat sheet |
| `2026-11-07-tm02-highlands-40km-height.jpg` | `-5.0,141.5,40000` | `5af1ba3ac3a7feb5` | unlit height diagnostic: continuous contour field across the view |
| `2026-11-07-tm02-papua-300km.jpg` | `-8.0,139.5,300000` | `6abc624df27e5233` | 300 km context: coast, lakes and relief |

Measured composition: at a highland point the fine LOD 14 mesh equals collision
exactly, while the LOD 9 mesh differs by more than 1 m (the band plus the fine
octave fade). The band weight is zero at LOD 0 and 5, full at 8, and zero again
at 11+.

Performance (40 km capture): 244 requested / 297 target / 181 visible / 241
resident tiles, 91.6 MiB of the 160 MiB budget, main-thread streaming 1.40 ms,
cold batch 243 ms.

**Recorded limits:** the rendered far field can differ from collision by up to
±30 m at LOD 5–9. Physics never samples there and the land-only weight keeps the
coastline identical; the relief is synthetic presentation, not measured terrain.

### Material classification under grading (`TM-01`, commit `18c78ea`)

Vandenberg is the most rugged predefined site. The frame is the TD-02 ground view
(`2000-01-01T20:20:17Z`, 0 s, 240 frames), repeated after the appearance path was
decoupled from the site grade.

| file | sha256 (first 16) | demonstrates |
| --- | --- | --- |
| `2026-11-07-tm01-vandenberg-pad.jpg` | `cbfa209bb2858319` | the pad apron and the surrounding grass/scrub are continuous; no material disc |

The site is gentle enough that the pre-fix disc was already subtle, so this frame
is a continuity check rather than a strong before/after. The regression is the
steep synthetic test: a 45° hillside graded flat keeps its rock albedo, and a
control shows that classifying the flattened slope would recolour the pad.

**Recorded rule:** geometry, collision, radar altitude and elevation bands follow
the graded surface; slope classification reads the natural surface through
`appearance_height_m`, which must never drive physics.

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

After TD-10 (resident payloads override the erosion refinement) and TD-11 (a
3 m platform floor above the local sea datum), the same seven sites were
re-captured at `17bc6f5`. All seven pads are now flush and dry:

| file | commit | site, UTC | time | frames | sha256 (first 16) | demonstrates | criterion |
| --- | --- | --- | --- | ---: | --- | --- | --- |
| `2026-11-05-td02-starbase-fixed.jpg` | `17bc6f5` | `starbase`, `2000-01-01T18:46:37` | 0 s | 240 | `6cc3b8264530d203` | pad on the measured coastal flat, lagoons beyond | dry pad, +3.00 m above sea |
| `2026-11-05-td02-ksc-fixed.jpg` | `17bc6f5` | `ksc`, `2000-01-01T17:40:25` | 0 s | 240 | `0438e2d7f0a6ae80` | LC-39A pad flush on dark marsh ground, water beyond | dry pad, +13.76 m |
| `2026-11-05-td02-wallops-fixed.jpg` | `17bc6f5` | `wallops`, `2000-01-01T17:19:52` | 0 s | 240 | `2086f9cf2a9189c0` | pad dry beside the coastal lagoons | dry pad, +13.26 m |
| `2026-11-05-td02-baikonur-fixed.jpg` | `17bc6f5` | `baikonur`, `2000-01-01T08:04:47` | 0 s | 240 | `e3635fa969186256` | steppe pad flush | dry pad, +113.55 m |
| `2026-11-05-td02-vandenberg-fixed.jpg` | `17bc6f5` | `vandenberg`, `2000-01-01T20:20:17` | 0 s | 240 | `e30c29a6720a5718` | coastal ridge pad flush; pale synthetic lakes are TD-06 | dry pad, +104.31 m |
| `2026-11-05-td02-kourou-fixed.jpg` | `17bc6f5` | `kourou`, `2000-01-01T15:48:36` | 0 s | 240 | `b4963047703ff27b` | ELA-3 pad flush beside the lagoons | dry pad, +9.19 m |
| `2026-11-05-td02-mahia-fixed.jpg` | `17bc6f5` | `mahia`, `2000-01-01T00:26:33` | 0 s | 240 | `a0a3cfadaf59c938` | peninsula pad flush; pale estuary patches are TD-06 | dry pad, +37.59 m |

Two repeat runs at the same pose differ by mean 2.4/255 (Starbase) and 3.1/255
(Baikonur), with 2.3 % and 4.8 % of pixels above 8/255: inside the documented
near-ground streaming variance, and the pad state is identical in both. The
surrounding terrain outside the resident payload tile was the remaining TD-10
item; the cache A/B/C frames below close it.

### Cache-independent terrain (`TD-10`, commit `5134ed2`)

The erosion rasters are now simulated from the immutable measured global DEM,
not from the streamed payload overlay, so the surrounding height, moisture, flow
and river channels no longer depend on payload load order or on an old disk tile.
Each pair below is the same site, pose and capture clock; the difference is only
`COSMIC_SYSTEMS_EROSION_CACHE`.

| file | commit | site, UTC | cache | frames | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- | ---: | --- | --- |
| `2026-11-05-td10-starbase-cold.jpg` | `5134ed2` | `starbase`, `2000-01-01T18:46:37` | empty dir (cold bake) | 240 | `fab2fad7bbee1850` | pad dry, surrounding land complete |
| `2026-11-05-td10-starbase-warm.jpg` | `5134ed2` | same | same dir, second run | 240 | `4ddaec915041c907` | same scene; mean 2.53/255 from cold |
| `2026-11-05-td10-starbase-off-1200.jpg` | `5134ed2` | same | `off` | 1,200 | `8250b69e1abee9aa` | same scene; 2.37/255 from cold |
| `2026-11-05-td10-starbase-off-240.jpg` | `5134ed2` | same | `off` | 240 | `8c864b3ab4a0d4a5` | **expected incomplete**: runtime bake still streaming, ungenerated water |
| `2026-11-05-td10-baikonur-cold.jpg` | `5134ed2` | `baikonur`, `2000-01-01T08:04:47` | empty dir (cold bake) | 240 | `4515d39c1fc5febc` | green steppe, pad dry |
| `2026-11-05-td10-baikonur-warm.jpg` | `5134ed2` | same | same dir, second run | 240 | `6f13c880b58d4b12` | same scene; 2.96/255 from cold |
| `2026-11-05-td10-baikonur-off-1200.jpg` | `5134ed2` | same | `off` | 1,200 | `c570be1de6d24e2b` | same scene; 3.16/255 from cold |

Measured frame differences (per-pixel max over RGB):

| site | cold vs warm | cold vs off | warm vs off |
| --- | --- | --- | --- |
| Starbase | 2.53/255 mean, 2.59 % > 8 | 2.37, 1.43 % | 2.40, 2.02 % |
| Baikonur | 2.96, 4.00 % | 3.16, 4.67 % | 3.06, 4.45 % |

These are the same magnitude as two identical warm runs (2.4 and 3.1/255), so the
disk cache no longer selects a different field. The frame budget matters: a cold
or cache-off run needs about 1,200 frames at this pose because the runtime bake
is expensive and the 64-tile in-memory cache thrashes; the warm store path
settles by 240. The global startup bake still touches thousands of 2 ° tiles and
belongs to the performance work, not to terrain authority.

### Regional imagery colour match (`TD-03`, commit `458d0b1`)

The regional tiles are installed without the measured gain when
`COSMIC_SYSTEMS_IMAGERY_COLOR_MATCH=off`, so the only difference between the two
frames is the correction. Both are at `-8.0,139.9,40000`, 400 frames,
`COSMIC_SYSTEMS_CAPTURE_TIME=0`, diagnostic mode `albedo` (geographic base colour,
no lighting or atmosphere), and the default epoch (local noon at Papua).

| file | commit | mode | gain | time | frames | sha256 (first 16) | demonstrates |
| --- | --- | --- | --- | --- | ---: | --- | --- |
| `2026-11-05-td03-albedo-40km-before.jpg` | `458d0b1` | `albedo` | off | 0 s | 400 | `ffcd184419567248` | pale regional inset; region is 3.87× the overview |
| `2026-11-05-td03-albedo-40km-after.jpg` | `458d0b1` | `albedo` | on | 0 s | 400 | `42ea7725d8cb1437` | region matches the overview; 1.16× |
| `2026-11-05-td03-production-80km-before.jpg` | `458d0b1` | production | off | 0 s | 400 | `65e4118340427603` | the same pale inset in context |
| `2026-11-05-td03-production-80km-after.jpg` | `458d0b1` | production | on | 0 s | 400 | `632a3c67268b279e` | corrected in context |
| `2026-11-05-td03-albedo-80km-before.jpg` | `458d0b1` | `albedo` | off | 0 s | 400 | `bc7742a2cc9788f2` | mosaic vs overview at 80 km |
| `2026-11-05-td03-albedo-80km-after.jpg` | `458d0b1` | `albedo` | on | 0 s | 400 | `7a4fa01cd0831d09` | tone matched; residual patch edge is the LOD limit below |
| `2026-11-05-td03-lod-80km.jpg` | `458d0b1` | `lod` | on | 0 s | 400 | `0a379f512e4a64c9` | the 80 km edge is a patch/LOD boundary, not the colour term |

Measured on the 40 km albedo pair over fixed pixel blocks (inside x 800–1200,
outside x 100–350, rows 60–660), converted from sRGB to linear:

| quantity | before | after |
| --- | ---: | ---: |
| region linear luma | 0.06667 | 0.01991 |
| overview linear luma | 0.01721 | 0.01721 |
| region / overview | **3.87×** | **1.16×** |
| median per-pixel before/after, R | 4.264 (declared 4.269) | |
| median per-pixel before/after, G | 2.470 (declared 2.464) | |
| median per-pixel before/after, B | 4.705 (declared 4.705) | |

Outside the region the frame is untouched: mean difference **0.007/255**, with 3
of 180,000 pixels above 2/255. The 16.6 km cross-fade is the gradual difference
rise between x ≈ 400 and 560, so the correction and the fade compose.

**Recorded limit:** at 80 km the patches outside the fine focus are coarser than
the package's `min_level: 8`, so they bind the global overview and cannot bind the
mosaic. The tone now matches across that boundary, but the resolution change is
still a straight patch edge. The `lod` frame shows the level change on the same
line. Closing it needs regional tiles below level 8, which is package production,
not a shader term; it is listed as a TD-03 caveat rather than presented as
corrected.

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
