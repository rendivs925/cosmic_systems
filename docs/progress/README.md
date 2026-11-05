# Project status

The entry point for "what is going on here". Open this first.

* Planning and task detail: [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md)
* Counts and current state: [`DASHBOARD.md`](DASHBOARD.md)
* Frames and what they prove: [`VISUAL_EVIDENCE.md`](VISUAL_EVIDENCE.md)
* Timings: [`PERFORMANCE_BASELINES.md`](PERFORMANCE_BASELINES.md)
* Task → requirement mapping: [`OPENSPEC_MAP.md`](OPENSPEC_MAP.md)

Snapshot: 2026-11-05. Last code checkpoint: `c582708`.

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
TD-02's capture set is complete and it found a real defect. A capture-only
`COSMIC_SYSTEMS_CAPTURE_UTC` now pins the start instant, because the shared
default epoch is local noon only at Papua; with it, every predefined site can be
measured in daylight. Four pads are flush (Baikonur, Vandenberg, Kourou, Mahia);
three stand in open water (Starbase, KSC, Wallops). The probe says the composed
ground at those three is below the local sea datum, and that the pad anchor is
decoupled from its regional payload by the erosion field (up to 81 m at
Vandenberg). That is TD-10, the next task.

## What is broken?

| defect | state |
| --- | --- |
| Rectangular tone blocks at 300–1,200 km | **verified and closed.** Per-vertex roughness removed the land term: the seam that motivated the task measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km. The ocean half was refuted with six new water views |
| Synthetic detail fabricates lakes on the coastal plain | guard implemented, activation blocked |
| Three launch pads sit under water: Starbase, KSC and Wallops are −0.78, −0.14 and −13.61 m against the local sea datum | **diagnosed**; needs TD-10 and TD-11 |
| The pad anchor and the visible terrain depend on the erosion cache: Vandenberg samples 81 m above its published package probe, and a warm vs cold cache changes 90 % of the frame | **diagnosed**; TD-10 |
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

Seventeen of forty-four roadmap tasks, each with implementation + tests + a named
frame or a named measurement:

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
| VL-02 OpenSpec strict | 42/42 |

Two more are implemented but **not** verified: imagery colour match (no
before/after frame) and streaming measurement (no split timings). Site grading
across the predefined sites is now captured but **blocked** on the two terrain
defects above.

## What is blocked or needs a decision?

| task | reason |
| --- | --- |
| P0-04 performance baseline | no presentable window |
| TD-02 site grading | three pads are below the sea datum (TD-10, TD-11) |
| TD-06 fabricated coastline | enabling the guard moves the committed LEO baseline, cause not isolated |
| TD-11 coastal datum floor | the baselines launch from `ksc-default-geodetic`, which is itself 0.94 m under the sea datum; moving it needs a baselines decision |
| VL-01 three-mode smoke | same presentation failure |

## What should be built next?

**TD-02 — one capture per predefined site.** The grading itself is implemented
and tested, and Papua has a clean frame; Starbase, Baikonur, Vandenberg, Wallops,
Kourou, Mahia and KSC do not. That is the remaining evidence for a P0 task.

After that: TD-03 (imagery colour-match A/B), RD-02 (streaming split timings),
then AT-04 (multiple-scattering horizon hue, opened by AT-02's measurement rather
than hidden).

## How do we know it works?

```bash
cargo fmt --check
cargo check --all-targets --features dem
cargo clippy --all-targets --features dem -- -D warnings
cargo test --features dem --lib          # 1139 passed, 2 ignored
cargo test --no-default-features --lib   # 1097 passed, 2 ignored
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

The simulator runs and its physics is regression-locked; 1,142 domain and adapter
tests pass (1,100 without the `dem` feature) and OpenSpec is strict-clean. The
realism programme has verified seventeen of its forty-four tasks.

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
change 90 % of the frame. Neither is a site-grading bug; both must be fixed
before the pads can be called verified.

Of the ten required visual scenes, five have usable evidence, two are partial,
and three are missing. That is the truest single measure of where this stands.
