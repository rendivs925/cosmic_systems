## Why

Guidance, control, and physics must remain separate (AGENTS.md section 18): guidance decides where the rocket should go, control commands the attitude/actuators, and physics determines what actually happens. Today there is a placeholder `update_rocket_controls` that directly manipulates `thrust` and `angular_velocity` — fusing guidance/control into a single hardcoded system. A world-class simulator needs a closed-loop flight architecture: mission → guidance → control → actuators → physics → state → guidance.

## What Changes

- Introduce separate guidance, control, actuator, and physics concepts as distinct systems/resources.
- Add guidance: profile/target generation (e.g., gravity-turn ascent, orbit insertion targeting).
- Add control: an attitude controller (PID) that commands actuator outputs from guidance targets and current state.
- Add actuation: applies physical limits (gimbal range, RCS, throttle slew) before physics.
- Keep physics as the sole authority over the rocket state; guidance/control/actuation never modify the transform or state directly.
- Add unit tests: PID convergence, gravity-turn trajectory stability, actuator limiting.

## Capabilities

### New Capabilities

- `rocket-guidance-control`: Separation of guidance (targets), control (attitude commands), actuation (physical limits), and physics, with closed-loop flight for launch/ascent.

### Modified Capabilities

<!-- None - no existing openspec/specs exist yet. -->