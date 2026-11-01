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
P0-03 (developer diagnostic render modes) is complete and verified; the next task
is TD-05, which now has the tool it was waiting for.

## What is broken?

| defect | state |
| --- | --- |
| Rectangular tone blocks at 300–1,200 km | at 1,200 km the base albedo is clean; the block is a rectangular region of the height field at the datum |
| Synthetic detail fabricates lakes on the coastal plain | guard implemented, activation blocked |
| Land nearly black at 400 km while clouds are white | cause unknown |
| Cloud edges are cut-outs; one shape per region | planned work |
| Vegetation reads as specks | planned work |
| Regional imagery steps at its patch edge | tone matched, edge not feathered |
| No geometric terrain detail beyond LOD 11 | planned work |
| **Windowed rendering fails on this machine** | environment: swapchain lost after one present |

The last row is why the three-mode smoke test cannot be re-run here. It is not a
code defect and it is not hidden: the offscreen path exists precisely because of
it.

## What has been verified?

Eleven of forty roadmap tasks, each with implementation + tests + a named frame
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

**TD-05 — the rectangular tone blocks.** The diagnostics have already done their
job: at 1,200 km the geographic albedo carries no block, while the height view
shows a rectangular region sitting within ±2 m of the sea datum against a shelf
tens of metres below it, with straight image-axis edges that cut across a patch.
The next step is a transect of the terrain authority across that edge, then
locating the tile that supplies it.

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

The simulator runs and its physics is regression-locked; 1,101 domain and adapter
tests pass and OpenSpec is strict-clean. The realism programme has verified ten of
its forty tasks. Two of the last three slices fixed real defects that the
screenshots alone could not have explained — the sea surface ramping into the
shore, and the launch pad floating on a slope — and both were confirmed by
measurement rather than by eye.

The largest visible defect, rectangular albedo blocks over the planet, is now
narrowed to albedo or geometry: it survives a seven-hour Sun change, 90 vs 400
frames, and appears at two altitudes. It is *not* the regional imagery. Isolating
it needs the diagnostic modes, which is the next task.

Two hard constraints are recorded rather than worked around. The machine cannot
present a Vulkan surface, so windowed validation is blocked and the offscreen
capture path exists instead. And a terrain guard that would stop synthetic relief
from fabricating lakes is implemented but switched off, because enabling it moves
a committed physics baseline for a reason that is not yet understood — and a
baseline shift nobody can explain is not a fix.

Of the ten required visual scenes, five have usable evidence, two are partial, and
three are missing. That is the truest single measure of where this stands.
