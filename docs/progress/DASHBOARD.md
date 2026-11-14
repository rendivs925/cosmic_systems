# Progress dashboard

Snapshot date: 2026-11-14.
Recovery checkpoint: this change, based on `bc30c4d`.
This table describes the 45 tasks in `MASTER_ROADMAP.md` — not the whole
repository and not a percentage of "the simulator being finished". Many tasks are
small slices; a low percentage here means "little of *this realism programme* is
evidenced", not "the simulator does not work".

## Task counts

| state | count | share of roadmap |
| --- | ---: | ---: |
| VERIFIED | 45 | 100 % |
| IMPLEMENTED (not verified) | 0 | 0 % |
| IN_PROGRESS | 0 | 0 % |
| BLOCKED | 0 | 0 % |
| PLANNED | 0 | 0 % |
| **total** | **45** | 100 % |

## Phase completion

| phase | verified / total | share | state | blocker |
| --- | ---: | ---: | --- | --- |
| P0 Measurement foundations | 4 / 4 | 100 % | complete | P0-01…04 closed |
| P1 Data and terrain correctness | 12 / 12 | 100 % | complete | TD-07, TD-08, TD-09 and TD-12 closed |
| P2 Launch facility | 4 / 4 | 100 % | complete | LF-01…04 closed |
| P3 Terrain materials | 4 / 4 | 100 % | complete | TM-01, TM-02, TM-03 and TM-04 closed |
| P4 Vegetation | 3 / 3 | 100 % | complete | VG-01…03 closed |
| P5 Clouds and wind | 6 / 6 | 100 % | complete | CL-01…06 closed |
| P6 Atmosphere | 4 / 4 | 100 % | complete | AT-04 closed the single-scattering horizon hue; the cloud-deck edge remains a cloud task |
| P7 Rocket and VFX | 2 / 2 | 100 % | complete | RK-01 and RK-02 closed |
| P8 Rendering optimisation | 3 / 3 | 100 % | complete | RD-01…03 closed |
| P9 Final validation | 3 / 3 | 100 % | complete | VL-01…03 closed |

## Critical unresolved defects

Ordered by visible impact, not by ease.

| # | defect | task | state | evidence |
| --- | --- | --- | --- | --- |
| 1 | Rectangular tone blocks at 300–1,200 km | TD-05 | **VERIFIED** | per-vertex roughness removed the land term; the seam that motivated it now measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km, inside ±1/255. The ocean half was measured with six new water views and **refuted** (no term carries a patch signal, p ≥ 0.12) |
| 2 | Synthetic detail fabricated lakes on the coastal plain | TD-06 | **VERIFIED — fixed** | land-only sea-datum guard; worst-case detail cannot cross the datum, ocean floor keeps signed relief, all three baselines unchanged; Papua A/B mean 1.53/255, `2026-11-05-td06-papua-*` |
| 3 | Land reads nearly black at 400 km while the deck is white | AT-03 | **VERIFIED — explained** | the altitude term is right: at `-4.0,138.0` the same ground measures the same albedo at 400 km and 3 km (0.0169 linear p50), occlusion is exactly 1.000, and the production/albedo ratio matches 300 km (1.77 vs 2.07). Tropical-forest albedo really is ~4 %; the deck is white because it is white. The *near-ground* render was 2.2× brighter because the local surface map carried absolute albedo 4× the overview's. **Fixed by TM-04**: it is now a tint anchored to the overview, so a descent moves the tone +2 % instead of +128 % |
| 4 | Cloud edges are salt-and-pepper cut-outs; one shape per regime | CL-05/06 | **VERIFIED — fixed** | cell-size spectrum (4.5–1.1 km from the existing octaves) and a smoothstep coverage edge with footprint-filtered mips; bright share 0.0350 vs 0.0332 at 1,200 km, inside ±0.02; `2026-11-08-cl05-deck-1200km.jpg`, `2026-11-08-cl06-deck-400km.jpg` |
| 5 | Vegetation is sparse and reads as specks | VG-02/03 | **VERIFIED — improved** | closed lobed crown volumes plus graded wind sway; near-band crowns now read as volumes, and the far band keeps impostors for density; `2026-11-08-vg02-papua-300m-crown.jpg`, `2026-11-08-vg03-wind-*` |
| 6 | Regional imagery inset still steps at its patch edge | TD-04 | **VERIFIED** | the edge is now a 16.6 km cross-fade at the region bound: the frame's worst step within ±40 px of the bound fell from 4.23 to 0.72/255 (its own texture level), and the before/after change is confined to x=381..737, the declared band. Resolution now uses patch overlap, so a straddling patch binds the tile |
| 7 | Distant terrain has no geometric detail beyond LOD 11 | TM-02 | **VERIFIED — fixed** | mesh-only ~4 km / ±30 m band on LOD 5–11, land-only guard; collision, anchor and coastline unchanged; 40 km height diagnostic shows continuous contours; `2026-11-07-tm02-*` |
| 8 | Noon horizon turns tan at 10 km and pink at 30–70 km; the faint limb tail reads teal | AT-04 | **VERIFIED — fixed** | bounded luminance-preserving diffuse fill; +2° saturation 0.252/0.467/0.655 → 0.204/0.305/0.344 at 10/30/70 km, teal band 0.670 → 0.339, limb e-folding 8.24/8.45 → 8.30/8.33 km; `2026-11-05-at04-*` |
| 9 | Three graded pads sat under water: Starbase, KSC LC-39A and Wallops were −0.78, −0.14 and −13.61 m against the local sea datum | TD-11 | **VERIFIED — fixed** | resident payloads replace the erosion refinement and a 3 m platform floor raises each pad; `2026-11-05-td02-*-fixed.jpg` shows all seven flush; clearances +3.00 … +113.55 m |
| 10 | Terrain outside the resident payload tile depended on the erosion cache: rasters were baked from the streamed payload overlay, so load order and an old disk tile changed height, moisture and flow | TD-10 | **VERIFIED — fixed** | erosion now bakes from the immutable measured global DEM (`5134ed2`); Starbase cold/warm/off agree to 2.4–2.5/255 mean, Baikonur to 3.0–3.2/255, the same as repeat-run noise; `2026-11-05-td10-*` |
| 11 | Windowed presentation failed on this machine | env | **resolved for validation** | the 2026-11-09 X11/i3 session presents all three modes; with the default AutoVsync the loop quantizes to ~1 s per present, so P0-04 uses the documented vsync-off profiling mode |
| 12 | The regional Sentinel-2 mosaic rendered as a pale inset against the Blue Marble overview | TD-03 | **VERIFIED — fixed** | measured per-channel linear gain applied before mips; region/overview linear-luma ratio 3.87× → 1.16×, median before/after ratio matches the inverse gain to 0.2 %; `2026-11-05-td03-albedo-40km-*` |
| 13 | Angular water bodies persist on the Papua plain after the detail guard | TD-12 | **VERIFIED — measured, plus a flooding bug** | the finest NASADEM payload is 4.3 cm below the datum (source class Ocean); the eroded base is +4 m above it, so erosion is not involved. The cap had used its 0.5 m depth-test offset as a flood threshold, submerging measured land at 0.28–0.46 m; classification now follows the datum and the offset is 0.05 m. Pale water-like share 8.19 % → 3.27 %; `2026-11-07-td12-papua-after.jpg` |
| 14 | A separated stage stayed frozen in the sky, and the rebuilt upper-stage mesh sat half an envelope low | RK-01 | **VERIFIED — fixed** | debris render snapshots now advance without a mission state; the serial mesh is built in the stage frame; the debug tripod also used the solar factor and drew at the planet centre; `2026-11-06-rk01-*` |
| 15 | The plume treated an engine as fully on or fully off, ignored gimbal direction, and did not lengthen with altitude | RK-02 | **VERIFIED — fixed** | intensity now follows burned mass flow, length follows the pressure expansion, direction follows the gimbaled thrust axis, and recovering stages drive their own plume; sea level 6.6 m / 96.8 kPa vs 40.4 km 20.7 m / 274 Pa at the same flow fraction; `2026-11-07-rk02-*` |

## Validation status

| check | result | commit |
| --- | --- | --- |
| `cargo fmt --check` | PASS | this change |
| `cargo check --all-targets --features dem` | PASS | this change |
| `cargo check --no-default-features` | PASS (existing feature-disabled warnings) | this change |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS | this change |
| `cargo test --features dem` | library 1268 passed, 0 failed, 2 ignored; other targets pass | this change |
| `cargo test --no-default-features` | library 1216 passed, 0 failed, 2 ignored | this change |
| `openspec validate --all --strict` | 43 passed, 0 failed | this change |
| `cargo build --release --features dem` | PASS | this change |
| release three-mode smoke (15 s each) | PASS — all three exit 124, no ERROR/panic matches; startup only | this change |
| default `cargo run` three-mode smoke (15 s each) | PASS — all three create mode windows, exit 124, no ERROR/panic matches | this change |
| headless Falcon 9 / RTLS re-flight (5,400 s) | accepted 201.178314 × 199.975904 km, final primary mass 23,858.331 kg; recovery crash 186.154696 m/s vertical, 31.822342 m/s lateral, 8.726413° tilt, 706,186.117161 m miss, 0 kg recovery propellant; prior rounded impact figures confirmed; RCS summary lacks entity attribution | this change |
| downrange deck landing (committed 15 t, 656.25 s) | **VERIFIED for deck 25.53904, −88.40209**, repeated with identical recovery summary: 1.599417 m/s vertical, 0.744938 m/s lateral, 10.097306° tilt, 667.612061 kg left, final `Landed` | this change |
| nearby deck counterexample | deck 25.538992, −88.402547; deck crash 1.534015 m/s vertical, 2.904969 m/s lateral, 17.117436° tilt, 266.267609 kg left; placement robustness GAP | this change |
| visual capture path | works offscreen and windowed; in-app F12 frames captured per mode | `8ea9207` |
| diagnostic capture matrix | 18 terrain frames at 300 km, 18 at 1,200 km, 7 at 3–70 km; the post-fix roughness/production pair; 9 water-view frames; 2 paired pre/post crops | `903d7c8` |
| seam harness | `scripts/measure_patch_seam.py` reproduces the pre-fix seam at +4.14/255 and the post-fix seam at −0.04/255 | `903d7c8` |
| region-edge harness | `scripts/imagery_region_mask.py` + `scripts/measure_tone_edge.py` report the imagery edge at 4.23/255 before and 0.72/255 after, and the change confined to 16.4 km | 81f3c04 |
| TD-05 attribution A/B | constant-roughness and morph-off frames, same pose/clock/frames | `ec97d6e` |
| near-ground tone control | replacing the local albedo mix with the base (same normals, roughness, layers) drops the 3 km ratio from 4.36 to 1.98; the tint build measures 2.01 against 2.03 at 40 km | eba5d6e |

## Visual evidence coverage

| scene | state |
| --- | --- |
| prelaunch | current — body tripod and retained upper envelope |
| launch site | current — one graded pad per predefined site, all flush and dry |
| terrain close-up | current |
| coastline | current — Bight coastline within one overview texel |
| clouds | current — cell-size spectrum and resolved edges |
| atmosphere / limb | current — 10/30/70/200/400 km sequence and 200/400 km limb at 6° FOV |
| vegetation | current — closed crowns and a fixed-pose wind pair |
| rocket ascent | current — ascent, staging and plume at two altitudes |
| high altitude | current |
| orbit | current — orbital-altitude viewpoint at 400 km |

All ten required scenes are current; `scripts/visual_acceptance.py` reports
`10 scenes pass`. See `VISUAL_EVIDENCE.md`.

## Performance baseline

| field | value |
| --- | --- |
| Current-commit measurement | prelaunch release at `8ea9207`, vsync off: 720p 19.76 / 27.25 / 32.15 ms, 1440p 51.11 / 59.07 / 63.71 ms (p50/p95/p99) |
| Last CPU p50/p95/p99 (prelaunch, release, discrete GPU) | 16.66 / 17.63 / 19.04 ms (2026-09-11) |
| Last CPU p50/p95/p99 (ascent) | 13.53 / 25.28 / 27.36 ms (2026-09-11) |
| Worst frame | not exposed |
| GPU frame time | not exposed |
| Draw calls / triangles | not exposed |
| Memory | 1,261–1,492 MiB GPU; ~1.77 GB RSS (2026-09-11); 610–640 MB RSS (2026-08-23) |
| Terrain streaming | bounded at 128 MiB geometry budget; two worker bakes/frame (2026-09-11) |
| Stationary render-asset creation (RD-01) | 0 mesh / 0 material / 0 image, 0 duplicate requests over two 600-frame windows at the Papua chase pose (pinned clock) |
| Rocket effects (debug capture, 40 km) | 33 effect entities / 21 visible; engine effect update 0.016–0.022 ms, ground 0.0011–0.0017 ms (2026-11-07) |

The offscreen capture path is pinned to 60 Hz and is therefore **not** a
frame-pacing harness. See `PERFORMANCE_BASELINES.md`.

## OpenSpec status

| item | value |
| --- | --- |
| specs | 42 |
| active changes | 1 — `spaceflight-game-mvp` (5/37 tasks) |
| archived changes | 51 |
| strict validation | 43/43 pass |
| requirement gaps | cloud lighting and aerial perspective still live only in the technical specification; visual atmosphere beyond `atmosphere` is thin |

## Next task

**Recovery extension (2026-11-14):** one specified 15 t full-flight deck landing
is now VERIFIED after fixing slew-aware gimbal allocation and lower-extent deck
clearance. A nearby placement still crashes on tilt, so convergence robustness
remains open. The earlier deck-crash claim used a gate that could capture a
sea-floor impact and is superseded. See **Deck touchdown verification** in
`docs/scientific_correctness_audit.md` for the repeat run, counterexample,
provenance and reserve limits. An opt-in 1 Hz DEBUG trace now supports repeatable
landing analysis; temporary environment-switched diagnostics remain removed.
The older scientific-extension narrative below describes historical measurements.

**The realism programme is complete.** All 45 tasks are VERIFIED. The two former
blockers closed on 2026-11-09: `VL-01` with three clean 25 s mode runs, and
`P0-04` with release measurements at 720p and 1440p.

The measurement limits are recorded rather than rounded away: P0-04 measured
prelaunch only (no input injection for the Space launch), with vsync off because
AutoVsync quantizes to about 1 s per present on this PRIME display; worst frame,
GPU pass time, draw calls and triangle counts are still unexposed by telemetry.

Extension work is a separate scientific-correctness track, not remaining roadmap
items. It is recorded in `docs/scientific_correctness_audit.md` and now has three
more closed items on 2026-11-11: SC-04 adds a representative per-thruster RCS
cluster with allocation, failure, minimum-impulse and propellant draw, and makes
gimbals supply the primary pitch/yaw command while RCS supplies only the residual
(the old ascent path doubled pitch/yaw authority); SC-06 records why the mutable
SpaceX vehicle page cannot honestly be byte-pinned, so the Falcon 9 entry stays
`Representative` and reproducibility stays pinned by `configuration_sha256`; and
SC-03 equalises the two lateral aerodynamic slopes at 1.5 per radian so the
axisymmetric airframe is roll-invariant. The headless Falcon 9 flight circularizes
at 201.1783 x 199.9759 km with 42.483 kg of RCS propellant used.

The residuals are recorded rather than hidden: the RCS cluster is a
representative single-arm pure-couple set drawing from the final stage main tank,
no flight exercises a failed thruster, and the booster recovery was flown but does
not land. The recovery flight exposed and fixed chutes on a landing-gear vehicle,
repeated crash events and a spinning-contact energy pump. The stage now targets
the rotating pad, and four grid fins now hold it engines-first with body rate
below 0.5 rad/s through entry. It still lands 710.88 km short at 652.94 m/s
because boostback consumes the 15 t reserve; 25 t improves the miss to 549.01 km
without a landing and 40 t breaks the ascent. The center engine, one-engine
subset, and retrograde aero fold are now committed: the stage holds engines-first
(α ≈ 179.7°, body rate ≈ 0) through the high-dynamic-pressure rise, and a
downrange entry still reaches 2.2 km/s at 15 km. Entry energy and flight-path
shaping, not attitude authority, is the remaining blocker. A sea-floor water-impact model also
remains. Recovery stays IMPLEMENTED-tested. The insertion law
trims apsides only, but the measured plane error is 0.00135°, so no plane-change
burn is justified by the current flight; the remaining insertion offset is the
altitude deadband. The `enu_basis` residual remains, and SC-06 needs a dated
immutable source before it can be promoted.
