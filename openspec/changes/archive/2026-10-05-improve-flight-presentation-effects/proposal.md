## Why

Rocket flight has authoritative physics and scientific reference validation, and
`premium-rocket-flight-presentation` already added engine plumes, ground
interaction, pad illumination, audio controls, and render-origin-stable effect
children. Three presentation gaps remain uncovered: effect quality cannot be
reduced or disabled, atmospheric shock/heating state is computed but not shown,
and lifecycle transitions such as staging and recovery have no transient visual
feedback.

## What Changes

- Add explicit effect-quality levels (full, reduced, disabled) to the existing
  Rocket presentation-quality resource without altering physics or simulation
  time.
- Add bounded atmospheric-flight visual feedback (shock and heating) derived
  from the existing smoothed presentation parameters.
- Add bounded, one-shot lifecycle feedback for staging, fairing separation,
  touchdown, splashdown, and crash, derived from existing authoritative events.
- Add presentation/simulation isolation and quality-gating regression tests.

## Capabilities

### New Capabilities
- `flight-presentation-effects`: State-driven visual effects and animation for
  rocket flight that consume authoritative simulation state without modifying it.

### Modified Capabilities
- `rocket-mode`: Rocket-mode presentation exposes coherent visual feedback for
  active flight and lifecycle transitions while preserving fixed simulation
  authority.

## Impact

- Presentation: rocket effect quality resource, atmospheric shock/heating
  children, transient lifecycle effects, and HUD feedback.
- Infrastructure: rocket-mode plugin composition and effect asset ownership.
- Validation: pure presentation-state, quality-isolation, and lifecycle tests
  plus bounded rocket startup checks.
- Simulation: no new physics model, force, coordinate system, or runtime
  scientific authority.
