# Progress dashboard

Snapshot date: 2026-11-07.
Last code checkpoint: `1ea0895` (2026-11-07).
This table describes the 45 tasks in `MASTER_ROADMAP.md` — not the whole
repository and not a percentage of "the simulator being finished". Many tasks are
small slices; a low percentage here means "little of *this realism programme* is
evidenced", not "the simulator does not work".

## Task counts

| state | count | share of roadmap |
| --- | ---: | ---: |
| VERIFIED | 38 | 84.4 % |
| IMPLEMENTED (not verified) | 4 | 8.9 % |
| IN_PROGRESS | 0 | 0 % |
| BLOCKED | 2 | 4.4 % |
| PLANNED | 1 | 2.2 % |
| **total** | **45** | 100 % |

## Phase completion

| phase | verified / total | share | state | blocker |
| --- | ---: | ---: | --- | --- |
| P0 Measurement foundations | 3 / 4 | 75 % | active | P0-04 blocked by presentation |
| P1 Data and terrain correctness | 12 / 12 | 100 % | complete | TD-07, TD-08, TD-09 and TD-12 closed |
| P2 Launch facility | 2 / 4 | 50 % | active | LF-02 closed; trench and structural detail remain |
| P3 Terrain materials | 4 / 4 | 100 % | complete | TM-01, TM-02, TM-03 and TM-04 closed |
| P4 Vegetation | 2 / 3 | 67 % | active | VG-01 and VG-02 closed; VG-03 implemented, evidence pending |
| P5 Clouds and wind | 4 / 6 | 67 % | active | CL-05 and CL-06 implemented; evidence pending |
| P6 Atmosphere | 4 / 4 | 100 % | complete | AT-04 closed the single-scattering horizon hue; the cloud-deck edge remains a cloud task |
| P7 Rocket and VFX | 2 / 2 | 100 % | complete | RK-01 and RK-02 closed |
| P8 Rendering optimisation | 2 / 3 | 67 % | active | RD-01 implemented; 600-frame counter evidence pending |
| P9 Final validation | 1 / 3 | 33 % | active | three-mode smoke blocked by presentation |

## Critical unresolved defects

Ordered by visible impact, not by ease.

| # | defect | task | state | evidence |
| --- | --- | --- | --- | --- |
| 1 | Rectangular tone blocks at 300–1,200 km | TD-05 | **VERIFIED** | per-vertex roughness removed the land term; the seam that motivated it now measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km, inside ±1/255. The ocean half was measured with six new water views and **refuted** (no term carries a patch signal, p ≥ 0.12) |
| 2 | Synthetic detail fabricated lakes on the coastal plain | TD-06 | **VERIFIED — fixed** | land-only sea-datum guard; worst-case detail cannot cross the datum, ocean floor keeps signed relief, all three baselines unchanged; Papua A/B mean 1.53/255, `2026-11-05-td06-papua-*` |
| 3 | Land reads nearly black at 400 km while the deck is white | AT-03 | **VERIFIED — explained** | the altitude term is right: at `-4.0,138.0` the same ground measures the same albedo at 400 km and 3 km (0.0169 linear p50), occlusion is exactly 1.000, and the production/albedo ratio matches 300 km (1.77 vs 2.07). Tropical-forest albedo really is ~4 %; the deck is white because it is white. The *near-ground* render was 2.2× brighter because the local surface map carried absolute albedo 4× the overview's. **Fixed by TM-04**: it is now a tint anchored to the overview, so a descent moves the tone +2 % instead of +128 % |
| 4 | Cloud edges are salt-and-pepper cut-outs; one shape per regime | CL-05/06 | PLANNED | `1200km-90f` |
| 5 | Vegetation is sparse and reads as specks | VG-02/03 | PLANNED | VG-01 closed the datum defect and far-field density; the remaining sparseness is canopy geometry and shading; `2026-11-07-vg01-*` |
| 6 | Regional imagery inset still steps at its patch edge | TD-04 | **VERIFIED** | the edge is now a 16.6 km cross-fade at the region bound: the frame's worst step within ±40 px of the bound fell from 4.23 to 0.72/255 (its own texture level), and the before/after change is confined to x=381..737, the declared band. Resolution now uses patch overlap, so a straddling patch binds the tile |
| 7 | Distant terrain has no geometric detail beyond LOD 11 | TM-02 | **VERIFIED — fixed** | mesh-only ~4 km / ±30 m band on LOD 5–11, land-only guard; collision, anchor and coastline unchanged; 40 km height diagnostic shows continuous contours; `2026-11-07-tm02-*` |
| 8 | Noon horizon turns tan at 10 km and pink at 30–70 km; the faint limb tail reads teal | AT-04 | **VERIFIED — fixed** | bounded luminance-preserving diffuse fill; +2° saturation 0.252/0.467/0.655 → 0.204/0.305/0.344 at 10/30/70 km, teal band 0.670 → 0.339, limb e-folding 8.24/8.45 → 8.30/8.33 km; `2026-11-05-at04-*` |
| 9 | Three graded pads sat under water: Starbase, KSC LC-39A and Wallops were −0.78, −0.14 and −13.61 m against the local sea datum | TD-11 | **VERIFIED — fixed** | resident payloads replace the erosion refinement and a 3 m platform floor raises each pad; `2026-11-05-td02-*-fixed.jpg` shows all seven flush; clearances +3.00 … +113.55 m |
| 10 | Terrain outside the resident payload tile depended on the erosion cache: rasters were baked from the streamed payload overlay, so load order and an old disk tile changed height, moisture and flow | TD-10 | **VERIFIED — fixed** | erosion now bakes from the immutable measured global DEM (`5134ed2`); Starbase cold/warm/off agree to 2.4–2.5/255 mean, Baikonur to 3.0–3.2/255, the same as repeat-run noise; `2026-11-05-td10-*` |
| 11 | Windowed presentation fails on this machine | env | external | one present, then swapchain lost |
| 12 | The regional Sentinel-2 mosaic rendered as a pale inset against the Blue Marble overview | TD-03 | **VERIFIED — fixed** | measured per-channel linear gain applied before mips; region/overview linear-luma ratio 3.87× → 1.16×, median before/after ratio matches the inverse gain to 0.2 %; `2026-11-05-td03-albedo-40km-*` |
| 13 | Angular water bodies persist on the Papua plain after the detail guard | TD-12 | **VERIFIED — measured, plus a flooding bug** | the finest NASADEM payload is 4.3 cm below the datum (source class Ocean); the eroded base is +4 m above it, so erosion is not involved. The cap had used its 0.5 m depth-test offset as a flood threshold, submerging measured land at 0.28–0.46 m; classification now follows the datum and the offset is 0.05 m. Pale water-like share 8.19 % → 3.27 %; `2026-11-07-td12-papua-after.jpg` |
| 14 | A separated stage stayed frozen in the sky, and the rebuilt upper-stage mesh sat half an envelope low | RK-01 | **VERIFIED — fixed** | debris render snapshots now advance without a mission state; the serial mesh is built in the stage frame; the debug tripod also used the solar factor and drew at the planet centre; `2026-11-06-rk01-*` |
| 15 | The plume treated an engine as fully on or fully off, ignored gimbal direction, and did not lengthen with altitude | RK-02 | **VERIFIED — fixed** | intensity now follows burned mass flow, length follows the pressure expansion, direction follows the gimbaled thrust axis, and recovering stages drive their own plume; sea level 6.6 m / 96.8 kPa vs 40.4 km 20.7 m / 274 Pa at the same flow fraction; `2026-11-07-rk02-*` |

## Validation status

| check | result | commit |
| --- | --- | --- |
| `cargo fmt --check` | PASS | 98dc788 |
| `cargo check --all-targets --features dem` | PASS | 98dc788 |
| `cargo check --no-default-features` | PASS | 98dc788 |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS | 98dc788 |
| `cargo test --features dem --lib` | 1167 passed, 0 failed, 2 ignored | 98dc788 |
| `cargo test --no-default-features --lib` | 1124 passed, 0 failed, 2 ignored | 98dc788 |
| `openspec validate --all --strict` | 43 passed, 0 failed | 98dc788 |
| `cargo build --release --features dem` | PASS | eba5d6e |
| three-mode smoke (25 s each) | PASS at the time; **blocked now** | `fb686ba` |
| visual capture path | works offscreen; windowed blocked | `d89e7d6` |
| diagnostic capture matrix | 18 terrain frames at 300 km, 18 at 1,200 km, 7 at 3–70 km; the post-fix roughness/production pair; 9 water-view frames; 2 paired pre/post crops | `903d7c8` |
| seam harness | `scripts/measure_patch_seam.py` reproduces the pre-fix seam at +4.14/255 and the post-fix seam at −0.04/255 | `903d7c8` |
| region-edge harness | `scripts/imagery_region_mask.py` + `scripts/measure_tone_edge.py` report the imagery edge at 4.23/255 before and 0.72/255 after, and the change confined to 16.4 km | 81f3c04 |
| TD-05 attribution A/B | constant-roughness and morph-off frames, same pose/clock/frames | `ec97d6e` |
| near-ground tone control | replacing the local albedo mix with the base (same normals, roughness, layers) drops the 3 km ratio from 4.36 to 1.98; the tint build measures 2.01 against 2.03 at 40 km | eba5d6e |

## Visual evidence coverage

| scene | state |
| --- | --- |
| prelaunch | partial (no HUD) |
| launch site | current — one graded pad per predefined site, all flush and dry |
| terrain close-up | current |
| coastline | **missing** |
| clouds | current |
| atmosphere / limb | current — 10/30/70/200/400 km sequence and 200/400 km limb at 6° FOV |
| vegetation | partial |
| rocket ascent | **missing** |
| high altitude | current |
| orbit | **missing** |

One of ten required scenes (coastline) has no usable frame. See `VISUAL_EVIDENCE.md`.

## Performance baseline

| field | value |
| --- | --- |
| Current-commit measurement | **none** |
| Last CPU p50/p95/p99 (prelaunch, release, discrete GPU) | 16.66 / 17.63 / 19.04 ms (2026-09-11) |
| Last CPU p50/p95/p99 (ascent) | 13.53 / 25.28 / 27.36 ms (2026-09-11) |
| Worst frame | not recorded |
| GPU frame time | not exposed |
| Draw calls / triangles | not exposed |
| Memory | 1,261–1,492 MiB GPU; ~1.77 GB RSS (2026-09-11); 610–640 MB RSS (2026-08-23) |
| Terrain streaming | bounded at 128 MiB geometry budget; two worker bakes/frame (2026-09-11) |
| Rocket effects (debug capture, 40 km) | 33 effect entities / 21 visible; engine effect update 0.016–0.022 ms, ground 0.0011–0.0017 ms (2026-11-07) |

The offscreen capture path is pinned to 60 Hz and is therefore **not** a
frame-pacing harness. See `PERFORMANCE_BASELINES.md`.

## OpenSpec status

| item | value |
| --- | --- |
| specs | 40 |
| active changes | 1 — `spaceflight-game-mvp` (5/37 tasks) |
| archived changes | 44 |
| strict validation | 41/41 pass |
| requirement gaps | clouds have no OpenSpec spec (technical spec only); visual atmosphere beyond `atmosphere` is thin |

## Next task

**Next task — VL-03, in-repo visual acceptance suite.** RD-01 is implemented:
every per-frame terrain system is classified, and the vegetation uniform no
longer rewrites inputs that are already current. CL-06 is implemented:
the coverage band is a smoothstep with a resolved lower-quarter edge, and the
march now filters to the coarser of the ray step and the pixel footprint, so a
distant deck stops aliasing into speckle. CL-05 is implemented: the deck's base
volume already carries three Worley octaves, so re-weighting them with an
already-fetched selector gives cells from about 4.5 km down to 1.1 km without a
new fetch or a coordinate warp. VG-03 is implemented: the deck
publishes the exact wind direction it advects with, vegetation reads that one
vector, and sway is graded from each vertex's attachment instead of sliding whole
cards. VG-02 is closed: near-band crowns are closed volumes and the
far band keeps impostors. LF-04 is closed: the clamps sat inside the SLS and
Starship hulls, the tower always had five platforms, and the service arms indexed
a platform a short tower does not have. LF-03 is closed: the streamed mesh cannot
resolve a 14 m excavation at its finest LOD, so a new mesh cutout stops the
rendered ground at the hardstand while collision keeps the same surface.

Evidence and the full gate are deferred to one submission pass after the
remaining implementation slice (VL-03), as requested.
