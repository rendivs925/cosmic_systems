## Why

Rocket flight has authoritative physics and scientific reference validation, but
its presentation does not yet communicate propulsion, atmospheric flight,
staging, and vehicle motion with a polished, coherent visual language.

## What Changes

- Add state-driven rocket presentation effects for engine operation, atmospheric
  flight, staging, and recovery without altering physical state or forces.
- Improve visual animation and camera-facing presentation from existing fixed
  simulation snapshots and lifecycle events.
- Provide explicit quality controls and graceful degradation for presentation
  effects so they do not compromise simulation cadence or mode startup.
- Add debug-friendly validation of effect lifecycle, render-origin conversion,
  and presentation/simulation isolation.

## Capabilities

### New Capabilities
- `flight-presentation-effects`: State-driven visual effects and animation for
  rocket flight that consume authoritative simulation state without modifying it.

### Modified Capabilities
- `rocket-mode`: Rocket-mode presentation exposes coherent visual feedback for
  active flight and lifecycle transitions while preserving fixed simulation
  authority.

## Impact

- Presentation: rocket presentation, camera, render interpolation, materials,
  lights, particle or mesh effects, and HUD feedback.
- Infrastructure: rocket-mode plugin composition and effect asset ownership.
- Validation: pure presentation-state and lifecycle tests plus bounded rocket
  startup checks.
- Simulation: no new physics model, force, coordinate system, or runtime
  scientific authority.
