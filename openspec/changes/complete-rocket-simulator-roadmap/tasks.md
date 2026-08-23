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
      material selection (`terrain_textures.rs`); no custom shader asset
- [x] 1.9 Wired only in rocket mode
- [-] 1.10 Covered indirectly by runtime sweeps (no dedicated z-fighting test)

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
- [ ] 2.9 Full orbital → deorbit → reentry → KSC landing integration test

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
- [ ] 3.11 Real-DEM flight integration test

## 4. Entry Physics — SHIPPED

Delivered under `openspec/changes/archive/2026-08-23-entry-physics/`.
All items complete (heating, ablation, blackout, parachutes,
retro-propulsion, wiring, tests).

## 5. Telemetry UI — PARTIAL (egui replaced by Bevy HUD)

- [-] 5.1 Delivered as HUD systems in `rocket_hud.rs` + `RocketSet::Telemetry`
      (no separate plugin, no egui)
- [-] 5.2 Orbital elements panel = HUD ORBIT group (`OrbitalElements` component)
- [ ] 5.3 Trajectory prediction panel/propagator
- [ ] 5.4 Maneuver nodes
- [ ] 5.5 Terrain map panel
- [x] 5.6 `FlightRecorder` ring buffer (component) at fixed rate/capacity
- [-] 5.7 Recorder controls (F9 record toggle / F10 clear / F11 CSV export);
      no playback UI
- [ ] 5.8 Replay/seek mode
- [-] 5.9 Config spread across existing resources (no `TelemetryConfig`)
- [x] 5.10 Wired in rocket mode only
- [ ] 5.11 Record/replay bitwise test

## 6. Staging Recovery — PARTIAL

- [x] 6.1 Recovery modes on `RocketAutopilot` (`Boostback`, `Landing`,
      `PoweredDescent`)
- [x] 6.2 `boostback_guidance` RTLS PD law (analytic; no Lambert targeting to
      moving zones)
- [-] 6.3 Spent-stage entry handled by debris aero/lifecycle system rather
      than full entry guidance per stage
- [x] 6.4 Suicide-burn + hover-slam terminal recovery
- [ ] 6.5 Grid fins + mixer
- [x] 6.6 `LandingLegs` component + `deploy_landing_legs` gate (Phase 13),
      strut contact in GroundContact
- [ ] 6.7 Drone ship entity/station keeping (only terrain-styling enum exists)
- [ ] 6.8 Catch tower
- [x] 6.9 Recovery phase transitions in `guidance_system`
- [x] 6.10 Boostback/throttle/gear unit tests
- [ ] 6.11 Full RTLS/droneship landing integration test

## 7. Determinism Regression Suite — DIFFERENT SHAPE

- [-] 7.1–7.2 Delivered as pinned physics regression tests inside domain
      modules (Tsiolkovsky closed-form vs integration, staging bookkeeping,
      pad T/W, gate/pitch pipeline tests, 100× burn-rig bookkeeping) instead
      of a baseline-RON harness
- [ ] 7.3–7.8 Baseline recording/comparison tooling, CI job, bisect script,
      update command
- [ ] 7.9 Per-variable tolerance config
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
