# Project status

The entry point for "what is going on here". Open this first.

* Planning and task detail: [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md)
* Counts and current state: [`DASHBOARD.md`](DASHBOARD.md)
* Frames and what they prove: [`VISUAL_EVIDENCE.md`](VISUAL_EVIDENCE.md)
* Timings: [`PERFORMANCE_BASELINES.md`](PERFORMANCE_BASELINES.md)
* Task → requirement mapping: [`OPENSPEC_MAP.md`](OPENSPEC_MAP.md)

Snapshot: 2026-11-01. Last code checkpoint: `f3b1af6`.

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
TD-05 (tone blocks), TD-04 (regional imagery edge) and AT-03 (high-altitude land
darkness, attributed) are closed; the next measurable visual defect is the
near-ground albedo tone step, scoped as TM-04.

## What is broken?

| defect | state |
| --- | --- |
| Rectangular tone blocks at 300–1,200 km | **verified and closed.** Per-vertex roughness removed the land term: the seam that motivated the task measures −0.04/255 against its own trend (was +4.14), frame-wide median +0.25/255 at 300 km and +0.47/255 at 1,200 km. The ocean half was refuted with six new water views |
| Synthetic detail fabricates lakes on the coastal plain | guard implemented, activation blocked |
| Land nearly black at 400 km while clouds are white | **verified — explained.** The altitude albedo is identical at 400 km and 3 km; the deck is white because it is white and the forest albedo is ~4 %. The remaining step is near the ground: local surfaces and the layered material are 2–2.5× brighter than the measured overview (TM-04) |
| Cloud edges are cut-outs; one shape per region | planned work |
| Vegetation reads as specks | planned work |
| Regional imagery steps at its patch edge | **verified and closed.** The edge is now a 16.6 km cross-fade at the region bound; the worst step near the bound fell from 4.23 to 0.72/255 (the frame's own texture level) and the change is confined to the declared band |
| No geometric terrain detail beyond LOD 11 | planned work |
| **Windowed rendering fails on this machine** | environment: swapchain lost after one present |

The last row is why the three-mode smoke test cannot be re-run here. It is not a
code defect and it is not hidden: the offscreen path exists precisely because of
it.

## What has been verified?

Fourteen of forty-one roadmap tasks, each with implementation + tests + a named frame
or a named measurement:

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
| AT-03 high-altitude darkness | the same ground measures the same albedo at 400 km and 3 km (0.0169 linear p50); the near-ground material is the 2–2.5× outlier |
| VL-02 OpenSpec strict | 42/42 |

Four more are implemented but **not** verified: site grading at more than one
site, imagery colour match (no before/after frame), the sky model (no altitude
sequence), and streaming measurement (no split timings).

## What is blocked?

| task | reason |
| --- | --- |
| P0-04 performance baseline | no presentable window |
| TD-06 fabricated coastline | enabling the guard moves the committed LEO baseline, cause not isolated |
| VL-01 three-mode smoke | same presentation failure |

## What should be built next?

**TD-05 — closed on a paired measurement.** Six water views false-colour the
sea's depth, body colour, alpha, landscape shadow, cloud shadow and mirror; all
are continuous, so the ocean half of the defect never existed. The land term was
per-vertex roughness, and `scripts/measure_patch_seam.py` — which fits each side of
a seam so a real slope cannot pass for a boundary — measures the seam that
motivated the task at +4.14/255 before and −0.04/255 after, with frame-wide medians
of +0.25/255 at 300 km and +0.47/255 at 1,200 km, inside the ±1/255 acceptance.

After that: TD-04 imagery feather, then AT-02 altitude and limb (which also fills
two of the five missing evidence scenes).

## How do we know it works?

```bash
cargo fmt --check
cargo check --all-targets --features dem
cargo clippy --all-targets --features dem -- -D warnings
cargo test --features dem --lib          # 1119 passed, 2 ignored
cargo test --no-default-features --lib   # 1077 passed, 2 ignored
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

The simulator runs and its physics is regression-locked; 1,133 domain and adapter
tests pass (1,091 without the `dem` feature) and OpenSpec is strict-clean. The
realism programme has verified fourteen of its forty-one tasks.

The last three slices each closed a defect that screenshots alone could not
explain, by building the instrument the question needed first. The sea surface
ramping into the shore was geometry; the launch pad floating on a slope was site
grading. The rectangular tone blocks at altitude were a per-patch roughness
constant on land and, in the water, nothing at all — the block read was the shelf
break. The regional imagery edge was not where anyone thought: the tile only
started at the next patch boundary, because tile resolution tested patch-centre
containment.

What is left of that family is a tone step rather than a defect: the ground
brighter near the camera than from orbit, because the near-ground material's
albedo is 2–2.5× the measured overview's. That is TM-04, and the method is
already proven — a measured gain, recorded with its derivation, the way the
regional mosaic was matched.

Two hard constraints are recorded rather than worked around. The machine cannot
present a Vulkan surface, so windowed validation is blocked and the offscreen
capture path exists instead. And a terrain guard that would stop synthetic relief
from fabricating lakes is implemented but switched off, because enabling it moves
a committed physics baseline for a reason that is not yet understood — and a
baseline shift nobody can explain is not a fix.

Of the ten required visual scenes, five have usable evidence, two are partial, and
three are missing. That is the truest single measure of where this stands.
