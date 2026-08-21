## Context

`src/main.rs` currently selects modes by `args.contains(&"gyro")` / `args.contains(&"craft")` and wires systems through imperative functions `setup_gyro_mode`, `setup_craft_systems`, and `setup_solar_system_mode`. Craft mode already demonstrates the correct composition pattern: it runs `setup_solar_system_mode` (sharing the world) and disables the solar camera via `SolarCameraEnabled(false)`, then adds craft systems and a `CraftCameraTag` camera. No Bevy `Plugin` structs exist anywhere in the codebase; all wiring is procedural. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Introduce a small set of composable plugins that preserve current wiring exactly.
- Add a `rocket` mode that shares the solar world and owns its camera (mirroring craft).
- Make mode selection explicit and robust.

**Non-Goals:**
- Implementing rocket physics, camera, or telemetry (separate changes).
- Refactoring the WASM entry (`wasm.rs`) or gyro internals beyond moving their registration.
- Changing any shared solar-system or craft behavior.

## Decisions

**Decision: Adopt explicit `Mode` enum + plugin composition.**
A `Mode` enum (`Solar`, `Craft`, `Rocket`, `Gyro`) parsed in `main.rs` selects plugins. This replaces `args.contains` and gives a single extension point for new modes.
- Alternative: keep the `if rocket {}` branching everywhere. Rejected: spreads mode logic across shared systems and invites accidental cross-mode coupling, contrary to AGENTS.md section 13.
- Rationale: matches the project's goal of one simulator with isolated mode-specific behavior (AGENTS.md sections 5 and 66).

**Decision: Extract existing setup bodies into plugins verbatim first.**
`SharedSimulationPlugin`, `SolarSystemModePlugin`, `CraftModePlugin`, `GyroModePlugin`, `RocketModePlugin` are thin wrappers that call the same system-registration code currently in `main.rs`. No system behavior changes in this change.
- Alternative: a large `SimulationPlugin`. Rejected: violates AGENTS.md section 35 (no god plugins).
- Rationale: minimizes risk; behavior is preserved because the same systems are registered in the same order.

**Decision: Rocket mode initially registers solar world + a rocket marker only.**
`RocketModePlugin` composes `SharedSimulationPlugin` + `SolarSystemModePlugin` (world present, solar camera disabled like craft) and leaves rocket systems for subsequent changes. This makes mode isolation real before any rocket code lands.
- Rationale: keeps this change reviewable and independently shippable.

## Risks / Trade-offs

- [Craft mode regression] → Run `cargo run -- craft` before and after; the refactor only moves registration code.
- [Accidental shared-system coupling] → Rocket systems are only registered in `RocketModePlugin`; no `if rocket {}` guards are added to shared systems.
- [Plugin ordering differences] → Preserve the exact `add_systems` order and resource insertion order from the current `setup_*` functions.

## Migration Plan

1. Add `Mode` enum and parser; keep old behavior as the fallback.
2. Introduce plugin structs wrapping existing registration code.
3. Switch `main()` to plugin composition.
4. Verify `cargo run`, `cargo run -- craft`, `cargo run -- rocket`, `cargo run -- gyro`.

## Open Questions

None.
