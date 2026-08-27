## Context

The current rocket mode has a correct high-level ECS pipeline and an authoritative `f64` `RocketDynamicsState`, but `rocket_systems.rs` concentrates unrelated adapters and repeats environment-dependent calculations. Rocket binding uses `String`; propulsion accepts a density value despite back pressure being the relevant input; guidance has only recently received live dynamic pressure. See `proposal.md` for motivation.

## Goals / Non-Goals

**Goals:**
- Preserve one authority for gravity, atmosphere, propulsion, terrain contact, dynamics, and rendering conversion.
- Make recurrent physical values cohesive, typed, and testable without Bevy.
- Split Bevy adapters by feature while retaining explicit fixed-stage ordering.
- Preserve the `f64` inertial simulation and `f32` render boundary.

**Non-Goals:**
- No manager/service-object layer over ECS queries.
- No closed enum for celestial bodies; the catalog supports configured bodies.
- No trait where a single value type or inherent method is sufficient.
- No new physics model beyond the identified pressure, epoch, and guidance corrections.
- No external API or asset-format breakage outside this repository.

## Decisions

### Typed body identity

Introduce a small validated `CelestialBodyId` domain value object and migrate rocket binding, lookups, launch configuration, and telemetry context to it. It remains string-backed to support configured planets while preventing raw-string mixing.

Alternatives considered:
- `enum Planet`: rejected because it blocks configured/non-Solar-System bodies.
- Keep `String`: rejected because lookup and validation are repeated and typo-prone.

### Cohesive operating and flight-condition values

Introduce `EngineOperatingPoint` for the pressure-selected Isp, calibrated mass flow, thrust, and gimbal-force magnitude of one engine. Introduce `FlightConditions` for sampled atmosphere plus atmosphere-relative velocity, speed, Mach, and dynamic pressure. Both are pure domain values with constructors and invariants; ECS only refreshes and stores them.

`AtmosphereSource` remains the existing multiple-implementation trait. No general `PhysicsService` trait is introduced because it would only wrap a single implementation.

### Feature-local adapters

Split `rocket_systems.rs` into cohesive modules for gravity/orbit, atmosphere/flight conditions, guidance/control, propulsion, dynamics/contact, and presentation. Systems remain functions registered by `RocketModePlugin`; shared query data stays local to the feature that owns it.

### Explicit fixed pipeline

Order stages as: flight conditions -> guidance -> control -> actuation -> gravity and external forces -> propulsion -> integration -> simulation epoch -> orbital/contact -> render capture -> telemetry. Apply one pause run condition to every fixed stage. Orbital elements intentionally represent the completed prior state when guidance reads them on the next tick.

### Presentation transitions

`RocketCameraController` owns a start-pose snapshot and transition state through inherent methods. Camera systems compute destination poses from interpolated render dynamics and never read/write authoritative dynamics mutably.

## Risks / Trade-offs

- [Internal type migration touches many queries] -> Migrate one bounded identifier at a time and compile after each feature module.
- [Splitting modules can accidentally reorder systems] -> Retain `RocketSet` ordering and add schedule-level tests.
- [Cached flight conditions can become stale] -> Refresh exactly once at the first fixed stage and use the completed state only in post-integration consumers.
- [Camera behavior cannot be fully verified headlessly] -> Add transition-state unit tests and manually validate rocket mode.
- [Engine configuration semantics vary by vehicle] -> Document existing catalog thrust as sea-level and validate the two endpoint cases.

## Migration Plan

1. Land and test the current camera/time/pressure correctness fixes as the baseline.
2. Add pure typed identifiers and operating/flight-condition value objects with unit tests.
3. Migrate ECS components and systems feature by feature, preserving schedule stages and running regression tests after each migration.
4. Split adapter modules without changing public plugin composition.
5. Run full Rust validation and manually exercise all application modes; revert the refactor commit if regressions emerge because no persisted format changes are planned.
