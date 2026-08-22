## 1. Terrain Rendering

- [ ] 1.1 Create `TerrainRenderPlugin` in `src/infrastructure/plugins/terrain_render.rs` that registers render systems
- [ ] 1.2 Add `TerrainPatchRenderState` component to track mesh/material per patch
- [ ] 1.3 Implement `spawn_patch_mesh` system: observes `TerrainPatchReady` event, generates `Mesh` from `CubeSphere` patch data, creates `StandardMaterial` with biome/altitude properties, spawns entity with `Mesh3d`, `Material3d`, `Transform`
- [ ] 1.4 Implement `despawn_patch_mesh` system: observes `TerrainPatchEvicted` event, despawns entity, releases mesh/material handles
- [ ] 1.5 Add `RenderOrigin` resource and `update_render_origin` system: shifts origin when camera distance > 10 km, updates all patch transforms
- [ ] 1.6 Implement skirt geometry in `CubeSphere::generate_patch_mesh` for crack-free LOD transitions
- [ ] 1.7 Create `TerrainMaterial` asset (extended `StandardMaterial`) with custom shader for biome blending, slope normal perturbation, altitude-based albedo
- [ ] 1.8 Add biome lookup: `TerrainSource::biome_at(lat, lon)` → biome ID; material uses biome ID for texture array indexing
- [ ] 1.9 Wire `TerrainRenderPlugin` into `RocketModePlugin` (only in rocket mode)
- [ ] 1.10 Add integration test: spawn patches, verify mesh entities exist, verify origin shift works, verify no z-fighting at 200 km altitude

## 2. Descent Guidance

- [ ] 2.1 Add `GuidancePhase::DeorbitBurn`, `ReentryCorridor`, `PoweredDescent`, `UnpoweredDescent` to `GuidancePhase` enum in `src/domain/entities/rocket.rs`
- [ ] 2.2 Implement `deorbit_burn_targeting` in `guidance.rs`: Lambert solver for targeted periapsis; analytic retrograde for circular orbits
- [ ] 2.3 Implement `reentry_corridor_guidance`: bank-angle profile with predictor-corrector for cross-range; g-load/q/heat-flux constraints
- [ ] 2.4 Implement `powered_descent_guidance`: lossless convexification for minimum-fuel landing; polynomial fallback
- [ ] 2.5 Implement `unpowered_descent_guidance`: parafoil lateral acceleration tracking
- [ ] 2.6 Add phase transition logic in `guidance_system`: gates on altitude, Mach, dynamic pressure, propulsion state
- [ ] 2.7 Add `GuidanceConfig` with per-body entry interface altitudes, corridor bounds, descent parameters
- [ ] 2.8 Unit tests: deorbit from 400 km circular → 120 km periapsis; reentry corridor hold; powered descent to 0 m/s at 0 m AGL
- [ ] 2.9 Integration test: full orbital flight → deorbit → reentry → powered landing at KSC

## 3. DEM Terrain Source

- [ ] 3.1 Add `geotiff` and `proj` crates to `Cargo.toml` (optional features gated behind `dem` feature flag)
- [ ] 3.2 Create `DemTerrainSource` in `src/domain/services/dem_terrain_source.rs` implementing `TerrainSource`
- [ ] 3.3 Implement `DemTileCache`: LRU cache keyed by (dataset, tile_index); integrated with `TerrainStreamingResource` memory budget
- [ ] 3.4 Implement SRTM loader: downloads/opens GeoTIFF, parses with `geotiff`, caches tiles
- [ ] 3.5 Implement LRO LOLA loader: handles polar stereographic + geographic projections
- [ ] 3.6 Implement MOLA loader: geographic projection on Mars sphere
- [ ] 3.7 Implement bilinear interpolation for height queries between grid posts
- [ ] 3.8 Add fallback chain: `DemTerrainSource` → `ProceduralTerrainSource` → flat sea level
- [ ] 3.9 Add `DemTerrainSource` to `TerrainSource` registry in `RocketModePlugin` (feature-gated)
- [ ] 3.10 Unit tests: SRTM tile load, height query matches known benchmark points (e.g., Everest summit)
- [ ] 3.11 Integration test: rocket flies over real terrain, collision heights match DEM

## 4. Entry Physics

- [ ] 4.1 Create `ThermalState`, `AblationState`, `ParachuteState` components in `entity_components.rs`
- [ ] 4.2 Implement `compute_heating` system (FixedUpdate, before integration): Sutton-Graves convective + Tauber-Sutton radiative
- [ ] 4.3 Implement `compute_ablation` system: char-layer recession, updates nose radius, mass loss
- [ ] 4.4 Implement `compute_plasma_blackout`: electron density model, emits `CommsBlackoutEvent`
- [ ] 4.5 Implement `compute_parachute_forces`: 3-stage inflation model, applies drag at canopy attach point to force accumulator
- [ ] 4.6 Implement `compute_retro_propulsion`: plume-freestream base pressure correlation
- [ ] 4.6 Add `EntryPhysicsConfig` with per-body coefficients, material properties, parachute parameters
- [ ] 4.7 Wire all entry physics systems into `RocketModePlugin` FixedUpdate chain (before force accumulation)
- [ ] 4.8 Unit tests: heating peak at max q; ablation mass loss matches analytical; parachute terminal velocity
- [ ] 4.9 Integration test: Earth reentry from lunar return → blackout → parachute deploy → splashdown

## 5. Telemetry UI

- [ ] 5.1 Create `TelemetryPlugin` in `src/infrastructure/plugins/telemetry.rs` with `UiSystems` set
- [ ] 5.2 Implement `OrbitalElementsPanel`: reads `RocketPhysicsState` + `Gravity`, computes osculating elements, renders with `egui`
- [ ] 5.3 Implement `TrajectoryPredictionPanel`: patched-conics propagator in background `TaskPool`, renders gizmo line strip
- [ ] 5.4 Add maneuver node support: click on prediction gizmo → places node → re-propagates
- [ ] 5.5 Implement `TerrainMapPanel`: orthographic projection, samples `TerrainSource` for height colorization, draws ground track
- [ ] 5.6 Create `FlightRecorder` resource: circular buffer of `RecordedFrame` (full state snapshot), configurable rate/capacity
- [ ] 5.7 Implement `FlightRecorderPanel`: record/stop, playback, timeline scrubber, speed control
- [ ] 5.8 Add `FlightReplaySeekEvent` and replay mode: overrides `SimulationTime`, restores state from buffer
- [ ] 5.9 Add `TelemetryConfig` for panel visibility, recorder settings, prediction horizon
- [ ] 5.10 Wire `TelemetryPlugin` into `RocketModePlugin`
- [ ] 5.11 Integration test: record 5-min flight, replay, verify bitwise state match

## 6. Staging Recovery

- [ ] 6.1 Add `RecoveryGuidance` mode to `RocketAutopilot` in `entity_components.rs`
- [ ] 6.2 Implement `boostback_guidance`: Lambert targeting to recovery zone (RTLS pad, drone ship, catch tower)
- [ ] 6.3 Implement `entry_guidance` for stage: bank-angle profile like reentry but for stage mass/aero
- [ ] 6.4 Implement `terminal_recovery_guidance`: hover-slam or powered descent to target
- [ ] 6.5 Add `GridFin` component and `grid_fin_mixer` in `actuation.rs`: torque → fin deflections via pseudo-inverse, Mach/alpha effectiveness tables
- [ ] 6.6 Add `LandingLeg` component and `deploy_landing_legs` system: trigger at altitude/velocity gate, verify lock
- [ ] 6.7 Create `DroneShip` entity/component: station-keeping PID, publishes `DroneShipPrediction` resource
- [ ] 6.8 Create `CatchTower` entity/component: arm kinematics, capture envelope, success criteria
- [ ] 6.9 Add recovery guidance phase transitions in `guidance_system` (separate from main vehicle)
- [ ] 6.10 Unit tests: boostback delta-v matches analytic; grid-fin mixer produces correct torque; leg deployment sequence
- [ ] 6.11 Integration test: Falcon 9 first stage → separation → boostback → entry → landing on drone ship

## 7. Determinism Regression Suite

- [ ] 7.1 Create `tests/determinism_regression.rs` with `DeterminismTestHarness`: headless Bevy app (FixedUpdate only, no renderer, fixed seed)
- [ ] 7.2 Define `RecordedFrame` schema (all physics state fields) and `BaselineFlight` struct (metadata + frame hashes)
- [ ] 7.3 Implement baseline recording mode: `cargo test --test determinism_regression -- record` runs flights, writes `tests/baselines/<flight>.ron`
- [ ] 7.4 Implement baseline comparison mode: re-simulates, computes xxHash64 per frame, compares with tolerance from `RegressionConfig.toml`
- [ ] 7.5 Create initial baseline set: suborbital_hop, leo_insertion, gto_insertion, lunar_transfer, earth_reentry, moon_landing, rtls_recovery, droneship_recovery
- [ ] 7.6 Add CI job (`.github/workflows/determinism.yml`): runs comparison mode, fails on divergence, artifacts diff on failure
- [ ] 7.7 Implement bisection wrapper script: `scripts/bisect_regression.sh` uses `cargo-bisect-rust` to find first bad commit
- [ ] 7.8 Implement baseline update command: `cargo test --test determinism_regression -- update` writes new baselines with metadata (git hash, date, author, PR)
- [ ] 7.9 Add `RegressionConfig.toml` with per-variable tolerances
- [ ] 7.10 Documentation: `docs/determinism_regression.md` explaining how to run, interpret, update baselines

## 8. Cross-Cutting / Polish

- [ ] 8.1 Update `Cargo.toml` with new optional features: `dem`, `telemetry`, `recovery`, `regression`
- [ ] 8.2 Ensure all new systems are gated behind `RocketModePlugin` (not in solar/craft modes)
- [ ] 8.3 Run full validation: `cargo fmt --check && cargo check && cargo clippy && cargo test`
- [ ] 8.4 Verify all three modes still launch: `cargo run`, `cargo run -- craft`, `cargo run -- rocket`
- [ ] 8.5 Archive this change: sync specs to `openspec/specs/`, archive to `openspec/changes/archive/`