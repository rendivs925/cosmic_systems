## Context

`rocket_systems.rs` currently does `a = thrust/mass`, `position += velocity*dt`, and the invalid `orientation * Quat::from_vec4(angular_velocity.extend(0.0)) * dt`. `RocketComponent` already has `position`, `velocity`, `orientation`, `angular_velocity`, `mass`, `dry_mass_kg`, `fuel_mass`. The `reference-frames` and `gravity` changes provide meters/frames and gravity. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Correct 6-DOF integration (semi-implicit Euler to start; evaluable later).
- Cohesive physical state with inertia tensor and center of mass.
- Clean force/torque accumulation pipeline.

**Non-Goals:**
- Propulsion specifics (throttle/ISP/staging/gimbal) — separate change.
- Aerodynamic forces — separate change.
- Guidance/control — separate change.

## Decisions

**Decision: Semi-implicit (symplectic) Euler integration to start.**
`v += a*dt; pos += v*dt` and `ω += α*dt; q = normalize(q * from_scaled_axis(ω*dt))`. Energy stability for orbital work; simple and deterministic. Higher-order integrators (RK4/leapfrog) are a documented future option requiring regression evidence.
- Alternative: RK4 immediately. Rejected: no profiling evidence yet (AGENTS.md section 12); semi-implicit Euler is stable for the initial flight envelope.

**Decision: `RocketPhysicalState` as a cohesive component.**
Extend `RocketComponent` (keeping its name/fields for compatibility) with f64 position/velocity from `reference-frames`, plus inertia tensor, center of mass, and angular acceleration.
- Alternative: many tiny components. Rejected: cohesive vehicle state is more practical here; AGENTS.md prefers cohesion where reuse is real.

**Decision: Ordered force/torque accumulation pipeline.**
Systems: `accumulate_forces` → `accumulate_torques` → `integrate_6dof` → `sync_render_transform`, chained explicitly (AGENTS.md sections 9 and 48). Physics writes state; only the sync system writes `Transform`.

## Risks / Trade-offs

- [Energy drift over long orbits] → Semi-implicit Euler conserves energy well for near-circular orbits; if long-duration drift appears, evaluate higher-order integrators with regression tests before replacing.
- [Inertia tensor realism] → Start with geometric approximations (cylindrical stages); refine with documented models later.
- [Quaternion drift] → Normalize every integration step; unit tests assert validity.

## Migration Plan

1. Add `domain/services/rocket_dynamics.rs` with integration math + unit tests.
2. Extend `RocketComponent` state; keep old fields as compatible facade.
3. Replace `update_rocket_physics` with the ordered pipeline.
4. Remove the old invalid quaternion step.

## Open Questions

None.