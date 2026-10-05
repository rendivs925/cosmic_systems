## 1. Presentation Foundations

- [x] 1.1 Add pure, unit-named Rocket presentation parameter mappings for plume intensity/expansion, ignition, ground interaction, aerodynamic shock, heating, and external-audio attenuation.
- [x] 1.2 Add focused deterministic tests for bounded, finite mappings across sea level, max-Q, thin atmosphere, and vacuum conditions.
- [x] 1.3 Add Rocket-mode-only presentation components/resources that retain render-only smoothing and quality state without duplicating simulation state.

## 2. Engine And Lifecycle Effects

- [x] 2.1 Spawn bounded child engine-effect entities from catalog engine stations and configure reusable core, inner, and outer plume materials/meshes.
- [x] 2.2 Update engine effects after Rocket render interpolation from authoritative propulsion, ambient pressure, density, and flight conditions.
- [x] 2.3 Add render-only ignition, thrust, plume expansion, subtle shock, and thermal visual transitions with distance/visibility quality LOD.
- [x] 2.4 Rebuild or retire engine-effect children at existing staging, separation, and relaunch boundaries without leaving stale presentation entities.
- [x] 2.5 Add lifecycle and render-origin stability regression tests for engine-effect ownership and local transforms.

## 3. Ground And Environmental Presentation

- [x] 3.1 Add bounded terrain-relative liftoff dust/haze and pad illumination presentation driven by active propulsion and authoritative terrain distance.
- [x] 3.2 Extend the procedural pad with flame-trench, deluge/deflector, structural, and local-emissive presentation detail while preserving its visual-only role.
- [x] 3.3 Improve existing offline terrain-surface and vegetation appearance with deterministic Papua tropical profile inputs and preserve streaming/cache budgets.
- [x] 3.4 Tune Rocket atmosphere, cloud, ambient, and fog presentation transitions from dense atmosphere to vacuum using existing flight conditions and ephemeris lighting.

## 4. Papua Launch Site

- [x] 4.1 Add a validated named Papua coastal-lowland preset to the existing launch-site value-object module with coordinate regression coverage.
- [x] 4.2 Make Rocket spawning use the Papua preset while retaining terrain-derived elevation/normal and the existing geodetic/body-fixed/inertial conversion path.
- [x] 4.3 Rename launch-site comments, visual labels, and user-facing presentation references that still identify the Rocket pad as Kennedy Space Center.

## 5. Audio And Content Provenance

- [x] 5.1 Add derived Rocket audio control parameters for engine, ground rumble, staging, and interior/external atmospheric attenuation at a bounded update cadence.
- [x] 5.2 Evaluate selected external non-terrain assets and record source URL, exact license, attribution/notice requirements, modification status, and intended use before adding each asset.
- [x] 5.3 Add approved Rocket-specific audio sources and lifecycle-controlled playback only after provenance review; do not reuse the UFO electronic loop.

## 6. Performance And Validation

- [x] 6.1 Extend cadence-limited presentation telemetry with actionable effect counts and update cost, preserving existing shared performance metrics ownership.
- [x] 6.2 Run formatting, native DEM check/test/release build, strict OpenSpec validation, and all three bounded mode startup checks. Clippy was not rerun per user instruction.
- [x] 6.3 Capture native release Rocket measurements for prelaunch, ignition/liftoff, ascent, thin atmosphere, staging, and render-origin/camera stress; report p50/p95/p99 and any unmeasured limitations.

  Native `:0`, release, default quality, `--features dem`,
  `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`, 600-frame windows (2026-10-05):
  - Prelaunch: p50/p95/p99 17.4/27.0/80.7 ms, 0 visible effects.
  - Ignition/liftoff + 1x ascent to ~22 km: p50 19-25 ms, p95 30-47 ms, p99
    45-67 ms, 27-30 visible effects at 0.03 ms effect-update cost.
  - Thin atmosphere / near orbit (~79.7 km): p50 ~28-44 ms as terrain
    residency approaches its budget.
  - Staging: HUD events `t+45.6 STAGE SEPARATED (-33000 kg)` and
    `t+47.6 STAGE 2 IGNITION` with the chase camera tracking the upper stage.
  - Render-origin/camera: stable chase presentation through ascent with no
    visible re-origin jump.
  - GPU passes: opaque 1.4-2.8 ms, transparent 0.26-0.61, bloom ~0.35,
    tonemapping ~0.11, shadow ~0.07-0.09, upscaling ~0.056.
  - Initial terrain build produced a one-time loading stall (p95 940 ms, p99
    3399 ms) before steady state.
  Unmeasured limitation: GPU memory is not directly instrumented, and the
  frame-time rise in the thin-atmosphere window tracks terrain residency rather
  than any effect cost.

  The native-display follow-up is recorded in
  `docs/native_release_performance_audit.md`. It confirms discrete-GPU release
  rendering and bounded terrain streaming, but preserves this unchecked task
  because its GPU pass timing, staging, high-altitude, and isolated camera
  evidence remain unavailable.
