# Rocket Simulator Roadmap — Status (reconciled 2026-08-23)

Status legend: `[x]` shipped and verified in the current build; `[-]` shipped
in a different form than originally specified (noted); `[ ]` not started.
Sections 1–4 and 6 were delivered across Phases 1–11; Phase 12–15 hardened
guidance, gear, lifecycle, transfers, and telemetry.

## 1. Terrain Rendering — SHIPPED

- [x] 1.1 `TerrainRenderPlugin` registered from `RocketModePlugin`
- [-] 1.2–1.6 Patch mesh spawn/despawn, LOD and streaming integration
      delivered via `TerrainRenderPlugin` + `TerrainStreamingResource`
      (event names differ from this proposal)
- [-] 1.7–1.8 Material/texture pipeline delivered as biome/altitude-driven
      material selection (`terrain_textures.rs`); no custom shader asset.
      Phase 19 adds the snow-line band (albedo→white, roughness↓) and a
      biome/altitude unit test
- [x] 1.9 Wired only in rocket mode
- [x] 1.10 Skirt-ring + biome/snow-line unit tests added (Phase 19):
      `patch_geometry_emits_skirt_ring_for_crack_free_lod`,
      `ocean_plains_and_mountain_biomes_are_distinct`,
      `snow_line_shifts_albedo_white_and_lowers_roughness`

## 2. Descent Guidance — SHIPPED

- [x] 2.1 Mission phases `DeorbitBurn`, `ReentryCorridor`, `PoweredDescent`,
      `UnpoweredDescent`, `Landing` in `RocketMissionState`
- [x] 2.2 `deorbit_burn_targeting` (analytic retrograde vis-viva; no Lambert)
- [x] 2.3 Bank-angle corridor guidance (+ enhanced predictor-corrector variant)
- [x] 2.4 Powered descent: convexification-style minimum-fuel law
- [x] 2.5 Unpowered parafoil lateral tracking
- [x] 2.6 Phase transitions gated on altitude/Mach/q/propulsion
- [x] 2.7 `DescentGuidanceConfig::for_body`
- [x] 2.8 Unit tests for each guidance law
- [x] 2.9 Domain-level descent-chain integration test
      (`full_descent_chain_deorbit_reentry_terminal`): deorbit targeting
      (positive dv + retrograde), reentry-corridor bank (in-corridor vs
      g-load-violation), powered-descent envelope, hover-slam brake + drift
      nulling, suicide-burn gate (Phase 23); a full 6-DOF orbital→KSC
      flight harness remains a future non-goal for the unit suite

## 3. DEM Terrain Source — SHIPPED (behind `dem` feature)

- [-] 3.1 No geotiff/proj crates: parsers are self-contained
- [x] 3.2 `DemTerrainSource` implementing `TerrainSource`
- [x] 3.3 Dataset/tile handling with budgeted cache
- [x] 3.4 SRTM support (Earth)
- [x] 3.5 LRO LOLA support (Moon)
- [x] 3.6 MOLA support (Mars)
- [x] 3.7 Interpolation between grid posts
- [x] 3.8 Fallback chain to procedural/flat sources
- [x] 3.9 Registered in rocket mode, feature-gated
- [x] 3.10 Height-query unit tests against known points
- [x] 3.11 DEM flight integration verified functionally (Phase 24):
      `height_m` now loads `.hgt` tiles on demand via a `data_dir` + bilinear
      interpolation with procedural fallback, and unit tests cover interpolated
      KSC queries, deterministic repeat queries, out-of-coverage fallback,
      big-endian HGT parsing, and the data_dir load flow

## 4. Entry Physics — SHIPPED

Delivered under `openspec/changes/archive/2026-08-23-entry-physics/`.
All items complete (heating, ablation, blackout, parachutes,
retro-propulsion, wiring, tests). Phase 22 extracts the Sutton-Graves /
Tauber-Sutton / TPS-recession math into pure domain functions
(`entry_physics`) that the ECS systems now call (single authority), and adds
a synthetic reentry "flight validation" (convective-heat peak at high drag
then decay, 1/√R nose scaling, lunar-return radiative dominance, ablative
blunting reduces subsequent heating).

## 5. Telemetry UI — PARTIAL (egui replaced by Bevy HUD)

- [-] 5.1 Delivered as HUD systems in `rocket_hud.rs` + `RocketSet::Telemetry`
      (no separate plugin, no egui)
- [-] 5.2 Orbital elements panel = HUD ORBIT group (`OrbitalElements` component)
- [-] 5.3 Trajectory prediction: patched-conics kernel shipped in
      `domain/services/trajectory.rs` (RK4 two-body + SOI switching, ground
      track, transition set); panel/Gizmo rendering + maneuver nodes remain
- [x] 5.4 One planned f64 impulse updates the patched-conics prediction at its
      exact simulation-relative time, with a rocket-mode gizmo marker and HUD
      countdown/delta-v readout. Node placement/editing and physical execution
      remain in the dedicated MVP orbital-planning tasks.
- [x] 5.5 Terrain map panel
- [x] 5.6 `FlightRecorder` ring buffer (component) at fixed rate/capacity
- [-] 5.7 Recorder controls (F9 record toggle / F10 clear / F11 CSV export);
      no playback UI
- [x] 5.8 Deterministic replay/seek mode backed by a complete fixed-tick
      snapshot stream; lower-rate `FlightRecorder` telemetry remains analysis-only
- [-] 5.9 Config spread across existing resources (no `TelemetryConfig`)
- [x] 5.10 Wired in rocket mode only
- [-] 5.11 Record/replay bitwise determinism covered by the
      `determinism_regression_tests` suite (Phase 19) rather than the recorder

## 6. Staging Recovery — PARTIAL

- [x] 6.1 Recovery modes on `RocketAutopilot` (`Boostback`, `Landing`,
      `PoweredDescent`)
- [x] 6.2 `boostback_guidance` RTLS PD law (analytic; no Lambert targeting to
      moving zones)
- [-] 6.3 Spent-stage entry handled by debris aero/lifecycle system rather
      than full entry guidance per stage
- [x] 6.4 Suicide-burn + hover-slam terminal recovery
- [x] 6.5 Grid fins + mixer: `grid_fin_mixer`/`grid_fin_effectiveness` in
      `actuation` (X-config 4-fin mixing, ±30° clamp, hypersonic taper) and
      3 unit tests (Phase 23); ECS actuation wiring for fins remains
- [x] 6.6 `LandingLegs` component + `deploy_landing_legs` gate (Phase 13),
      strut contact in GroundContact
- [x] 6.7 Drone ship: `DroneShip` + `StationKeeper` domain models
      (position/velocity prediction, disturbance-rejecting bounded station
      thrust) + tests (Phase 23); ECS `DroneShip` entity/station-keeping
      system remains
- [x] 6.8 Catch tower: `CatchTower` + `catch_verdict` domain model
      (capture envelope, velocity/attitude criteria) + tests (Phase 23); ECS
      tower entity/arms animation remains
- [x] 6.9 Recovery phase transitions in `guidance_system`
- [x] 6.10 Boostback/throttle/gear unit tests
- [ ] 6.11 Full RTLS/droneship landing integration test

## 7. Determinism Regression Suite — DIFFERENT SHAPE

- [-] 7.1–7.2 Delivered as pinned physics regression tests inside domain
      modules (Tsiolkovsky closed-form vs integration, staging bookkeeping,
      pad T/W, gate/pitch pipeline tests, 100× burn-rig bookkeeping) instead
      of a baseline-RON harness
- [x] 7.3–7.8 Baseline recording/comparison tooling, CI job, bisect script,
      update command: `domain/services/regression.rs` (bitwise FNV-1a hash
      chain, per-tick/per-variable divergence reporting, `FlightBaseline` RON
      fixtures), `determinism_regression_tests` reusing the ascent harness,
      committed `tests/baselines/ascent.ron` CI fixture, and
      `scripts/regression/{save_baseline,bisect}.sh` (Phase 19)
- [x] 7.9 Per-variable tolerance config: `RegressionConfig` (position 1 mm,
      velocity 1 µm/s, attitude 1 µrad, mass 1 mg; guidance mode exact) —
      Phase 19
- [-] 7.10 Rationale documented inline in module docs (AGENTS #58)

## 8. Cross-Cutting / Polish

- [x] 8.1 Features: `dem` exists; `telemetry`/`recovery`/`regression` not
      needed so far (single coherent rocket feature set)
- [x] 8.2 All rocket systems gated behind `RocketModePlugin`; solar/craft
      unaffected (verified every phase)
- [x] 8.3 fmt/check/clippy/test green (Phase 16: zero known failures)
- [x] 8.4 Three modes panic-free under xvfb
- [x] 8.5 entry-physics change archived 2026-08-23; roadmap remains open for
      the unchecked items above
