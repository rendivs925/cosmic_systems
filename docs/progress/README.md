# Project status

The entry point for "what is going on here". Open this first.

* Planning and task detail: [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md)
* Counts and current state: [`DASHBOARD.md`](DASHBOARD.md)
* Frames and what they prove: [`VISUAL_EVIDENCE.md`](VISUAL_EVIDENCE.md)
* Timings: [`PERFORMANCE_BASELINES.md`](PERFORMANCE_BASELINES.md)
* Task → requirement mapping: [`OPENSPEC_MAP.md`](OPENSPEC_MAP.md)

Snapshot: 2026-11-05. Last code checkpoint: `458d0b1`.

---

## What has been built?

A single Rust + Bevy simulator with three modes (`solar`, `craft`, `rocket`) over
one shared domain: SPICE/DE440 ephemerides, WGS-84 reference frames, a cube-sphere
`TerrainSource` with measured ETOPO 2022 + regional payloads, fixed-step 6-DOF
flight, streamed terrain with LOD, volumetric clouds, a single-scattering sky, and
deterministic regression baselines (ascent, LEO, staging recovery).

On the realism programme specifically: ocean surface on the sea-level datum,
launch sites graded level in the terrain authority, regional imagery colour
matched to the global overview, cloud coverage/sun-path/aerial-perspective work,
and a measurement path that captures frames without a swapchain.

## What is currently being built?

**Nothing is mid-edit.** The last checkpoint is committed, pushed and green.
TD-02 is verified: every captured pad is flush and dry. Getting there needed
three things. A capture-only `COSMIC_SYSTEMS_CAPTURE_UTC` pins the start instant,
because the shared default epoch is local noon only at Papua. Resident regional
payloads now override the erosion refinement, and every site payload is preloaded
before the grade anchors are sampled, so a pad no longer lands on the coarse
global fallback (Vandenberg was 81 m out). And a 3 m platform floor keeps a pad
above the local sea datum, with Kourou moved from the town to the ELA-3 pad.
TD-10 is now closed too: the erosion rasters are simulated from the immutable
measured global DEM, not from the streamed payload overlay, so cache-on,
cache-off and cold runs agree to repeat-run noise. And TD-03 now has the
before/after frame it was missing: a capture switch turns the regional colour
match off, and at the same 40 km pose the region/overview tone ratio falls from
3.87× to 1.16×.

## What is broken?

| defect | state |
| --- | --- |
| Rectangular tone blocks at 300–1,200 km | **verified and closed.** Per-vertex roughness removed the land term: the seam that motivated the task measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km. The ocean half was refuted with six new water views |
| Synthetic detail fabricates lakes on the coastal plain | guard implemented, activation blocked |
| Three launch pads sat under water: Starbase, KSC and Wallops were −0.78, −0.14 and −13.61 m against the local sea datum | **verified and closed.** Resident payloads replace the erosion refinement, the platform is floored 3 m above the sea datum, and all seven captured pads are dry |
| Terrain outside the resident payload tile depended on the erosion cache: rasters were baked from the streamed payload overlay, so load order and an old disk tile changed height, moisture and flow | **verified and closed.** Erosion now bakes from the immutable measured global DEM (`5134ed2`); Starbase cold/warm/off agree to 2.4–2.5/255 mean and Baikonur to 3.0–3.2/255, the same as two identical warm runs. A cold or cache-off run needs ~1,200 frames to finish streaming |
| Land nearly black at 400 km while clouds are white | **verified — explained, and the near-ground half fixed.** The altitude albedo is identical at 400 km and 3 km; the deck is white because it is white and the forest albedo is ~4 %. The near ground was 2.2× brighter because the local surface map carried absolute albedo 4× the measured overview; it is now a tint anchored to the overview, so a descent moves the tone +2 % instead of +128 % (TM-04) |
| Cloud edges are cut-outs; one shape per region | planned work |
| Vegetation reads as specks | planned work |
| Regional imagery steps at its patch edge | **verified and closed.** The edge is now a 16.6 km cross-fade at the region bound; the worst step near the bound fell from 4.23 to 0.72/255 (the frame's own texture level) and the change is confined to the declared band |
| No geometric terrain detail beyond LOD 11 | planned work |
| **Windowed rendering fails on this machine** | environment: swapchain lost after one present |

The last row is why the three-mode smoke test cannot be re-run here. It is not a
code defect and it is not hidden: the offscreen path exists precisely because of
it.

## What has been verified?

Twenty-one of forty-four roadmap tasks, each with implementation + tests + a
named frame or a named measurement:

| task | evidence |
| --- | --- |
| P0-01 deterministic capture view | every frame in the registry |
| P0-02 offscreen capture | produced 9 frames with no display |
| P0-03 diagnostic render modes | 18 false-colour views at one pose; uniform-layout parity test |
| TD-01 ocean on the datum | pale shoreline pixels 1.58 % → 0.40 %; tests |
| LF-01 launch-site footprint | `pad-250m` (disc) vs `pad-110m` (clean) |
| CL-01 coverage as sky share | coverage invariant; `1200km-90f` |
| CL-02 layer-scaled cells | `1200km-90f` |
| CL-03 sun path to slab top | 11 % of deck pixels moved 1–5 %; parity test |
| CL-04 aerial perspective | parity test; limb behaviour |
| RD-03 shadow lifecycle | 90-pose sweep pixel-identical; residency test |
| TD-05 tone blocks | seam excess +4.14 → −0.04/255 at the seam that motivated it; frame-wide +0.25/+0.47/255; ocean half refuted with six water views |
| TD-04 imagery edge | the inset edge is a 16.6 km cross-fade at the region bound; worst step near the bound 4.23 → 0.72/255, change confined to x=381..737 |
| AT-03 high-altitude darkness | the same ground measures the same albedo at 400 km and 3 km (0.0169 linear p50); the near-ground material was the 2.2× outlier, closed by TM-04 |
| TM-04 near-ground tone | the local albedo map was 4× the overview; a control build isolates it (4.36 → 1.98) and the tint build measures 2.01 at 3 km against 2.03 at 40 km |
| AT-01 single-scattering sky | one `SkyParams` authority; the 10/30/70/200/400 km sequence and the 200/400 km limb exercise it from sea level to vacuum |
| AT-02 altitude transition and limb | 6° limb e-folding 8.30 km (200 km) and 8.54 km (400 km) in linear light over 24–32 km, within 7 % of 8 km; displayable top 35–38 km; horizon luminance monotonic 0.138 → 0.073 → 0.026 → floor |
| TD-02 site grading | one capture per predefined site, all flush and dry (`2026-11-05-td02-*-fixed`); pad anchors now come from the resident payload |
| TD-10 cache-independent terrain | erosion baked from the immutable global DEM; Starbase cold/warm/off 2.53/2.37/2.40/255 mean, Baikonur 2.96/3.16/3.06/255, all within repeat-run noise; `2026-11-05-td10-*` |
| TD-03 imagery colour match | region/overview linear-luma ratio 3.87× → 1.16×; median before/after ratio 4.264/2.470/4.705 vs inverse gain 4.269/2.464/4.705 (0.2 %); outside unchanged; `2026-11-05-td03-albedo-40km-*` |
| TD-11 coastal platform floor | Starbase +0.10 → +3.00 m, Papua +0.54 → +3.00 m, KSC reference +0.94 → +3.00 m; Kourou moved to ELA-3 (+9.19 m) |
| VL-02 OpenSpec strict | 42/42 |

One more is implemented but **not** verified: streaming measurement (no split
timings).

## What is blocked or needs a decision?

| task | reason |
| --- | --- |
| P0-04 performance baseline | no presentable window |
| TD-06 fabricated coastline | enabling the guard used to move the committed LEO baseline; the fixture is now decoupled (`c886205`), so the guard can be re-evaluated |
| VL-01 three-mode smoke | same presentation failure |

## What should be built next?

**RD-02 — the streaming split timings.** It is implemented but still has no
measured breakdown. After that, AT-04 (multiple-scattering horizon hue, opened by
AT-02's measurement rather than hidden), then the TD-06 guard re-evaluation.

The practical caveat from TD-10 is convergence time: a cold or cache-off erosion
run needs about 1,200 frames at this pose, because the runtime bake is expensive
and the 64-tile in-memory cache thrashes; the warm store path settles by 240. The
global startup bake that touches thousands of 2 ° tiles belongs to the P0-04
performance work, not to terrain authority.

## How do we know it works?

```bash
cargo fmt --check
cargo check --all-targets --features dem
cargo clippy --all-targets --features dem -- -D warnings
cargo test --features dem --lib          # 1147 passed, 2 ignored
cargo test --no-default-features --lib   # 1105 passed, 2 ignored
openspec validate --all --strict         # 42 passed
```

And for anything visual (or, with `COSMIC_SYSTEMS_DIAGNOSTIC_MODE=<mode>`, for one
terrain input on its own — see `docs/measurement/terrain_diagnostics.md`):

```bash
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE=/tmp/frame.png \
COSMIC_SYSTEMS_OFFSCREEN_CAPTURE_FRAMES=200 \
COSMIC_SYSTEMS_CAPTURE_POSE="-8.0,139.5,1200000" \
COSMIC_SYSTEMS_CAPTURE_TIME=42 \
  ./target/debug/cosmic_systems rocket --site papua
```

A visual claim without a frame in `VISUAL_EVIDENCE.md` at a named commit is not a
verified claim. A performance claim without a row in `PERFORMANCE_BASELINES.md` is
not a measured claim.

---

## Concise status report

The simulator runs and its physics is regression-locked; 1,147 domain and adapter
tests pass (1,105 without the `dem` feature) and OpenSpec is strict-clean. The
realism programme has verified twenty-one of its forty-four tasks.

The last three slices each closed a defect that screenshots alone could not
explain, by building the instrument the question needed first. The sea surface
ramping into the shore was geometry; the launch pad floating on a slope was site
grading. The rectangular tone blocks at altitude were a per-patch roughness
constant on land and, in the water, nothing at all — the block read was the shelf
break. The regional imagery edge was not where anyone thought: the tile only
started at the next patch boundary, because tile resolution tested patch-centre
containment.

The last of that family was a tone step rather than a defect: the ground read
brighter near the camera than from orbit because the local surface map carried an
absolute albedo 4× the measured overview's. A control build that replaced only
that term put the near frame back at the orbital level (4.36 → 1.98), and the
map is now a tint anchored to the overview, so a descent changes the tone by
+2 % instead of +128 %.

Two hard constraints are recorded rather than worked around. The machine cannot
present a Vulkan surface, so windowed validation is blocked and the offscreen
capture path exists instead. And a terrain guard that would stop synthetic relief
from fabricating lakes is implemented but switched off, because enabling it moves
a committed physics baseline for a reason that is not yet understood — and a
baseline shift nobody can explain is not a fix.

Site grading added a third such seam, and this one is now measured rather than
suspected. At the epoch the shared clock uses, only Papua is in daylight, so the
capture path gained a start-instant override and every site was framed at the
same solar hour angle. Four pads sit flush. Starbase, KSC and Wallops do not:
the composed ground under them is below the local sea surface, and the tower
stands in water. Probing the composition separates two causes. The erosion field
is baked while only the site's finest payload tile is resident, so most of the
tile is built from the coarse global fallback — Vandenberg's pad samples 81 m
above the site probe its own package publishes. And Starbase's measured payload
is only 0.2 m above the sea surface, so a sub-metre erosion term drowns it.
Worse, the disk cache freezes one version of that field: a warm and a cold cache
change 90 % of the frame. Both were fixed for the pad: a resident regional
payload now outranks the erosion refinement, every site's payload tile is loaded
before the grade anchor is sampled, and a 3 m platform floor plus the Kourou
coordinate correction put all seven captured pads on dry, flush ground. The
baselines did not move because the ascent fixture now pins its launch altitude
and pad (`c886205`, `17bc6f5`).

The last piece was subtler than the pad itself. Even with the pad anchored to the
payload, the erosion rasters were baked from the *streamed* surface, so the
surrounding height, moisture and rivers depended on which payload tiles had
loaded first and whether an old raster came from disk. The rasters now bake from
the immutable measured global DEM, which is what the offline baker already used;
a resident payload stays a finer measurement on top. Cache-on, cache-off and cold
frames then agree to the same 2–3/255 that two identical warm runs already did.

The imagery colour match had the opposite problem: it was implemented and
numerically tested, but never demonstrated against the artefact it exists to
remove. A capture-only switch now installs the regional tiles without the gain.
At 40 km the Sentinel-2 mosaic measured 3.87× the overview in linear luminance;
with the gain it measures 1.16×, and the median per-pixel before/after ratio
matches the declared inverse gain in all three channels to within 0.2 %. Outside
the region the frame is untouched. The residual boundary at 80 km is a package
LOD-coverage limit — patches coarser than `min_level: 8` cannot bind the mosaic —
and is recorded rather than smoothed over.

Of the ten required visual scenes, five have usable evidence, two are partial,
and three are missing. That is the truest single measure of where this stands.
