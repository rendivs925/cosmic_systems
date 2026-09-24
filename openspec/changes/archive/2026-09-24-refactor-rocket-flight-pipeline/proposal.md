## Why

The rocket flight path is physically capable but its implementation spreads related calculations across broad Bevy systems, repeats pressure/velocity derivation, uses unvalidated body-name strings, and has implicit presentation transitions. Refactoring now preserves the authoritative `f64` simulation while making correctness boundaries explicit and future flight features safer to add.

## What Changes

- Introduce typed domain identifiers for celestial-body bindings and remove raw body-name matching from rocket flight interfaces.
- Encapsulate engine operating-point calculations so ambient pressure, thrust, mass flow, and gimbal torque share one authoritative model.
- Encapsulate atmosphere-relative flight conditions so guidance, aerodynamics, telemetry, propulsion, and entry physics consume consistent derived values.
- Narrow ECS system queries and divide the current broad flight adapter into cohesive feature-local modules without duplicating physical authority.
- Make fixed-flight schedule stages explicit: atmosphere/flight conditions, guidance/control/actuation, forces, integration, time advance, post-integration state, presentation capture, telemetry.
- Make camera mode transitions deterministic presentation state that blends from the rendered pose and never writes simulation state.
- Preserve simulation behavior except for corrections: guidance uses live dynamic pressure, propulsion uses ambient pressure for its configured sea-level/vacuum endpoints, post-integration consumers use the completed simulation epoch, and pausing gates the entire fixed flight pipeline.

## Capabilities

### New Capabilities
- `rocket-flight-architecture`: Typed bindings and cohesive flight-domain value types that define ownership between the domain, ECS systems, and presentation.

### Modified Capabilities
- `rocket-dynamics`: Fixed-step epoch, post-integration state, pause behavior, and render capture become explicit pipeline requirements.
- `rocket-propulsion`: Ambient pressure selects engine operating points while preserving pressure-independent propellant mass flow and matching gimbal torque.
- `rocket-guidance-control`: Guidance uses the authoritative atmosphere-relative dynamic pressure for flight-phase constraints.
- `rocket-mode`: Camera mode transitions become continuous presentation-only transitions.

## Impact

- Affects rocket components, spawning/config adapters, fixed schedule registration, camera systems, telemetry, debug rendering, propulsion and atmosphere domain services.
- `RocketPlanetBinding` and engine-operation interfaces are internal API changes; all users will migrate in one change.
- No new dependencies, no alternate coordinate system, no second physics or atmosphere implementation.
