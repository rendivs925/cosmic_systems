## Context

`update_rocket_controls` in `rocket_systems.rs` is a placeholder that directly sets `rocket.thrust` and mutates `rocket.angular_velocity`. The 6-DOF dynamics and propulsion/aero changes provide the accumulator pipeline that guidance/control/actuation must feed. No guidance exists. See proposal.md for motivation.

## Goals / Non-Goals

**Goals:**
- Distinct guidance/control/actuation/physics pipeline with explicit ordering.
- PID attitude controller producing bounded actuator commands.
- A working closed-loop ascent (gravity-turn) demonstrating the architecture.

**Non-Goals:**
- Full mission planning (transfers, landing guidance) — later phases.
- Advanced estimation/filtering (EKF) — future; state assumed observable for now.
- Changing physics integration itself (6-DOF change owns it).

## Decisions

**Decision: System-set ordering with named stages.**
Define a `RocketFlightSet` with `Guidance → Control → Actuation → Physics` and express ordering via `.chain()` or explicit `.after()` relationships (AGENTS.md sections 9 and 48). Guidance/control/actuation write a `RocketCommands` resource consumed by physics.
- Alternative: single monolithic autopilot system. Rejected: violates AGENTS.md section 18 separation.

**Decision: PID attitude controller first.**
A PID on the attitude error producing gimbal/RCS command, with clamping at the actuation layer. Anti-windup included (AGENTS.md and docs reference PID with anti-windup).
- Alternative: LQR/optimal control. Rejected: more machinery than needed initially; PID is testable and sufficient for gravity-turn.

**Decision: Guidance emits phase targets from a `MissionPhase` resource.**
`MissionPhase` (PreLaunch/Launch/Ascent/Orbit/...) drives which target guidance produces. This reuses the existing `RocketMissionState` concepts.
- Rationale: keeps guidance data-driven and testable without Bevy.

## Risks / Trade-offs

- [Controller tuning] → PID gains need tuning; unit tests assert convergence, not absolute perf; gains exposed as config.
- [Phase transitions] → Guidance target switching at phase boundaries must be smooth; test transition stability.
- [State observability assumption] → We assume full state (no estimator); documented as a future improvement.

## Migration Plan

1. Add `RocketCommands` resource and `RocketFlightSet` ordering.
2. Add guidance (phase-based targets; gravity-turn ascent).
3. Add control (PID) and actuation (limit application) systems.
4. Wire into the accumulator pipeline; remove placeholder `update_rocket_controls`.
5. Unit tests for PID convergence, actuator limits, gravity-turn stability.

## Open Questions

None.