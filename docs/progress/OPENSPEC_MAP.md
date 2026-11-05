# Task → OpenSpec map

OpenSpec owns the requirements. `MASTER_ROADMAP.md` owns the work. This file is
the join between them, so neither has to restate the other.

Where a task maps to a spec, the task's acceptance criteria are a pointer to that
requirement. Where no requirement exists, the gap is listed rather than papered
over with a new duplicate spec.

## Mapping

| task | OpenSpec spec | active change | note |
| --- | --- | --- | --- |
| P0-01 capture view | — | — | measurement facility, deliberately outside the requirement set |
| P0-02 offscreen capture | — | — | measurement facility; documented in `docs/measurement/offscreen_capture.md` |
| P0-03 diagnostic modes | — | — | **gap**: no requirement for developer diagnostics; behaviour is instead pinned by the uniform-layout parity test and `docs/measurement/terrain_diagnostics.md` |
| P0-04 performance refresh | `scientific-validation` | — | evidence obligation only |
| TD-01 ocean datum | `planetary-water` | — | |
| TD-02 site grading | `terrain-source`, `launch-site-presentation` | — | grading lives in the terrain authority; pad anchors come from the resident payload and a 3 m platform floor |
| TD-03 imagery colour match | `earth-imagery-streaming` | — | measured per-channel linear gain before mip generation; region/overview ratio 3.87× → 1.16×; coarse-LOD mosaic coverage below level 8 remains a package limit |
| TD-04 imagery feather | `earth-imagery-streaming` | — | |
| TD-05 albedo blocks | `earth-imagery-streaming`, `terrain-rendering`, `planetary-terrain-material` | — | cause not yet attributed, so no single requirement owns it |
| TD-06 fabricated coastline | `terrain-source` | — | added and archived `2026-11-05-guard-coastal-synthetic-detail`: land-only sea-datum guard, continuous underwater fade, worst-case non-crossing |
| TD-07 regional DEM | `elevation-tile-payload`, `terrain-source` | — | |
| TD-08 vertical datum | `terrain-source` | — | |
| TD-09 seam continuity | `terrain-source`, `terrain-lod` | — | |
| TD-10 pad anchor vs payload and cache | `elevation-tile-payload`, `terrain-source`, `determinism-regression` | — | payload outranks erosion for the sample; erosion bakes from the immutable global DEM so load order and the disk cache cannot select a different field |
| TD-11 coastal datum clearance | `terrain-source`, `planetary-water` | — | platform floor at sea + 3 m; the ascent fixture pins its launch altitude, so baselines are unaffected |
| TD-12 near-datum base water | `terrain-source`, `terrain-erosion` | — | open: compare persisting water against the measured DEM before changing erosion |
| LF-01 footprint | `launch-site-presentation` | — | |
| LF-02…04 facility | `launch-site-presentation` | — | |
| TM-01 material stability | `planetary-terrain-material`, `terrain-material-splat`, `terrain-lighting-occlusion` | — | |
| TM-02 distance detail | `terrain-lod`, `terrain-rendering` | — | |
| TM-03 land/sea reconciliation | `earth-imagery-streaming`, `terrain-source` | — | |
| TM-04 near-ground albedo tone | `planetary-terrain-material`, `terrain-material-splat`, `scientific-lighting` | — | tone continuity between the measured overview and the layered material |
| VG-01 distribution | `vegetation-land-cover`, `vegetation-placement`, `vegetation-species` | `stabilize-far-field-tree-placement` | change tasks complete, not archived |
| VG-02 canopy | `vegetation-species` | — | |
| VG-03 wind and shading | `vegetation-species` | — | |
| CL-01…06 clouds | — | — | **gap**: requirements live in `docs/technical_specification_photorealistic_procedural_3d_cloud_engine.md`, not in OpenSpec |
| AT-01 sky | `atmosphere`, `rocket-flight-presentation` | — | |
| AT-02 altitude/limb | `atmosphere`, `rocket-flight-presentation` | — | continuous sky/fog transition is specified; limb thickness is visual evidence, not a separate requirement |
| AT-03 high-altitude darkness | `atmosphere`, `scientific-lighting` | — | |
| AT-04 multiple-scattering horizon | `scientific-lighting` | — | added and archived `2026-11-05-add-sky-multiple-scattering`: bounded, luminance-preserving diffuse fill with an explicit energy bound and unchanged limb e-folding |
| RK-01 attitude/staging | `rocket-flight-presentation`, `rocket-dynamics`, `staging-recovery` | — | |
| RK-02 plume | `rocket-engine-effects`, `flight-presentation-effects` | — | |
| RD-01 invalidation | `terrain-rendering`, `terrain-lod` | — | |
| RD-02 streaming measurement | `terrain-lod`, `terrain-rendering` | — | culling, LOD reconcile and the upload burst are timed separately; GPU upload completion is still unexposed and not claimed |
| RD-03 shadow lifecycle | `terrain-lighting-occlusion` | — | |
| VL-01 smoke | `scientific-validation` | — | |
| VL-02 spec validation | — | — | process check |
| VL-03 visual suite | `scientific-validation` | — | |

## Requirement gaps, ranked

1. **Clouds have no OpenSpec spec.** The cloud engine has a thorough technical
   specification document and a long implementation history, but no OpenSpec
   requirement with scenarios. Consequence: CL-01…06 cannot point at an
   authoritative acceptance criterion, and "what does correct look like" lives in
   a doc that is not validated. **Action:** write an OpenSpec change for the cloud
   presentation requirements when CL-05 starts, not before — it must describe the
   behaviour that survives that work.
2. **Diagnostics have no requirement.** P0-03 is developer tooling. A short spec
   is still worth having so "the mode must not change the simulation" is
   enforceable.
3. **Visual atmosphere beyond `atmosphere`.** The spec covers the physical model;
   continuity of the ground→space transition and the limb are not specified.
4. **`spaceflight-game-mvp` is active with 5/37 tasks done.** It is unrelated to
   this programme and must not be counted as progress here.

## Rules

* A roadmap task that changes behaviour must be traceable to a spec requirement,
  or explicitly flagged as a gap above.
* Never create a second spec for a requirement that already exists; amend the
  existing one through an OpenSpec change.
* The roadmap does not redefine thresholds. If a task needs a numeric threshold
  that the spec does not state, the spec gets amended first and the roadmap
  points at it.
