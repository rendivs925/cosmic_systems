# Project status

The entry point for "what is going on here". Open this first.

* Planning and task detail: [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md)
* Counts and current state: [`DASHBOARD.md`](DASHBOARD.md)
* Frames and what they prove: [`VISUAL_EVIDENCE.md`](VISUAL_EVIDENCE.md)
* Timings: [`PERFORMANCE_BASELINES.md`](PERFORMANCE_BASELINES.md)
* Task → requirement mapping: [`OPENSPEC_MAP.md`](OPENSPEC_MAP.md)

Snapshot: 2026-11-07. Last code checkpoint: `6209a73`.

---

## What has been built?

A single Rust + Bevy simulator with three modes (`solar`, `craft`, `rocket`) over
one shared domain: SPICE/DE440 ephemerides, WGS-84 reference frames, a cube-sphere
`TerrainSource` with measured ETOPO 2022 + regional payloads, fixed-step 6-DOF
flight, streamed terrain with LOD, volumetric clouds, a physically based sky with a bounded diffuse fill, and
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
| Noon horizon reddened to tan/pink and the limb tail read teal | **verified and fixed by a bounded diffuse fill** (AT-04); the exact-horizon cloud-deck band remains a cloud task |
| Vegetation reads as specks | planned work |
| Regional imagery steps at its patch edge | **verified and closed.** The edge is now a 16.6 km cross-fade at the region bound; the worst step near the bound fell from 4.23 to 0.72/255 (the frame's own texture level) and the change is confined to the declared band |
| No geometric terrain detail beyond LOD 11 | planned work |
| **Windowed rendering fails on this machine** | environment: swapchain lost after one present |

The last row is why the three-mode smoke test cannot be re-run here. It is not a
code defect and it is not hidden: the offscreen path exists precisely because of
it.

## What has been verified?

Twenty-six of forty-five roadmap tasks, each with implementation + tests + a
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
| RD-02 streaming split | culling, LOD reconcile and upload burst timed separately; prelaunch p95 0.257 / 0.865 ms per frame, 40 km 0.171 / 0.597 ms; 347–352 meshes at 0.016–0.019 ms build and 0.188–0.192 ms submission each; `PERFORMANCE_BASELINES.md` |
| AT-04 multiple-scattering horizon | +2° saturation 0.252/0.467/0.655 → 0.204/0.305/0.344 at 10/30/70 km; 70 km teal band 0.670 → 0.339; limb e-folding 8.30/8.33 km (before 8.24/8.45); `2026-11-05-at04-*` |
| TD-11 coastal platform floor | Starbase +0.10 → +3.00 m, Papua +0.54 → +3.00 m, KSC reference +0.94 → +3.00 m; Kourou moved to ELA-3 (+9.19 m) |
| TD-06 synthetic coastline guard | land-only sea-datum guard enabled; worst-case detail cannot cross the datum across the whole window, measured ocean keeps signed relief, all three committed baselines unchanged; Papua A/B mean 1.53/255; `2026-11-05-td06-papua-*` |
| RK-01 attitude and staging | separation preserves attitude/angular velocity; jettisoned debris now advances its own render snapshot; the serial-stage mesh is centred on the physical stage; the debug tripod uses the flight frame; three body-axis frames, staging at `T+27.5 s`, capture at `T+29.9 s`; `2026-11-06-rk01-*` |
| RK-02 plume state | intensity is the burned mass-flow fraction, length follows the pressure expansion, direction follows the gimbaled thrust axis; same flow fraction at sea level 6.6 m / 96.8 kPa and 40.4 km 20.7 m / 274 Pa; 33 effects, 0.022 ms update; `2026-11-07-rk02-*` |
| VL-02 OpenSpec strict | 43/43 |

Every implemented task is now verified. The two remaining blocked items are
environmental; the rest are planned work.

## What is blocked or needs a decision?

| task | reason |
| --- | --- |
| P0-04 performance baseline | no presentable window |
| VL-01 three-mode smoke | same presentation failure |

## What should be built next?

**TD-12 — the Papua water bodies.** Determine whether the angular water bodies
that persist after the land-only detail guard are carved by erosion or measured,
by comparing against ETOPO before changing the erosion model.

RK-02 is closed. The plume was already partly state-driven, but intensity used a
binary running flag, length ignored the pressure expansion, and direction ignored
the gimbal. It now reads the mass flow the fixed pipeline actually burned,
lengthens with the same expansion that widens it, and points along the
authoritative gimbaled thrust axis — including for a recovering first stage, which
previously had no applied-propulsion sample. The remaining terrain and material
audits (TD-07, TD-08, TD-09, TD-12, TM-01/02/03, TG-01/02) and RD-01 follow.

## How do we know it works?

```bash
cargo fmt --check
cargo check --all-targets --features dem
cargo clippy --all-targets --features dem -- -D warnings
cargo test --features dem --lib          # 1165 passed, 2 ignored
cargo test --no-default-features --lib   # 1123 passed, 2 ignored
openspec validate --all --strict         # 43 passed
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

The simulator runs and its physics is regression-locked; 1,165 domain and adapter
tests pass (1,123 without the `dem` feature) and OpenSpec is strict-clean. The
realism programme has verified twenty-six of its forty-five tasks.

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
capture path exists instead. The other was a terrain guard that would stop
synthetic relief from fabricating lakes. It is now enabled, but only after the
baseline shift was isolated rather than accepted: the guard had been suppressing
negative detail across the entire ocean floor, and the orbital fixture samples
that floor. Making it a land rule restored the committed bathymetry
bit-for-bit.

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

The atmosphere's last defect was a missing physical term rather than a shading
error: a single-scattering sky cannot keep the horizon pale when the path is
long, because the light that scattered more than once is absent. A full
multiple-scattering LUT was out of proportion to the task, so the sky now applies
a bounded, luminance-preserving diffuse fill that grows with altitude. At 10/30/70
km the +2° saturation falls from 0.252/0.467/0.655 to 0.204/0.305/0.344, and the
70 km teal tail from 0.670 to 0.339, while the linear-light limb e-folding stays
8.30/8.33 km. It is explicitly an approximation of the diffuse fill's
desaturating effect, not a second-order radiative-transfer solve, and the
exact-horizon cloud-deck band is left to the cloud work.

The coastal-lake guard had the same shape of problem as the sky: the mechanism
was right, but its scope was wrong. Suppressing negative synthetic detail near
the sea datum is what stops a ±36 m octave from turning a plain that is 2–14 m
above the sea into angular lakes. Applying it everywhere below the datum also
rewrote the ocean floor, which changed the terrain normal the orbital fixture
samples and moved a committed baseline. The guard is now a land rule with a
continuous fade below the datum, bounded from the octave's own amplitude so the
worst case cannot cross the datum anywhere. All three baselines pass unchanged.
The large water bodies that remain in the Papua frame are therefore not
detail-fabricated; they are below the datum in the base surface and are tracked
separately as TD-12 instead of being claimed as fixed.

The performance split had a subtler gap: the phase timers existed, but the
rolling window was shorter than the report cadence, so the startup upload burst
was always gone before the first full report. Widening the window to ten seconds
retained it. At the current machine the terrain LOD reconcile costs about
0.6–0.9 ms per reconcile frame and culling about 0.17–0.26 ms; upload is a
one-time 347–352 meshes at roughly 0.02 ms of mesh construction and 0.19 ms of
CPU submission each. Those are CPU costs; GPU upload completion is still not
exposed.

Of the ten required visual scenes, five have usable evidence, two are partial,
and three are missing. That is the truest single measure of where this stands.

The rocket audit had the same shape one level down: the physics was right and the
picture was wrong. A separated stage kept its own authoritative state but the
render snapshot system required a mission state, so debris stopped updating at
the separation tick and hung in the sky. The rebuilt upper-stage mesh was centred
on the stage plus the fairing envelope while physics had already re-origined to
the stage itself, so it sat half an envelope low. And the debug tripod, the tool
needed to show any of this, converted metres with the solar-map factor and drew
a twenty-metre axis a hundred-thousandth of a unit long at the planet centre.
All three are fixed, pinned by tests, and demonstrated with body-axis frames at
prelaunch, ascent and the first staging tick.

The rocket plume had the same shape as the attitude audit: the state was there,
the picture only used part of it. Intensity treated every engine as fully on or
fully off, length responded to the burn but not to the altitude that widens the
exhaust, and every plume pointed straight down the body even when the engine was
gimbaled. It now reads the propellant mass flow the fixed pipeline actually
burned, lengthens with the pressure-derived expansion, and follows the
authoritative gimbaled thrust axis. The same change gave recovering first stages
their own applied-propulsion sample, so a landing burn is no longer drawn from a
commanded-throttle guess.
