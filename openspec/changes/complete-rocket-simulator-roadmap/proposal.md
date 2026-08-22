## Why

The rocket flight simulator now has a complete physics pipeline (gravity, 6-DOF, propulsion, atmosphere/aero, guidance/control/actuation, terrain data/LOD/collision/streaming), but it is not yet a complete simulator. The remaining gaps are: GPU terrain rendering (planets still show flat patches), no descent/reentry/landing guidance, no real planetary DEM data, no atmospheric entry physics (heating/ablation/parachutes), minimal telemetry UI, no multi-vehicle/staging-recovery logic, and no formal determinism regression suite. Completing these yields a world-class, physically coherent, visually convincing, deterministic, scalable, testable simulator.

## What Changes

- **Terrain GPU rendering**: Spawn cube-sphere LOD meshes from the streaming manager; wire PBR materials; add planetary surface shaders.
- **Descent & reentry guidance**: Add deorbit burn targeting, reentry corridor management, terminal guidance for powered/unpowered landing (RTLS/drone ship/lunar).
- **Real DEM data**: Implement a `DemTerrainSource` that loads NASA SRTM / LRO / MOLA heightmaps behind the existing `TerrainSource` interface.
- **Planetary atmospheric entry physics**: Heating model (stagnation-point convective/radiative), ablation, plasma blackout comms, parachute deployment logic, supersonic retro-propulsion.
- **Telemetry/debug UI**: Orbital elements panel, trajectory prediction (patched conics), terrain map overlay, flight profile recorder/replay.
- **Multi-vehicle / staging recovery**: Boostback burn guidance, grid-fin control, landing-leg deployment, drone-ship station-keeping, catch-tower logic.
- **Determinism regression suite**: Saved baseline trajectories, CI gate that re-sims and compares state hashes; bisection for physics changes.

## Capabilities

### New Capabilities

- `terrain-rendering`: GPU mesh spawning, materials, shaders for cube-sphere LOD patches from the streaming system
- `descent-guidance`: Deorbit targeting, reentry corridor, terminal landing guidance (powered/unpowered)
- `dem-terrain-source`: Real planetary heightmap loading (SRTM, LRO, MOLA) via `TerrainSource` interface
- `entry-physics`: Aerothermal heating, ablation, plasma blackout, parachutes, supersonic retro-propulsion
- `telemetry-ui`: Orbital panel, trajectory prediction, terrain map, flight recorder/replay
- `staging-recovery`: Boostback, grid fins, landing legs, drone ship, catch tower
- `determinism-regression`: Baseline trajectories, CI comparison gate, bisection tooling

### Modified Capabilities

None — all are new capabilities extending the existing pipeline.

## Impact

- **Rendering**: `terrain-rendering` adds mesh spawning in `terrain_patch_manager.rs` / new `terrain_render.rs`, material/shader assets, and integration with Bevy's PBR pipeline.
- **Guidance**: `descent-guidance` extends `guidance.rs` with new phases and targeting algorithms; `staging-recovery` adds new guidance modes.
- **Terrain data**: `dem-terrain-source` adds a new `TerrainSource` implementation; no renderer changes (interface unchanged).
- **Physics**: `entry-physics` adds new force/torque systems in the fixed-update pipeline (heating, ablation mass loss, parachute drag, retro-propulsion).
- **UI**: `telemetry-ui` adds new Bevy UI systems, panels, and a flight recorder resource.
- **Testing/CI**: `determinism-regression` adds test fixtures, a comparison runner, and CI configuration.
- **Dependencies**: Potential new crates for heightmap parsing (e.g., `geotiff`, `hgt-reader`), but prefer minimal/std where possible.