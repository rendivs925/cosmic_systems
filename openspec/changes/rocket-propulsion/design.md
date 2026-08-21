## Context

`domain/entities/rocket.rs` already models a Falcon-9 `Rocket` with `dry_mass_kg`, `fuel_mass_kg`, `max_thrust_kn`, `isp_sea_level`, `isp_vacuum`, `gimbal_range_deg`, and `mass_flow_rate_kg_s(throttle)`. `rocket_systems.rs` currently hardcodes `100 kg/s` burn and `thrust.y = 100000`. The `rocket-6dof-dynamics` change provides the force/torque accumulator. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Full vehicle definition (stages, engines, positions, ISP, gimbal).
- Physically correct thrust, mass flow, propellant depletion, staging, gimbal torque.
- Feed the 6-DOF pipeline; never write the transform.

**Non-Goals:**
- Atmospheric/aero effects on thrust (separate change).
- Guidance/control commanding throttle (separate change — this change exposes the command interface).
- Engine plume/visual effects.

## Decisions

**Decision: Extend `Rocket` entity to a staged vehicle definition.**
Add `stages: Vec<RocketStage>`, each with engines (`Vec<RocketEngine>`: position, thrust vector, ISP, gimbal range, state), dry mass, propellant. Keep `falcon9()` producing a two-stage Falcon 9 as the default vehicle.
- Alternative: flat single-engine model. Rejected: cannot express staging/gimbal (AGENTS.md section 17).

**Decision: ISP selection by altitude threshold.**
Blend or switch sea-level/vacuum ISP based on atmospheric density/altitude from the atmosphere capability (added in a later change; for now a density-independent threshold on altitude).
- Alternative: always sea-level. Rejected: wrong in vacuum, contradicts existing entity which already carries both ISP values.

**Decision: Propulsion systems write accumulator, not transform.**
`propulsion_thrust`, `propulsion_consumption`, `propulsion_staging`, `propulsion_gimbal` systems add to the 6-DOF accumulator. No transform writes.
- Rationale: physics authority (AGENTS.md section 17, "Do not fake flight by manipulating Transform").

## Risks / Trade-offs

- [ISP transition discontinuity] → Document a smooth blend once atmosphere exists; acceptable for now.
- [Staging timing] → Stage separation on exhaustion + explicit command; edge cases (zero-mass stage) guarded.
- [Gimbal torque sign conventions] → Unit tests assert torque direction relative to thrust offset; document axis conventions.

## Migration Plan

1. Extend `Rocket` entity with stages/engines (keep `falcon9()`).
2. Add propulsion domain functions + unit tests (rocket equation, mass loss, staging).
3. Add propulsion systems wired into the 6-DOF accumulator.
4. Remove hardcoded thrust/fuel logic.
5. Keep `mass_flow_rate_kg_s` as the single mass-flow authority.

## Open Questions

None.