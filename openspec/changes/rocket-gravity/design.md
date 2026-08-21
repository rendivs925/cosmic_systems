## Context

The solar system already models planets analytically via `calculate_planet_position` (no n-body integration), and each `Planet` stores `mass_kg` (f64). The craft physics uses a hardcoded `GRAVITY = 0.29` (speculative ZPE model — out of scope). The `reference-frames` change provides planet-centered frame and meter-scale conversions. This change adds the gravity force/acceleration layer. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Single `gravitational_acceleration(body_mass_kg, position_m, body_position_m) -> DVec3` pure function.
- Consume existing `Planet.mass_kg` and reference-frame output.
- Testable without Bevy.

**Non-Goals:**
- N-body multi-body superposition beyond the dominant-body model for the initial implementation (foundation only; extensible later).
- Modifying the craft ZPE physics.
- Adding gravity to the solar-system planets themselves (they are placed analytically).

## Decisions

**Decision: Dominant-body gravity model first.**
Compute gravity from the nearest/selected dominant celestial body (the rocket's frame parent). Multi-body summation can be added later behind the same function signature.
- Alternative: full n-body for every body each tick. Rejected: overkill for a vehicle in the SOI of one body; AGENTS.md section 41 (measure before optimizing).

**Decision: Pure domain function + thin Bevy system.**
`domain/services/gravity.rs` holds the math; a Bevy system feeds `PlanetComponent.domain_planet.mass_kg` and the reference frame into it. This keeps physics in the domain layer (AGENTS.md section 3).
- Alternative: gravity logic inline in ECS systems. Rejected: not testable without Bevy, violates domain/infrastructure separation.

**Decision: Preserve the craft `GRAVITY` constant.**
Only the rocket path uses real gravity. The UFO's speculative model is intentionally untouched to avoid altering existing behavior.

## Risks / Trade-offs

- [Dominant-body selection ambiguity] → Use the reference-frame parent body; document selection rules and revisit for multi-body later.
- [Performance of f64 gravity per tick] → One body per vehicle is trivial; measure before optimizing.
- [SOI transitions] → For a single dominant body the transition is a frame-parent change; documented as future multi-body work.

## Migration Plan

1. Add `domain/services/gravity.rs` with the pure function and unit tests.
2. Add the Bevy system consuming `PlanetComponent` + reference frames.
3. Keep the craft ZPE gravity constant unchanged.

## Open Questions

None.