# Progress dashboard

Snapshot date: 2026-11-05.
Last code checkpoint: `c582708` (2026-11-05).
This table describes the 44 tasks in `MASTER_ROADMAP.md` — not the whole
repository and not a percentage of "the simulator being finished". Many tasks are
small slices; a low percentage here means "little of *this realism programme* is
evidenced", not "the simulator does not work".

## Task counts

| state | count | share of roadmap |
| --- | ---: | ---: |
| VERIFIED | 17 | 38.6 % |
| IMPLEMENTED (not verified) | 2 | 4.5 % |
| IN_PROGRESS | 0 | 0 % |
| BLOCKED | 4 | 9.1 % |
| PLANNED | 21 | 47.7 % |
| **total** | **44** | 100 % |

## Phase completion

| phase | verified / total | share | state | blocker |
| --- | ---: | ---: | --- | --- |
| P0 Measurement foundations | 3 / 4 | 75 % | active | P0-04 blocked by presentation |
| P1 Data and terrain correctness | 3 / 11 | 27 % | active | TD-02 submerged pads; TD-06 and TD-11 blocked on baseline isolation; TD-10 payload/cache coupling |
| P2 Launch facility | 1 / 4 | 25 % | active | facility hardware and trench not audited |
| P3 Terrain materials | 1 / 4 | 25 % | active | TM-04 closed; TM-01, TM-02 and TM-03 remain |
| P4 Vegetation | 0 / 3 | 0 % | planned | audit not started |
| P5 Clouds and wind | 4 / 6 | 67 % | active | shape diversity and edge structure remain |
| P6 Atmosphere | 3 / 4 | 75 % | active | AT-01/02/03 verified; AT-04 tracks the single-scattering horizon hue |
| P7 Rocket and VFX | 0 / 2 | 0 % | planned | attitude audit not started |
| P8 Rendering optimisation | 1 / 3 | 33 % | active | performance baseline blocked |
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
| 9 | Three graded pads sit under water: Starbase, KSC LC-39A and Wallops are at −0.78, −0.14 and −13.61 m against the local sea datum | TD-11 (and TD-10) | BLOCKED | `2026-11-05-td02-{starbase,ksc,wallops}.jpg`; Baikonur/Vandenberg/Kourou/Mahia are flush |
| 10 | The visible terrain and the pad anchor depend on the erosion cache and on which payload tiles are resident: Vandenberg's pad samples 81 m above its published package probe, and a warm vs cold cache changes the frame by 90 % of pixels | TD-10 | PLANNED | cold composition probe per site; warm/cold pairs |
| 11 | Windowed presentation fails on this machine | env | external | one present, then swapchain lost |

## Validation status

| check | result | commit |
| --- | --- | --- |
| `cargo fmt --check` | PASS | c582708 |
| `cargo check --all-targets --features dem` | PASS | c582708 |
| `cargo check --no-default-features` | PASS | c582708 |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS | c582708 |
| `cargo test --features dem --lib` | 1142 passed, 0 failed, 2 ignored | c582708 |
| `cargo test --no-default-features --lib` | 1100 passed, 0 failed, 2 ignored | c582708 |
| `openspec validate --all --strict` | 42 passed, 0 failed | c582708 |
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
| launch site | current — one graded pad per predefined site; four flush, three submerged |
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

**Next task — TD-10, decouple the pad anchor and the visible terrain from the
erosion cache.** The TD-02 capture set is complete and did its job: Baikonur,
Vandenberg, Kourou and Mahia show a flush pad, while Starbase, KSC and Wallops
stand in open water. The probe explains why. Sampling the resident payload tile
and then the composed surface differs by up to 81 m (Vandenberg), because the
erosion tile is baked while only the site's finest payload tile is resident, so
most of it comes from the coarse global base. The disk cache then freezes one
version of that field: a warm and a cold cache change 90 % of the frame.

TD-10 is P0 and must land before TD-11 can floor the coastal pads above the sea
datum without guessing at the surface. TD-11 in turn needs a maintainer ruling,
because the committed ascent, gravity-turn and LEO baselines launch from
`ksc-default-geodetic`, which the same probe puts 0.94 m under the sea surface,
so any datum floor moves those baselines. TD-03 and RD-02 remain the other
implemented-but-unverified tasks; AT-04 comes after them because it changes the
sky model, not just its evidence.
