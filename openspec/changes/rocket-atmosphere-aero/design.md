## Context

No atmosphere or aerodynamic model exists. Propulsion uses a planned altitude threshold for ISP; 6-DOF provides the accumulator and frame; gravity provides the gravitational force. Rocket geometry (`diameter_m`, `height_m`) exists in the `Rocket` entity for reference area and center of pressure. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- `AtmosphereSource` trait with an Earth model; extensible to Mars/Venus and a vacuum (moon) model.
- Aerodynamic force/torque pipeline consuming atmosphere.
- Max Q tracking for telemetry.

**Non-Goals:**
- Reentry thermal heating (future).
- Real fluid dynamics or coefficient tables beyond simple analytic models.
- Changing propulsion ISP logic beyond making it consume atmosphere density.

## Decisions

**Decision: `AtmosphereSource` trait with per-planet implementations.**
`trait AtmosphereSource { fn properties(altitude_m) -> AtmosphereProperties }` with `EarthAtmosphere` (exponential/ISA-style reference) and `VacuumAtmosphere` for bodies without air. Planets carry an `AtmosphereSource` reference.
- Alternative: a single hardcoded Earth function. Rejected: AGENTS.md section 19 requires different planets → different atmospheres.
- Alternative: external crate (e.g., `atmosphere` crate). Rejected: AGENTS.md section 59 — check before adding deps; a small internal model suffices.

**Decision: Analytic aerodynamic coefficients.**
Start with constant/reference-area-based Cd and a simple Cl(alpha) model, with center of pressure from geometry. Coefficient tables can replace the functions behind the same interface later.
- Alternative: tabulated aerodynamic data. Rejected: no data source yet; analytic first, measured later.

**Decision: Dynamic pressure and Mach as derived values.**
Computed from atmosphere + state in the aero system; exposed to telemetry. Not stored as separate authoritative state (AGENTS.md section 63 — derive, don't duplicate).

## Risks / Trade-offs

- [Coefficient realism] → Analytic coefficients are approximations; document limits, refine behind the interface with tests.
- [ISP-atmosphere coupling] → Propulsion's ISP selection should consume atmosphere density; coordinate ordering with the aero/atmosphere systems.
- [Center of pressure accuracy] → Start from geometry; refine with documented model.

## Migration Plan

1. Add `AtmosphereSource` trait + `EarthAtmosphere` + `VacuumAtmosphere` + tests.
2. Add aerodynamics domain functions + tests (q, Mach, drag, lift, AoA, torque, Max Q).
3. Add atmosphere/aero systems into the 6-DOF accumulator pipeline.
4. Update propulsion ISP selection to consume atmosphere density.

## Open Questions

None.