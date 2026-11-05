# Progress dashboard

Snapshot date: 2026-11-05.
Last code checkpoint: `e0ea385` (2026-11-05).
This table describes the 44 tasks in `MASTER_ROADMAP.md` — not the whole
repository and not a percentage of "the simulator being finished". Many tasks are
small slices; a low percentage here means "little of *this realism programme* is
evidenced", not "the simulator does not work".

## Task counts

| state | count | share of roadmap |
| --- | ---: | ---: |
| VERIFIED | 22 | 50.0 % |
| IMPLEMENTED (not verified) | 0 | 0 % |
| IN_PROGRESS | 0 | 0 % |
| BLOCKED | 3 | 6.8 % |
| PLANNED | 19 | 43.2 % |
| **total** | **44** | 100 % |

## Phase completion

| phase | verified / total | share | state | blocker |
| --- | ---: | ---: | --- | --- |
| P0 Measurement foundations | 3 / 4 | 75 % | active | P0-04 blocked by presentation |
| P1 Data and terrain correctness | 7 / 11 | 64 % | active | TD-02, TD-03, TD-10 and TD-11 verified; TD-06 and the audits remain |
| P2 Launch facility | 1 / 4 | 25 % | active | facility hardware and trench not audited |
| P3 Terrain materials | 1 / 4 | 25 % | active | TM-04 closed; TM-01, TM-02 and TM-03 remain |
| P4 Vegetation | 0 / 3 | 0 % | planned | audit not started |
| P5 Clouds and wind | 4 / 6 | 67 % | active | shape diversity and edge structure remain |
| P6 Atmosphere | 3 / 4 | 75 % | active | AT-01/02/03 verified; AT-04 tracks the single-scattering horizon hue |
| P7 Rocket and VFX | 0 / 2 | 0 % | planned | attitude audit not started |
| P8 Rendering optimisation | 2 / 3 | 67 % | active | RD-01 needs the presentation baseline |
| P9 Final validation | 1 / 3 | 33 % | active | three-mode smoke blocked by presentation |

## Critical unresolved defects

Ordered by visible impact, not by ease.

| # | defect | task | state | evidence |
| --- | --- | --- | --- | --- |
| 1 | Rectangular tone blocks at 300–1,200 km | TD-05 | **VERIFIED** | per-vertex roughness removed the land term; the seam that motivated it now measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km, inside ±1/255. The ocean half was measured with six new water views and **refuted** (no term carries a patch signal, p ≥ 0.12) |
| 2 | Synthetic detail fabricates lakes on the coastal plain | TD-06 | BLOCKED | visible in `terrain-600m`; guard implemented but inert |
| 3 | Land reads nearly black at 400 km while the deck is white | AT-03 | **VERIFIED — explained** | the altitude term is right: at `-4.0,138.0` the same ground measures the same albedo at 400 km and 3 km (0.0169 linear p50), occlusion is exactly 1.000, and the production/albedo ratio matches 300 km (1.77 vs 2.07). Tropical-forest albedo really is ~4 %; the deck is white because it is white. The *near-ground* render was 2.2× brighter because the local surface map carried absolute albedo 4× the overview's. **Fixed by TM-04**: it is now a tint anchored to the overview, so a descent moves the tone +2 % instead of +128 % |
| 4 | Cloud edges are salt-and-pepper cut-outs; one shape per regime | CL-05/06 | PLANNED | `1200km-90f` |
| 5 | Vegetation is sparse and reads as specks | VG-01/02 | PLANNED | `terrain-600m` |
| 6 | Regional imagery inset still steps at its patch edge | TD-04 | **VERIFIED** | the edge is now a 16.6 km cross-fade at the region bound: the frame's worst step within ±40 px of the bound fell from 4.23 to 0.72/255 (its own texture level), and the before/after change is confined to x=381..737, the declared band. Resolution now uses patch overlap, so a straddling patch binds the tile |
| 7 | Distant terrain has no geometric detail beyond LOD 11 | TM-02 | PLANNED | `300km-papua` |
| 8 | Noon horizon turns tan at 10 km and pink at 30–70 km; the faint limb tail reads teal | AT-04 | PLANNED | `2026-11-05-at02-sky2-10/30/70km`; the orbital limb itself is blue and measures 8.30/8.54 km e-folding |
| 9 | Three graded pads sat under water: Starbase, KSC LC-39A and Wallops were −0.78, −0.14 and −13.61 m against the local sea datum | TD-11 | **VERIFIED — fixed** | resident payloads replace the erosion refinement and a 3 m platform floor raises each pad; `2026-11-05-td02-*-fixed.jpg` shows all seven flush; clearances +3.00 … +113.55 m |
| 10 | Terrain outside the resident payload tile depended on the erosion cache: rasters were baked from the streamed payload overlay, so load order and an old disk tile changed height, moisture and flow | TD-10 | **VERIFIED — fixed** | erosion now bakes from the immutable measured global DEM (`5134ed2`); Starbase cold/warm/off agree to 2.4–2.5/255 mean, Baikonur to 3.0–3.2/255, the same as repeat-run noise; `2026-11-05-td10-*` |
| 11 | Windowed presentation fails on this machine | env | external | one present, then swapchain lost |
| 12 | The regional Sentinel-2 mosaic rendered as a pale inset against the Blue Marble overview | TD-03 | **VERIFIED — fixed** | measured per-channel linear gain applied before mips; region/overview linear-luma ratio 3.87× → 1.16×, median before/after ratio matches the inverse gain to 0.2 %; `2026-11-05-td03-albedo-40km-*` |

## Validation status

| check | result | commit |
| --- | --- | --- |
| `cargo fmt --check` | PASS | e0ea385 |
| `cargo check --all-targets --features dem` | PASS | e0ea385 |
| `cargo check --no-default-features` | PASS | e0ea385 |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS | e0ea385 |
| `cargo test --features dem --lib` | 1148 passed, 0 failed, 2 ignored | e0ea385 |
| `cargo test --no-default-features --lib` | 1106 passed, 0 failed, 2 ignored | e0ea385 |
| `openspec validate --all --strict` | 42 passed, 0 failed | e0ea385 |
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

Three of ten required scenes have no usable frame. See `VISUAL_EVIDENCE.md`.

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

The offscreen capture path is pinned to 60 Hz and is therefore **not** a
frame-pacing harness. See `PERFORMANCE_BASELINES.md`.

## OpenSpec status

| item | value |
| --- | --- |
| specs | 40 |
| active changes | 2 — `spaceflight-game-mvp` (5/37 tasks), `stabilize-far-field-tree-placement` (tasks complete, not archived) |
| archived changes | 29 |
| strict validation | 42/42 pass |
| requirement gaps | clouds have no OpenSpec spec (technical spec only); visual atmosphere beyond `atmosphere` is thin |

## Next task

**Next task — AT-04, multiple scattering at the horizon.** The single-scattering
sky reddens the 10–70 km noon horizon; the measured limb e-folding is 8.30/8.54
km and the fix must keep 24–32 km within 8 ± 1 km. AT-04 needs an OpenSpec change
and a bounded multiple-scattering term.

RD-02 is closed: the CPU phases are now measured separately (culling, LOD
reconciliation, upload burst) in `PERFORMANCE_BASELINES.md`, with the offscreen
runner's frame times explicitly excluded because they are the runner's cadence.
The full frame baseline (P0-04) and RD-01 still need a presentable display; TD-06's
guard re-evaluation and the remaining terrain audits follow.
