## Context

The rocket simulator has a complete physics pipeline across 8 archived changes: mode isolation, physical scale/frames, gravity, 6-DOF dynamics, propulsion, atmosphere/aero, guidance/control/actuation, and terrain data/LOD/collision/streaming. The terrain streaming manager (`TerrainStreamingResource`, `stream_terrain_patches`) manages patch lifecycles; `TerrainSource` abstraction separates data from rendering/collision; `CubeSphere` + quadtree LOD is implemented in `cube_sphere.rs`/`terrain_patch_manager.rs`. The flight loop runs in `FixedUpdate` with the chain: guidance → control → actuation → terrain interaction → atmosphere → accumulate forces → aero forces/torque → propulsion thrust/gimbal/consumption/staging → 6-DOF integration → render sync. Physics is authoritative; nothing writes `Transform` directly except `sync_render_transform`. All code follows AGENTS.md constraints (single authoritative implementations, f64 dynamics, ECS composition).

## Goals / Non-Goals

**Goals:**
- GPU terrain rendering from existing streaming patches with crack-free LOD, local-origin precision, PBR materials
- Complete guidance loop from orbit to touchdown (deorbit, reentry corridor, powered/unpowered terminal)
- Real planetary DEM data behind the existing `TerrainSource` interface
- Physically coherent entry physics (heating, ablation, plasma, parachutes, retro-propulsion)
- Operator-grade telemetry UI (orbital panel, trajectory prediction, terrain map, flight recorder)
- Autonomous stage recovery (boostback, grid fins, legs, drone ship, catch tower)
- Deterministic regression suite with baselines, CI gate, bisection, audit trail

**Non-Goals:**
- Multiplayer / networked simulation
- Real-time weather / atmospheric variability beyond static models
- Full orbital mechanics solver replacement (patched conics is sufficient for prediction)
- Photorealistic rendering (PBR with procedural textures is the target)
- CFD-level aerothermal modeling (engineering correlations are sufficient)

## Decisions

### Terrain Rendering Architecture

**Decision:** Spawn meshes from `TerrainPatchManager` when patches reach `Ready` state; use a `TerrainRenderPlugin` that observes `TerrainStreamingResource` events.

**Rationale:** Keeps rendering decoupled from data generation; the streaming manager already has the lifecycle — rendering just reacts to `Ready`/`Evicted` events.

**Alternatives considered:**
- Push meshes from generator: couples generation to rendering, violates separation
- Separate render-world quadtree: duplicates LOD logic, sync complexity

**Key design points:**
- Each patch → one Bevy `Mesh` asset (generated from `CubeSphere` face + quadtree subdivision + skirt indices)
- `Material` per biome/altitude band: use a `TerrainMaterial` asset with `ExtendedMaterial` over `StandardMaterial` for custom shader logic (biome blending, slope-based normal perturbation)
- Floating origin: maintain a `RenderOrigin` resource updated when camera drifts > 10 km; all patch transforms stored relative to this origin
- GPU frustum culling: rely on Bevy's built-in view frustum culling; patches beyond far plane are culled automatically
- LOD transition: skirt geometry (vertical extrusion at patch edges) ensures crack-free adjacency; no geomorphing needed at this scale

### Descent Guidance Architecture

**Decision:** Extend `GuidanceSystem` with new `GuidancePhase` variants: `DeorbitBurn`, `ReentryCorridor`, `PoweredDescent`, `UnpoweredDescent`. Each phase has its own targeting algorithm; phase transitions are altitude/velocity/state gated.

**Rationale:** Reuses the existing guidance/control/actuation separation; control system already handles attitude tracking — guidance just provides new targets.

**Algorithms:**
- Deorbit: Lambert solver for targeted periapsis; analytic retrograde burn for circular orbits
- Reentry corridor: Bank-angle profile (Apollo-style) with numerical predictor-corrector for cross-range
- Powered descent: Convex optimization (lossless convexification) for minimum-fuel landing with thrust bounds; fallback to polynomial guidance for real-time
- Unpowered: Parafoil guidance via lateral acceleration command tracking

**Phase gating:**
- `DeorbitBurn` → `ReentryCorridor` at entry interface altitude (configurable per body, default 120 km Earth)
- `ReentryCorridor` → `PoweredDescent` when Mach < 1.5 and dynamic pressure < 5 kPa
- `PoweredDescent` → `Terminal` at 100 m AGL
- `UnpoweredDescent` triggered if `RocketPropulsion` reports no active engines

### DEM Terrain Source

**Decision:** New `DemTerrainSource` struct implementing `TerrainSource`; loads GeoTIFF/HGT tiles on demand via a `DemTileCache` (LRU, integrated with `TerrainStreamingResource` budget). Uses `geotiff` crate for parsing, `proj` for coordinate transforms.

**Rationale:** `TerrainSource` trait is already defined — new implementation is a drop-in replacement; no renderer/collision changes needed.

**Tile management:**
- Tiles keyed by (dataset, lat/lon tile index)
- Cache eviction coordinated with streaming manager's memory budget
- Fallback chain: DEM → procedural → flat (sea level)

**Coordinate handling:**
- SRTM: geographic (WGS84), 1"/3" spacing
- LRO LOLA: polar stereographic at poles, geographic elsewhere
- MOLA: geographic (Mars sphere)
- All converted to planet body-fixed Cartesian at query time

### Entry Physics Architecture

**Decision:** New systems in `FixedUpdate` before 6-DOF integration: `compute_heating`, `compute_ablation`, `compute_parachute_forces`, `compute_retro_propulsion`. Each writes to dedicated components (`ThermalState`, `AblationState`, `ParachuteState`) read by the force accumulator.

**Rationale:** Keeps physics pipeline modular; each phenomenon is independent and testable.

**Models:**
- Convective: Sutton-Graves (sphere-cone) `q_dot = k * sqrt(rho/R_nose) * v^3` with `k` calibrated per body
- Radiative: Tauber-Sutton approximation for Earth lunar-return; negligible for Mars/Moon
- Ablation: Char-layer recession model `dr/dt = q_dot / (rho_tps * H_abl)`; updates nose radius and mass
- Plasma blackout: Electron density `n_e = f(rho, v, T_wall)`; blackout if `n_e > n_crit(f_comms)`
- Parachutes: 3-stage inflation (mortar → reefed → full) with `Cd(t)` curve; force = 0.5 * rho * v^2 * Cd(t) * A_ref applied at canopy attach point
- Retro-propulsion: Plume-freestream interaction via empirical base pressure correlation (e.g., DLR model)

### Telemetry UI Architecture

**Decision:** New `TelemetryPlugin` with `UiSystems` set in `Update`; reads from simulation resources/components (no writes). Flight recorder is a `FlightRecorder` resource with circular buffer (configurable capacity, default 10 min at 10 Hz).

**Components:**
- `OrbitalElementsPanel`: reads `RocketPhysicsState` + `Gravity` for osculating elements
- `TrajectoryPredictionPanel`: runs lightweight patched-conics propagator in a background task (Bevy `TaskPool`), renders as `Gizmo` line strip
- `TerrainMapPanel`: orthographic projection of current body; samples `TerrainSource` for height colorization; draws ground track from recorder/predictor
- `FlightRecorderPanel`: playback controls + timeline scrubber. A dedicated fixed-tick replay snapshot stream records complete rocket state separately from the lower-rate telemetry ring buffer. On seek, replay restores the selected snapshot and derives presentation from that state; it never rewinds the live `SimulationTime` clock.

**Replay determinism:** The fixed-tick snapshot stream stores all authoritative state needed to resume a rocket deterministically: dynamics, mission, propulsion and stage state, control/guidance state, atmospheric and terrain-contact state, thermal/recovery state, and timestamp/entity identity. Replay pauses live simulation and restores a selected snapshot while bypassing physics integration. It does not set `SimulationTime` backward; resuming live flight restores the pre-replay clock and control state.

### Staging Recovery Architecture

**Decision:** Recovery logic lives in `guidance.rs` (new `RecoveryGuidance` mode) and `actuation.rs` (grid-fin mixer, leg deployment). Drone ship and catch tower are separate entities with their own `DroneShip`/`CatchTower` components and simple station-keeping controllers.

**Guidance modes:**
- `Boostback`: Lambert targeting to recovery zone
- `Entry`: Bank-angle guidance (like reentry but for stage)
- `Terminal`: Hover-slam or powered descent to pad/ship/tower

**Grid fins:** Four fins at 45° offsets; mixer converts desired body torque → fin deflections via pseudo-inverse. Effectiveness scaled by Mach/alpha tables.

**Drone ship:** Entity with an ECS `DroneShip` component embedding the domain
`recovery::DroneShip` state and `StationKeeper`. A fixed-tick adapter
integrates the bounded station-keeping thrust, then a guidance adapter writes
the existing landing target from `predict_position`; no transform participates
in either path. A post-integration deck-relative contact constraint evaluates
relative velocity and touchdown criteria, records the normal landing scorecard,
and prevents terrain contact from applying a second static-world constraint.

**Catch tower:** Entity with `CatchTower { arm_positions, capture_envelope }`; arms are kinematic; capture succeeds if stage hardpoints enter envelope with low relative velocity.

### Determinism Regression Architecture

**Decision:** Baseline fixtures stored as `ron` files in `tests/baselines/` (one per flight). CI runs `cargo test --test determinism_regression` which re-simulates each baseline in headless mode (no renderer) and compares state hashes.

**Implementation:**
- `DeterminismTestHarness`: headless Bevy app with only `FixedUpdate` schedule, fixed seed, no input/camera/UI
- State hashing: `xxHash64` of all `RocketPhysicsState` fields per tick
- Comparison: per-variable tolerance from `RegressionConfig` (TOML)
- Bisection: external `cargo-bisect-rust` wrapper script that checks out commits, runs the test, parses pass/fail
- Baseline update: `cargo test --test determinism_regression -- update` writes new baselines with metadata (git hash, date, author, PR link)

## Risks / Trade-offs

| Risk | Mitigation |
|------|------------|
| Terrain rendering performance at max LOD | Budget patches via streaming LRU; GPU instancing for repeated biome materials; frustum culling |
| DEM tile parsing latency blocking main thread | Parse in background task (`TaskPool`); streaming manager already async-ready |
| Guidance phase transition oscillations | Hysteresis on gate conditions; minimum dwell time per phase |
| Patched-conics prediction divergence over long horizons | Limit prediction to 2 SOI transitions; re-run predictor each frame |
| Flight recorder memory growth | Circular buffer with configurable max frames; optional file streaming for long flights |
| Grid-fin model fidelity vs. real data | Start with lookup tables from public Falcon 9 data; validate against known recovery footage |
| CI determinism flakiness (floating-point non-determinism) | Enforce strict `FixedUpdate` timestep; disable SIMD fast-math; pin Rust/Cargo versions; use `f64` throughout |
| Baseline bisection time | Run simulations in parallel across CI runners; cache baseline simulations per commit |
