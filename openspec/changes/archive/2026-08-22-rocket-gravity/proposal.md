## Why

There is no gravity system for vehicles anywhere in the codebase. The only "gravity" is a hardcoded fictional `GRAVITY: f32 = 0.29` in the ZPE craft physics and `g0` used only for mass-flow calculation. The solar-system `Planet` entity already stores real f64 masses (`Earth = 5.97237e24 kg`), but nothing consumes them. Rocket flight and orbital mechanics are impossible without authoritative `F = GMm/r²` gravity that reuses the existing planet data and frame conversions.

## What Changes

- Add a gravity calculation for vehicles that consumes existing `Planet.mass_kg` (f64) and the reference-frame module.
- Compute gravitational acceleration/force from the body that is authoritative (one gravity implementation, reused by all consumers — AGENTS.md sections 16 and 50).
- Provide gravity for the rocket in both the current celestial body of influence and (as a foundation) the solar-system context.
- Replace the hardcoded craft `GRAVITY` constant only where it feeds rocket physics; leave the speculative ZPE craft model untouched.
- Add unit tests for inverse-square behavior, surface acceleration, and orbital period consistency.

## Capabilities

### New Capabilities

- `gravity`: Authoritative planetary gravity from real planet masses, applied to vehicles through the shared reference-frame module.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->