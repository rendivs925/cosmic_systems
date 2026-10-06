# Rocket Attitude Regression and Simulator-Quality Plan

## Purpose

Restore the authoritative rocket attitude pipeline after a rendering regression,
repair the terrain-shadow lifecycle and redundant rendering work, then improve
world quality in independently validated stages.

`spaceflight-game-mvp` is deferred. This plan prioritizes simulator correctness,
performance, and visual credibility over game features.

This is a regression. The previous smooth gravity-turn tilt is the expected
behavior; the current "almost vertical at high altitude" appearance must not be
accepted as a new design.

## Non-negotiable constraints

- The physical simulation remains the single authority for attitude, angular
  velocity, guidance, control, propulsion, gravity, terrain height, and
  reference frames.
- Preserve the pipeline: physical attitude -> presentation adapter -> Bevy
  transform.
- Do not fix the appearance by moving/rotating the camera, hard-coding an
  orientation, forcing the vehicle along velocity, or altering guidance.
- Preserve render interpolation, fixed timestep, `RenderOrigin`, deterministic
  headless behavior, and existing performance improvements.
- Do not add a second reference-frame conversion or floating origin. Reuse the
  existing authoritative machinery.
- Do not silently regenerate determinism baselines.

## 1. Current evidence and unresolved questions

| Area | Established from inspection | Still to establish |
|---|---|---|
| Attitude authority | `RocketPhysicsState.dynamics.orientation` feeds fixed render snapshots, quaternion `slerp`, then `Transform.rotation`. | Where the reported flight diverges from the intended physical attitude. |
| Body convention | Dynamics declares **+Y longitudinal/roll axis**. | Full mesh nose/nozzle mapping, including rebuilt and detached stages. |
| Render origin | `render_transform()` subtracts origin from position and passes through orientation. | Whether another path rotates, overwrites, or reparents the rendered vehicle. |
| Recent presentation history | `5a9cf72` moved the interpolation function without changing its mathematics. | Last demonstrably good revision and first bad revision. |
| Schedule | Fixed capture follows integration/contact through `RocketSet::SyncRender`; variable-rate interpolation is ordered after recentering and launch input. | Complete cross-plugin writer inventory and deferred-command/staging behavior. |
| Terrain shadows | Mutable shadow images are created with `RENDER_WORLD` residency; refresh expects main-world access. Baking occurs before lookup, and failed lookup does not consume the refresh budget. | Runtime extraction/eviction evidence and the complete resource lifetime. |
| Trees | The supplied specification is present and untracked. | Differences between the existing implementation and its required geometry/shading behavior. |

No attitude root cause is proven yet. The shadow code has a concrete
repeated-work failure path, but its runtime trigger must still be verified.

## 2. Phase A - Establish reproducible evidence

### A1. Freeze the reproduction configuration

Record:

- Revision and working-tree changes.
- Vehicle/configuration and launch site.
- Ephemeris dataset and starting epoch.
- Guidance settings, launch inputs, staging, and time-warp changes.
- Window resolution, quality settings, VSync, GPU/driver.
- Terrain/imagery availability and cache state.

Use the existing launch-state builder and headless scenario infrastructure. Do
not assume a default headless launch matches the native Papua launch.

### A2. Establish good and bad behavior

1. Reproduce the current ascent.
2. Identify when the physical/render attitude first differs from expectations.
3. Examine history for all relevant paths, including renamed files:
   - Spawn and launch-state initialization.
   - Guidance/control/dynamics.
   - Render snapshots and synchronization.
   - Mesh generation and staging.
   - Reference frames and origin recentering.
   - Whole-transform writers and hierarchy changes.
4. Compare candidate revisions under the same scenario.
5. Once the failure has a deterministic predicate, bisect the narrowed range.

Use isolated revision workspaces during implementation rather than disturbing
the current working tree.

**Gate A:** a recorded failing scenario and either a verified good revision or
an explicitly documented absence of one. A remembered visual result is useful
evidence, but not a substitute for a reproduced baseline.

## 3. Phase B - Trace and repair the attitude pipeline

### B1. Build an authority and writer map

```text
guidance target
  -> controller
  -> actuator commands
  -> forces/torques
  -> physical quaternion/angular velocity
  -> fixed render snapshots
  -> interpolated render quaternion
  -> root Transform
  -> propagated GlobalTransform
  -> mesh-local longitudinal axis
```

For every writer, record:

- Entity/query filters.
- Schedule and ordering.
- Source/destination frame.
- Whether it writes simulation or presentation state.
- Whether it runs during launch, staging, recovery, or relaunch.

Search beyond `.rotation =`: include complete transform replacement, transform
insertion, parenting, `look_at`/`look_to`, quaternion construction, and mesh
replacement.

### B2. Locate the first incorrect boundary

At matching ticks and render frames, capture bounded diagnostic samples:

- Simulation tick/epoch and entity identity.
- Mission phase, stage, command target, angular velocity.
- Physical quaternion.
- Previous/current snapshot quaternions.
- Interpolation fraction.
- Expected render quaternion.
- Actual root and global rotation.
- Longitudinal direction and local vertical.

Use quaternion sign-invariant comparisons. Compare rendering against the
**interpolated** attitude, not blindly against the newest fixed state.

Distinguish:

1. Physical attitude actually changed.
2. Correct dynamics were captured incorrectly.
3. Correct snapshots were interpolated incorrectly.
4. Correct interpolation was overwritten.
5. Correct root rotation was invalidated by hierarchy/model mapping.
6. The displayed object is the wrong stage/entity.

### B3. Verify reference frames and model axes

Confirm from code - not naming alone - that the attitude maps body coordinates
into the inertial frame used by rendering. Verify:

- Model nose and nozzle directions.
- Roll/transverse axes.
- Existing model correction, if any.
- Parent transform inheritance.
- Detached/recovering-stage initialization.
- Origin changes without unintended rotational rebasing.

Do not add a frame conversion merely because the architecture diagram includes
one. If physical and render bases already match, the correct orientation
conversion is the existing pass-through.

### B4. Implement the minimum proven fix

Change only the identified defective boundary. Preserve:

- Camera configuration and behavior.
- Authoritative guidance and dynamics.
- Fixed timestep and simulation clock.
- Render interpolation.
- Existing origin and frame machinery.
- Deterministic baselines and performance improvements.

If the fault is upstream physics rather than rendering, document the evidence
and add a physical regression before changing that implementation.

### B5. Regression coverage

Add meaningful tests for:

- Nontrivial pitch/yaw/roll mapped through the actual model axis.
- Origin rebasing preserving rotation.
- Smooth interpolation at intermediate fractions.
- Quaternion sign-equivalent snapshots.
- Multiple render frames between physics steps.
- Staging and continuing-vehicle attitude continuity.
- Relevant schedule execution preserving the synchronized rotation.
- Presentation leaving physical state unchanged.

Test the real mesh convention, not just `q * Y` against another calculation
using the same assumption.

**Gate B:** the regression fails on the bad implementation, passes after the
fix, and the flight matrix below confirms the result. Only then create
**checkpoint 1: attitude regression fix**.

## 4. Phase C - Repair terrain-shadow lifetime and redundant work

### C1. Prove the lifecycle failure

```text
geometry ready
  -> shadow request/bake
  -> image installation
  -> render extraction
  -> Sun-driven refresh
  -> patch replacement/eviction
```

Inspect residency, strong handles, explicit removal, patch identity, material
references, and replacement order. Distinguish:

- GPU-resident image removed from CPU storage.
- Pending generation.
- Truly missing/evicted asset.
- Stale patch state.

### C2. Correct the refresh contract

For refreshable images:

- Use a residency/update strategy consistent with main-world mutation.
- Validate resource availability before expensive baking.
- Bound attempts, failures, and queued work - not only successful refreshes.
- Preserve stable terrain/water references.
- Retain a valid previous map or neutral fallback while pending.

For asynchronous generation:

- Reuse the existing task infrastructure.
- Deduplicate by patch identity plus relevant geometry/configuration/Sun revision.
- Bound pending jobs and result installation.
- Reject results for evicted or superseded patches.
- Invalidate only on relevant changes.

Do not suppress the warning as a substitute for repair.

### C3. Audit repeated work with counters

Measure:

- Mesh/material/image creation.
- Occlusion bakes and requests.
- Terrain scans, culling and LOD decisions.
- Vegetation generation.
- Cloud resource regeneration.
- Transform and material updates.

Apply **unchanged relevant inputs -> no rebuild**.

Moving cameras legitimately require visibility/LOD evaluation; changing
interpolation fractions legitimately require transforms. Avoid broad
change-detection gates that freeze either.

**Gate C:** no repeated missing-map loop, bounded work, valid fallback, stable
asset counts, passing lifecycle tests, and measured frame-pacing improvement.
Create **checkpoint 2**.

## 5. Phase D - World-quality improvements

Each pass starts with captures and profiling, extends existing systems, and has
its own acceptance gate.

| Checkpoint | Implementation scope | Acceptance |
|---|---|---|
| **3. Terrain, ocean, launch site, lighting** | Fix demonstrated material/relief/coastline defects; inspect LOD and imagery transitions; verify pad/tower anchoring against authoritative terrain; improve lighting/shadows where evidence requires it. | Consistent orbit-to-ground appearance, grounded structures, no terrain/collision disagreement, no new streaming spikes. |
| **4. Trees and vegetation** | Adapt the supplied specification to Bevy/WGSL: continuous ring-extruded branches, smooth junctions, plausible taper, volumetric canopy lobes/normals, translucent cutout foliage, GPU wind, distance LOD. Preserve bounded patch batching. | No disconnected near-field branches, believable canopy volume, matching wind deformation in shadow/depth passes, conservative animated bounds, deterministic generation. |
| **5. Clouds and wind** | Extend existing cloud presentation with separate coverage, shape, detail, and coherent motion; bounded soft volumetric structure and lighting appropriate to measured GPU budget. | Natural multilayer structure from ground/ascent/orbit, no whole-field per-frame regeneration, no single flat scrolling sheet. |
| **6. Atmosphere** | Audit existing scattering and altitude transitions before extending it; ensure planet geometry, aerial perspective, exposure, and limb rendering agree. | Continuous appearance through 10/30/70 km and orbit; no abrupt blue plate or oversized shell. |
| **7. Exhaust and effects** | Use existing propulsion/environment telemetry for plume expansion, intensity, shock structure, turbulence, and ground interaction; reuse bounded effects resources. | Smooth sea-level-to-vacuum evolution, correct nozzle attachment through staging, no physics mutation or particle/resource growth. |

The tree document's algorithms are requirements to adapt; its embedded
standalone Three.js application prompt is not the repository architecture.

## 6. Shared validation matrix

### Flight scenarios

Run continuous ascent through:

**Prelaunch -> liftoff -> 1 km -> 5 km -> 10 km -> 30 km -> 70 km ->
exoatmospheric -> orbital flight**

Also cover:

- Stage separation.
- Pause/resume and supported time warp.
- Origin recentering.
- Rapid camera movement.
- Terrain streaming and LOD transitions.
- Recovery/landing for affected systems.

Compare matched flight states using consistent altitude definitions, epoch,
stage and camera settings. Sparse altitude screenshots alone cannot establish
"no snap"; capture continuous attitude-error traces and visual sequences.

### Numerical acceptance

- Render/model orientation agrees with expected interpolated physical attitude
  within an explicit f32-appropriate tolerance.
- Camera motion does not change the physical attitude.
- Origin recentering introduces no rotational discontinuity.
- Presentation-only changes preserve deterministic physical outputs.
- No silently regenerated ascent baselines.

### Performance acceptance

Run comparable before/after release scenarios, separating cold-cache startup
from warm steady state.

Report **p50, p95, p99, worst**, plus sample count and scenario duration, for
available timings:

- CPU frame time.
- Terrain generation/upload, culling, LOD, shadows.
- Clouds, atmosphere, VFX, vegetation where instrumentation supports attribution.
- GPU pass/frame timings when available.
- GPU utilization separately.
- Draw calls, visible patches, resource counts and pending jobs.

Do not invent missing GPU timings. Define numerical budgets after establishing
the hardware/resolution baseline; a world-quality gain must not silently spend
away recovered frame-time stability.

## 7. Final checks and delivery

```bash
cargo fmt --check
cargo check --features dem
cargo clippy --features dem -- -D warnings
cargo test --features dem
cargo build --release --features dem
cargo check --no-default-features
cargo test --no-default-features
openspec validate --all --strict
```

Then perform native release flight validation and normal/craft regression smoke
runs. Startup alone does not establish ascent correctness.

For each completed checkpoint, deliver:

1. Root cause or measured defect.
2. Scoped changes and reused infrastructure.
3. Regression tests and results.
4. Matched visual evidence.
5. Before/after measurements and unavailable metrics.
6. Remaining limitations.
7. A separate validated commit, staging only intended files.

## Execution order

**Reproduce -> identify first bad attitude boundary -> regression test ->
minimal attitude fix -> native acceptance -> shadow lifecycle fix -> measured
redundant-work removal -> separate world-quality passes -> final integrated
validation.**

## Commit checkpoints

Only create each checkpoint if the corresponding work is actually completed and
validated.

1. Fix rocket attitude/render-frame regression.
2. Fix terrain shadow lifecycle and redundant rendering.
3. Improve terrain/world materials.
4. Improve procedural vegetation/tree rendering.
5. Improve clouds/wind.
6. Improve atmosphere.
7. Improve rocket plume/VFX.

## Open inputs

- The reported screenshot and, if known, the last run/revision with correct
  smooth tilt. This improves the historical comparison but does not block the
  code and deterministic-scenario investigation.
