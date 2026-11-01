# Progress dashboard

Snapshot date: 2026-11-01.
Last code checkpoint: `f3b1af6` (2026-11-01).
This table describes the 40 tasks in `MASTER_ROADMAP.md` — not the whole
repository and not a percentage of "the simulator being finished". Many tasks are
small slices; a low percentage here means "little of *this realism programme* is
evidenced", not "the simulator does not work".

## Task counts

| state | count | share of roadmap |
| --- | ---: | ---: |
| VERIFIED | 11 | 27.5 % |
| IMPLEMENTED (not verified) | 4 | 10 % |
| IN_PROGRESS | 1 | 2.5 % |
| BLOCKED | 3 | 7.5 % |
| PLANNED | 21 | 52.5 % |
| **total** | **40** | 100 % |

## Phase completion

| phase | verified / total | share | state | blocker |
| --- | ---: | ---: | --- | --- |
| P0 Measurement foundations | 3 / 4 | 75 % | active | P0-04 blocked by presentation |
| P1 Data and terrain correctness | 1 / 9 | 11 % | active | TD-05 attribution needs P0-03; TD-06 blocked on baseline isolation |
| P2 Launch facility | 1 / 4 | 25 % | active | facility hardware and trench not audited |
| P3 Terrain materials | 0 / 3 | 0 % | planned | depends on P0-03 |
| P4 Vegetation | 0 / 3 | 0 % | planned | audit not started |
| P5 Clouds and wind | 4 / 6 | 67 % | active | shape diversity and edge structure remain |
| P6 Atmosphere | 0 / 3 | 0 % | planned | needs altitude and limb captures |
| P7 Rocket and VFX | 0 / 2 | 0 % | planned | attitude audit not started |
| P8 Rendering optimisation | 1 / 3 | 33 % | active | performance baseline blocked |
| P9 Final validation | 1 / 3 | 33 % | active | three-mode smoke blocked by presentation |

## Critical unresolved defects

Ordered by visible impact, not by ease.

| # | defect | task | state | evidence |
| --- | --- | --- | --- | --- |
| 1 | Rectangular tone blocks at 300–1,200 km | TD-05 | IN_PROGRESS | land component attributed to per-patch constant roughness (A/B: patch step 12.2 → 4.1/255); ocean component not diagnosable until a water-side view exists |
| 2 | Synthetic detail fabricates lakes on the coastal plain | TD-06 | BLOCKED | visible in `terrain-600m`; guard implemented but inert |
| 3 | Land reads nearly black at 400 km while the deck is white | AT-03 | PLANNED | `300km-papua` |
| 4 | Cloud edges are salt-and-pepper cut-outs; one shape per regime | CL-05/06 | PLANNED | `1200km-90f` |
| 5 | Vegetation is sparse and reads as specks | VG-01/02 | PLANNED | `terrain-600m` |
| 6 | Regional imagery inset still steps at its patch edge | TD-04 | PLANNED | tone matched, boundary not feathered |
| 7 | Distant terrain has no geometric detail beyond LOD 11 | TM-02 | PLANNED | `300km-papua` |
| 8 | Windowed presentation fails on this machine | env | external | one present, then swapchain lost |

## Validation status

| check | result | commit |
| --- | --- | --- |
| `cargo fmt --check` | PASS | `3155717` |
| `cargo check --all-targets --features dem` | PASS | `f3b1af6` |
| `cargo check --no-default-features` | PASS | `f3b1af6` |
| `cargo clippy --all-targets --features dem -- -D warnings` | PASS | `3155717` |
| `cargo test --features dem --lib` | 1119 passed, 0 failed, 2 ignored | `3155717` |
| `cargo test --no-default-features --lib` | 1077 passed, 0 failed, 2 ignored | `3155717` |
| `openspec validate --all --strict` | 42 passed, 0 failed | `f3b1af6` |
| `cargo build --release --features dem` | PASS | `f3b1af6` — the only later change is test-only |
| three-mode smoke (25 s each) | PASS at the time; **blocked now** | `fb686ba` |
| visual capture path | works offscreen; windowed blocked | `d89e7d6` |
| diagnostic capture matrix | 18 frames at 300 km (production plus 17 modes), 18 at 1,200 km (production plus 17 modes), 7 at 3–70 km | `<this checkpoint>` |
| TD-05 attribution A/B | constant-roughness and morph-off frames, same pose/clock/frames | `<this checkpoint>` |

## Visual evidence coverage

| scene | state |
| --- | --- |
| prelaunch | partial (no HUD) |
| launch site | current |
| terrain close-up | current |
| coastline | **missing** |
| clouds | current |
| atmosphere / limb | **missing** |
| vegetation | partial |
| rocket ascent | **missing** |
| high altitude | current |
| orbit | **missing** |

Five of ten required scenes have no usable frame. See `VISUAL_EVIDENCE.md`.

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

**TD-05 — replace per-patch constant roughness with a continuous field.** The
transect and A/B work is done: the land tone blocks come from `surface.roughness`
being one scalar per patch sampled at the patch centre, and forcing it constant
removes two thirds of a measured 12/255 step. The remaining work is the fix
(per-texel roughness baked where the patch surface is prepared), plus a
water-side diagnostic before the ocean blocks can be attributed.
