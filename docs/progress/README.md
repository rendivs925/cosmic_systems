# Project status

The entry point for "what is going on here". Open this first.

* Planning and task detail: [`MASTER_ROADMAP.md`](MASTER_ROADMAP.md)
* Counts and current state: [`DASHBOARD.md`](DASHBOARD.md)
* Frames and what they prove: [`VISUAL_EVIDENCE.md`](VISUAL_EVIDENCE.md)
* Timings: [`PERFORMANCE_BASELINES.md`](PERFORMANCE_BASELINES.md)
* Task → requirement mapping: [`OPENSPEC_MAP.md`](OPENSPEC_MAP.md)

Snapshot: 2026-11-11. Last code checkpoint: `feb8bfe`.

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
The five deferred slices — VG-03 foliage wind, CL-05 cell-size spectrum, CL-06
edge structure, RD-01 invalidation audit and VL-03 visual acceptance — are
implemented, evidenced and archived. The two former blockers also closed on
2026-11-09: the release baseline (P0-04) and the three-mode smoke (VL-01).

The scientific-correctness extension also closed three items on 2026-11-11. RCS
is no longer one scalar clamp per axis: `domain::services::rcs` owns a
representative six-thruster pure-couple cluster with allocation, failure, a
0.1 N·s minimum impulse and a propellant draw, and gimbals now take the primary
pitch/yaw command in every phase while RCS supplies only the residual (the old
ascent sent the full command to both, doubling pitch/yaw authority). The Falcon 9
source entry records why the mutable vendor page cannot honestly be byte-pinned,
so it stays `Representative` and run identity pins the loaded configuration hash
instead. And the two lateral aerodynamic slopes are now equal at 1.5 per radian,
which makes the axisymmetric airframe roll-invariant; all three baselines pass
unchanged. The headless Falcon 9 flight now closes at 201.1783 x 199.9759 km with
42.483 kg of RCS propellant used. The booster recovery was then flown on the same
flight and exposed two real defects: a landing-gear vehicle deployed parachutes
because the chute model is per-body, and a wreck re-published a crash every fixed
tick. Both are fixed and tested. The return itself still does not land — the
representative 15 t reserve is consumed by boostback, the scenario leaves the pad
target unset, and the crash exposes a spinning-contact energy pump — so recovery
remains IMPLEMENTED-tested, not flight-verified. What that does *not* claim is
recorded in the audit: the cluster is representative, a failed-thruster flight was
not flown, recovery is not verified, and the insertion law still trims apsides
only.

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
| Synthetic detail fabricates lakes on the coastal plain | **verified and closed.** A land-only sea-datum guard, bounded by the octave amplitude, stops negative detail crossing the datum on land while the ocean floor keeps signed relief; all three baselines unchanged (`2026-11-05-td06-papua-*`) |
| Three launch pads sat under water: Starbase, KSC and Wallops were −0.78, −0.14 and −13.61 m against the local sea datum | **verified and closed.** Resident payloads replace the erosion refinement, the platform is floored 3 m above the sea datum, and all seven captured pads are dry |
| Terrain outside the resident payload tile depended on the erosion cache: rasters were baked from the streamed payload overlay, so load order and an old disk tile changed height, moisture and flow | **verified and closed.** Erosion now bakes from the immutable measured global DEM (`5134ed2`); Starbase cold/warm/off agree to 2.4–2.5/255 mean and Baikonur to 3.0–3.2/255, the same as two identical warm runs. A cold or cache-off run needs ~1,200 frames to finish streaming |
| Land nearly black at 400 km while clouds are white | **verified — explained, and the near-ground half fixed.** The altitude albedo is identical at 400 km and 3 km; the deck is white because it is white and the forest albedo is ~4 %. The near ground was 2.2× brighter because the local surface map carried absolute albedo 4× the measured overview; it is now a tint anchored to the overview, so a descent moves the tone +2 % instead of +128 % (TM-04) |
| Cloud edges are cut-outs; one shape per region | **verified and closed.** The deck carries a cell-size spectrum from 4.5 km to 1.1 km by re-weighting its existing octaves, and the coverage band is a resolved smoothstep with footprint-filtered mips. Bright share at 1,200 km is 0.0350 against 0.0332 before, inside ±0.02; `2026-11-08-cl05-deck-1200km.jpg`, `2026-11-08-cl06-deck-400km.jpg` |
| Noon horizon reddened to tan/pink and the limb tail read teal | **verified and fixed by a bounded diffuse fill** (AT-04); the exact-horizon cloud-deck band remains a cloud task |
| Vegetation reads as specks | **verified and improved.** Near-band crowns are closed lobed volumes rather than crossed cards, and foliage sway is graded from each vertex's attachment. The far band keeps impostors for density. `2026-11-08-vg02-papua-300m-crown.jpg`, `2026-11-08-vg03-wind-*` |
| Regional imagery steps at its patch edge | **verified and closed.** The edge is now a 16.6 km cross-fade at the region bound; the worst step near the bound fell from 4.23 to 0.72/255 (the frame's own texture level) and the change is confined to the declared band |
| No geometric terrain detail beyond LOD 11 | **verified and closed.** A mesh-only ~4 km / ±30 m band on LOD 5–11 adds distant relief; collision, anchor and coastline are unchanged (`2026-11-07-tm02-*`) |
| **Windowed presentation** | **resolved for validation.** All three modes present on the 2026-11-09 X11/i3 session and the three-mode smoke passes. With the default AutoVsync the loop quantizes to about one present per second on this PRIME display, so P0-04 uses the documented vsync-off profiling mode and says so |

The windowed probe changes what is known about the environment but does not
close `P0-04` or `VL-01`; both still require their own acceptance runs. The
offscreen path remains the capture path either way.

## What has been verified?

All forty-five roadmap tasks, each with implementation + tests + a named frame
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
| TD-12 near-datum water | the Papua strip is measured NASADEM (4.3 cm below the datum; the eroded base is +4 m above it); the ocean cap had used its 0.5 m depth-test offset as a flood threshold and submerged measured land at 0.28–0.46 m; classification now follows the datum, offset 0.05 m, pale water share 8.19 % → 3.27 %; `2026-11-07-td12-papua-after.jpg` |
| TD-07 regional DEM at Papua | site resolves at payload level 12 (64×64), measures 71.978 m against the bake probe, global fallback 77.592 m (5.61 m refinement); 3 km and 300 km frames; `2026-11-07-td07-*` |
| TD-08 vertical datum | one ellipsoidal height, one EGM2008 datum, one subtraction; synthetic 5 m geoid test across source/mesh/class/water/collision; Papua control point 0.56 m orthometric plain, 3.00 m pad; `geoid_is_applied_once_across_source_mesh_and_collision`, `papua_control_point_reports_one_datum_across_consumers` |
| TD-09 seam continuity | twelve cube-face pairs agree exactly (0 m); antimeridian step 4.8 mm at a 0.1 m separation; polar spread 1e-12 m; imagery wrap and pole tests already present |
| TM-01 material under grading | classification samples the natural surface via `appearance_height_m`; geometry stays graded; gentle and 45° synthetic tests, the latter keeping rock instead of a flat disc; `2026-11-07-tm01-vandenberg-pad.jpg` |
| TM-02 distant relief | mesh-only ~4 km / ±30 m band, band-pass fade LOD 5–11, land-only sea guard; collision and coast unchanged; 40 km height diagnostic shows continuous contours; 241 tiles / 91.6 MiB, 1.40 ms main-thread streaming; `2026-11-07-tm02-*` |
| TM-03 coastal reconciliation | Bight control transect: DEM coast ≈132.138°, albedo ≈132.136° (0.002° ≈ 0.2 km) inside the 4.3 km overview texel; luminance-midpoint method; 3 km frame `2026-11-07-tm03-bight-coast-3km.jpg` |
| VG-01 vegetation distribution | gate/species/treeline use height above the local sea datum (the raw ellipsoidal test accepted a tree 5.2 m under the sea at Papua); generator version 1 + pinned seed/salt; real coastal-patch test, treeline test, identity test; `2026-11-07-vg01-*` |
| VG-02 volumetric canopy | near-band crowns are closed lobed volumes with branch tips kept as cards; the far band keeps impostors; 3,000-instance draw path unchanged; `2026-11-08-vg02-papua-300m-crown.jpg` |
| VG-03 foliage wind and shading | the deck publishes the exact wind direction it advects with and vegetation reads that vector; sway is graded from each vertex's attachment; fixed-pose half-period pair measures a 1 px crown shift at 0.058 m/px, consistent with the clamped 0.10 m amplitude; `2026-11-08-vg03-wind-*` |
| CL-05 cloud cell-size spectrum | re-weights the base volume's 6/12/24-cell Worley octaves (about 4.5/2.25/1.1 km) from already-fetched weather and coarse channels; selector zero reproduces the old carve; bright share 0.0350 vs 0.0332 at 1,200 km, inside ±0.02; `2026-11-08-cl05-deck-1200km.jpg` |
| CL-06 cloud edge structure | coverage band wrapped in a smoothstep whose lower quarter is the resolved edge; march uses the coarser of ray step and pixel footprint; 400 km edges are continuous ramps; `2026-11-08-cl06-deck-400km.jpg` |
| RD-01 invalidation audit | every `Update` system in `TerrainRenderPlugin` classified; the vegetation uniform no longer rewrites unchanged inputs; two 600-frame stationary windows report 0 mesh / 0 material / 0 image creations, 0 published/activated patches and 0 duplicate requests |
| LF-02 grounded hardware | hold-down clamps floated 0.10 m and deluge rails were buried 0.25 m; both grounded; spawned-scene AABB test over tanks, blockhouse, poles, legs, clamps and rails; `2026-11-07-lf02-starbase-pad-ground.jpg` |
| LF-03 flame trench | a `TerrainMeshCutout` stops the rendered ground at the hardstand while collision keeps the same surface; a guaranteed 36.5 m opening leaves the trench and deflector visible; `aa7da40` |
| LF-04 facility scale | clamp ring, level count and support scaling follow the vehicle envelope; hardstand and trench stay fixed; `cf3fd0a` |
| VL-02 OpenSpec strict | 43/43 |
| VL-03 visual acceptance | manifest + harness; `10 scenes pass`, including an orbital-altitude 400 km viewpoint |
| P0-04 performance baseline | release `8ea9207`, vsync off, prelaunch: 720p p50/p95/p99 19.76/27.25/32.15 ms, 1440p 51.11/59.07/63.71 ms; `PERFORMANCE_BASELINES.md` |
| VL-01 three-mode smoke | `solar`, `craft`, `rocket` each 25 s, all exit 124, zero hard errors; one windowed frame per mode (`2026-11-09-vl01-*`) |

Every roadmap task is now verified. There is no remaining implementation or
measurement item in this programme.

## What is blocked or needs a decision?

**Nothing.** The two former environmental blockers closed on 2026-11-09. Their
limits are recorded rather than hidden: P0-04 measured prelaunch only, with vsync
off, and VL-01 does not exercise input beyond the screenshot key.

## What should be built next?

**The realism programme is complete.** All 45 tasks are verified.

If the programme is extended, the known residuals are the smallest visible
defects rather than unstarted roadmap items: a ~0.4 m sea-cap sliver at one
trench corner, hardstand/trench geometry that does not scale with very small
vehicles, faceted crowns at extreme close range, and sparse vegetation at 600 m.

A separate scientific-correctness track has been opened and is recorded in
`docs/scientific_correctness_audit.md`. It found and fixed seven defects — the
retro-propulsion correlation being applied to powered ascents, orbit insertion
targeting the WGS-84 geodetic latitude instead of the geocentric one, orbital
inclination being measured about the geometric pole instead of the rotation
axis, the Falcon 9 catalog carrying 5.4 g of thrust-to-weight on legacy test
masses (now public Block 5 masses, 1.41 g), a closed-loop insertion law that
started its trim before the apsis and cycled the upper stage's ignition budget
without circularizing, an undefined body lateral-axis convention whose
static-margin comment described the opposite sign, and an ascent attitude target
that commanded a ~4 rad roll slew at liftoff and forced a 5e7 N·m RCS clamp —
and records two further audited items: handedness (which led to the third
defect) and per-thruster RCS allocation. Preserving the pad roll cut the RCS
requirement to 5e3 N·m, the per-thruster model now runs at that physical
authority, the mutable Falcon 9 web source records why it cannot be byte-pinned,
and the lateral aero slopes are equalised. A headless Falcon 9 ascent reaches a
201.18 x 199.98 km orbit (e ≈ 9.1e-5) with 42.5 kg of RCS propellant used. None
of this closes a roadmap task.

The open scientific-correctness items are recorded in the audit's "Recorded
residuals": the `enu_basis` geodetic-normal residual, the representative RCS
station/tank layout with an unflown recovery at the new allocation, and the
black offscreen solar capture path. The insertion law still has no plane-change
burn, but the flight measures its plane error at 0.00135°, so that is a
capability gap for off-nominal insertion rather than a defect on the current
trajectory.

## How do we know it works?

```bash
cargo fmt --check
cargo check --all-targets --features dem
cargo clippy --all-targets --features dem -- -D warnings
cargo test --features dem --lib          # 1227 passed, 2 ignored
cargo test --no-default-features --lib   # 1175 passed, 2 ignored
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

The simulator runs and its physics is regression-locked; 1,227 domain and adapter
tests pass (1,175 without the `dem` feature) and OpenSpec is strict-clean. The
realism programme has verified all forty-five tasks, and the separate
scientific-correctness track has closed its per-thruster RCS, source-identity and
lateral-aerodynamics items with the residuals recorded.

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

Two hard constraints were recorded rather than worked around, and one has since
cleared. Windowed validation now runs on the X11/i3 session; the offscreen capture
path remains because it is deterministic and does not need a compositor. The
other constraint is the terrain guard that would stop synthetic relief from
fabricating lakes. The other was a terrain guard that would stop
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

All ten required visual scenes now have current evidence and pass the harness;
the `orbit` row is an orbital-altitude viewpoint rather than a vehicle in orbit,
and says so.

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

The Papua water bodies were the last open question from the terrain audits, and the
answer was neither of the two candidates. They are measured: the NASADEM payload
puts the strip four centimetres below the sea datum, and the source itself calls
it ocean. Erosion is four metres above the datum there. What made the bodies look
invented was the renderer: the ocean cap had used its half-metre depth-test lift
as the wet/dry threshold, so every measured plain within half a metre of the sea
was drawn as water. The lift now belongs to the surface only; classification
follows the datum, and the measured plain at 0.28–0.46 m stays land.

The scientific-correctness track closed three more items with the same discipline.
The RCS was not missing authority but a model: actuation had one scalar clamp and
the ascent sent the full PID torque to both the gimbals and the RCS, so the
hardware duplication was the defect. A representative per-thruster cluster now
owns the torque envelope and the gimbals keep the primary command. The lateral
aero anisotropy was a genuine non-invariance — an axisymmetric body cannot have a
1.5 pitch-plane and a 0.8 yaw-plane slope without a physical reason — so both
planes now use 1.5 and a roll-rotation test pins it. And the Falcon 9 source is
not pinned because the cited page is mutable vendor HTML: the audit records that
refusal instead of inventing a checksum, while run identity still pins the loaded
configuration bytes. The flight numbers moved by 0.33 km in apsis and 42.5 kg of
RCS propellant, attributed rather than rounded away.
