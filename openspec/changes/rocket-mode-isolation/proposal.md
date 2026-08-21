## Why

The project must evolve into a single coherent rocket flight simulator, but today it exposes only `cargo run` (solar system) and `cargo run -- craft` (UFO). Rocket flight needs an explicit, isolated application mode so its physics, camera, and telemetry can be composed without disturbing the existing solar-system and UFO behavior. The current CLI parsing (`args.contains("craft")`) is brittle string matching, and mode wiring lives in imperative `setup_*` functions in `main.rs` rather than composable Bevy plugins.

## What Changes

- Add a `rocket` application mode launched via `cargo run -- rocket`.
- Preserve `cargo run` (solar system) and `cargo run -- craft` (UFO) with identical behavior.
- Refactor mode wiring from imperative `setup_*` functions into explicit Bevy `Plugin` structs (`SharedSimulationPlugin`, `SolarSystemModePlugin`, `CraftModePlugin`, `RocketModePlugin`, `GyroModePlugin`).
- Replace brittle `args.contains(...)` matching with an explicit `Mode` enum parser in `main.rs` (or `application/modes.rs`).
- Register rocket-only systems (physics, controls, camera, telemetry) exclusively in `RocketModePlugin` so they never run in solar or craft modes.
- Keep the existing UFO mode behavior intact; the craft refactor into `CraftModePlugin` is a pure re-organization, not a behavior change.

## Capabilities

### New Capabilities

- `rocket-mode`: The `cargo run -- rocket` application mode, how it is selected, how it composes shared solar-system infrastructure, and which systems it activates.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet; all rocket capabilities are new. -->

## Impact

- `src/main.rs` — mode selection logic and plugin composition.
- `src/application/*.rs` — extract `setup_solar_system_mode`, `setup_craft_systems`, `setup_gyro_mode` bodies into plugins.
- New plugin modules under `src/infrastructure/` (e.g., `infrastructure/plugins/`).
- No changes to shared physics, rendering, or solar-system modules.
- No new external dependencies.
