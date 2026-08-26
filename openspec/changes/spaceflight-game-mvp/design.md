## Context

The repository already has the simulation half of the MVP: f64 fixed-step rocket state, data-driven multi-stage definitions, propulsion/staging, terrain collision, entry/recovery systems, cameras, HUD telemetry, trajectory prediction, and Earth/Moon presentation. It lacks player ownership of commands, an in-game assembly/missions loop, and persistence. See `proposal.md` and the change specs for required behavior.

## Goals / Non-Goals

**Goals:**
- Ship one coherent desktop vertical slice from menu to debrief on Earth.
- Reuse the existing vehicle catalog and authoritative fixed-update pipeline.
- Make direct control approachable without hiding the real flight consequences.
- Keep mission evaluation deterministic and testable without launching Bevy rendering.

**Non-Goals:**
- Moon launch/landing, interplanetary transfers, multiplayer, modding, a dynamic economy, real-time weather, or exact mid-flight save/resume.
- A general-purpose CAD editor or arbitrary procedural vehicle geometry.
- Replacing existing gravity, dynamics, atmosphere, terrain, camera, telemetry, or application-mode infrastructure.

## Decisions

### Bounded Build Catalog

**Decision:** Extend the existing RON-backed `RocketCatalog` with a small `PartCatalog` and represent a player draft as an ordered stack of part identifiers plus user metadata. The draft validates into the existing `VehicleDef` before it can launch; `VehicleDef` remains the sole flight configuration authority.

**Rationale:** The project already validates masses, stages, engines, and limits in `rocket_config.rs`. A separate runtime rocket model would duplicate physics configuration and introduce mismatches.

**Alternatives considered:** Editing raw RON from the UI exposes implementation details and unsafe configurations. A freeform attachment graph is premature for a stack-rocket MVP and would require a new structural/drag/inertia solver.

### Explicit Command Arbitration

**Decision:** Add a small per-rocket command-ownership component/resource upstream of the existing `RocketCommands` pipeline. Input, assists, and autopilot write proposals; one ordered arbitration system selects owner per axis; the existing control, actuation, and fixed-step physics consume only the resolved command.

**Rationale:** This makes direct, assisted, and autopilot flight compatible without letting UI or guidance mutate `RocketPhysicsState`.

**Alternatives considered:** Disabling autopilot systems in direct mode would make partial assists difficult. Letting each producer write `RocketCommands` directly relies on accidental schedule order.

### One-Node Planning Model

**Decision:** MVP supports one maneuver node for one active vehicle and a patched-conic, f64 prediction. Map interactions create planned delta-v; execution remains a physical burn through player controls or maneuver assist.

**Rationale:** It teaches orbit shaping and supports the four missions without a maneuver-planner graph, n-body propagator, or background mission control system.

**Alternatives considered:** A full multi-node planner is valuable later but expands UI, prediction, and failure cases disproportionately. Teleporting to the predicted post-burn orbit would violate physics authority.

### Data-Driven Mission State

**Decision:** Define four mission records with prerequisites, allowed part identifiers, objective predicates, and rewards. Evaluate predicates from authoritative components/events in a pure domain service; Bevy systems adapt the result into UI and progression state.

**Rationale:** This creates deterministic tests and allows future missions without hard-coded UI branches.

**Alternatives considered:** Scripted Bevy systems for each mission would duplicate event logic and make mission results difficult to validate headlessly.

### Session Boundary and Persistence

**Decision:** Persist a versioned player profile containing settings, unlocks, mission history, and player vehicle drafts in a local RON file with atomic write/backup behavior. Active flights end when the process ends; flight recording remains a diagnostic tool rather than a save game.

**Rationale:** This is a reliable MVP boundary. Exact restoration would require serializing every authoritative world resource, scheduled event, asset state, and terrain cache.

**Alternatives considered:** Browser/local-storage-only persistence is not appropriate for the native desktop target. In-flight checkpoints would imply an unvalidated simulation restore contract.

### Presentation State Is Separate From Flight State

**Decision:** Add rocket-mode game-flow states for menu, assembly, briefing, flight, pause, and debrief. They orchestrate existing plugins and UI only; mission phase and physical state remain entity/domain data.

**Rationale:** The user-facing lifecycle is global, while flight state is per vehicle. This preserves ECS ownership and mode isolation.

## Risks / Trade-offs

- [Existing vehicle meshes are Falcon-specific] -> Generate the MVP stack preview and in-flight mesh from the validated definition before expanding part variety.
- [Manual control exposes unstable launch outcomes] -> Provide discoverable direct/assisted/autopilot modes, safe default sensitivity, restart, and telemetry explanations.
- [Prediction diverges from powered flight] -> Label plans as predictions, recompute after every fixed-state change, and never use them as physical authority.
- [Save-schema changes] -> Version profiles, validate each field, write backups, and test migration/default initialization.
- [UI scope dominates physics work] -> Deliver the vertical slice in dependency order and keep styling/audio polish behind playable flow and input.
- [Time warp causes missed controls or unstable powered integration] -> Gate warp by flight context and retain fixed-step substepping constraints.

## Migration Plan

1. Keep `cargo run` and `cargo run -- craft` unchanged; retain `cargo run -- rocket --vehicle <key>` as a developer quick-launch path.
2. Introduce the game flow behind rocket mode with a default profile and a single prebuilt tutorial vehicle.
3. Add direct command arbitration, then mission/objective evaluation, before exposing assembly and unlocks.
4. Add persistence with a versioned profile and corrupted-save fallback after the data model is stable.
5. Validate each milestone headlessly plus desktop rocket-mode smoke runs; a failed game-flow plugin can be isolated without affecting non-rocket modes.
