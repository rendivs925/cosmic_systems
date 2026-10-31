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
  ./target/debug/cosmic_systems rocket --site papua
```

The offscreen path does not draw the HUD, so HUD text is not part of any
measurement.

## Required scene matrix

| # | scene | state | frame | note |
| --- | --- | --- | --- | --- |
| 1 | prelaunch | PARTIAL | `pad-110m-footprint` | pad and tower, no HUD, chase angle only |
| 2 | launch site | CURRENT | `pad-110m-footprint` | shows the pad flush after grading |
| 3 | terrain close-up | CURRENT | `terrain-600m` | 600 m nadir; no geometric relief visible |
| 4 | coastline | MISSING | `coast-40km-attempt` | two attempts (7 km, 40 km) rendered a featureless field; cause unknown |
| 5 | clouds | CURRENT | `1200km-90f`, `300km-papua` | deck, edges and shadows visible |
| 6 | atmosphere | MISSING | — | no altitude sequence or limb capture yet |
| 7 | vegetation | PARTIAL | `terrain-600m` | trees read as isolated specks only |
| 8 | rocket ascent | MISSING | — | no offscreen path for firing the vehicle yet |
| 9 | high altitude | CURRENT | `300km-papua` | 300 km nadir |
| 10 | orbit | MISSING | — | 300 km is the highest captured |

Five of ten required scenes have no usable frame. That is the point of the
matrix: it is the most honest measure of how much of the visual programme is
actually evidenced.

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

## Negative results worth keeping

* A 7 km attempt at the same coastal pose produced an almost identical featureless
  field, so the 40 km frame is not a one-off. Either the location is under a
  uniform overcast at this epoch or something between camera and ground is filling
  the frame. Recorded as an open observation, not a diagnosis.
* An earlier attempt to give cloud tops relief via a top-height field measured
  0.03/255 and was removed; the penetration probes also revealed two real bugs
  (probe origin at the hit point, inertial frame where body-fixed was expected).
* A cut-and-fill apron mesh sampled from the terrain source interpenetrated the
  rendered terrain and was reverted in favour of grading the authority.

## Rules

1. Do not delete a failing frame. A frame that fails its criterion is the record
   that the defect exists.
2. Never relabel a frame from an older commit as current. Re-capture instead.
3. Report the measured statistic where one exists; a picture alone is not a
   measurement.
4. Keep the image small (JPEG q85, 1280x720). The registry is about reproducibility,
   not archival quality.
