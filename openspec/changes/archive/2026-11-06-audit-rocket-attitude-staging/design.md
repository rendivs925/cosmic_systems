## Context

The rocket pipeline already separates simulation from presentation:
`RocketDynamicsState` is authoritative, `capture_render_state` snapshots it in
`FixedUpdate`, and `interpolate_render_transform` projects the interpolated state
into a Bevy `Transform` in `Update`. The body frame is documented in
`rocket_dynamics.rs`: position is the geometric centre of the attached assembly,
`+Y` is longitudinal, and orientation is body→world.

The audit found the seam in two places where the projection assumed something
about the body that was not true.

## Goals / Non-Goals

**Goals:**

- Make the rendered transform follow every authoritative body, including debris.
- Make the rendered mesh frame equal the physical body frame at all times,
  especially immediately after staging.
- Pin the distinction between "normal interpolation" and the deliberate
  surface-constrained pin.
- Prove separation does not alter attitude.

**Non-Goals:**

- Change the integrator, the force model, mass properties, guidance, or the
  separation impulse.
- Rotate or offset the render transform to hide a physical mismatch.
- Model fairing halves as true half shells. They remain cosmetic solids; only
  their frame and independent motion are in scope.

## Decisions

### Decision: a missing mission state means debris, not "do not render"

`RocketMissionState` is a mission-vehicle component. Jettisoned hardware is
deliberately not a mission vehicle (`PrimaryVehicle` excludes it), so requiring
the component in the snapshot query silently excluded it. The mission read is now
`Option<&RocketMissionState>`; absent means the normal interpolation path. The
surface-constrained pin still requires an actual mission state, so a landed
vehicle behaves exactly as before.

### Decision: the mesh parameter is an origin shift, not a new builder

`build_rocket_mesh` builds bottom-up from the declared envelope. Rather than
duplicating the cylinder/cone builder for the serial stage, it takes one
`body_origin_offset_y_m` shift. The full stack passes `0` because the envelope
centre is its physical origin. A serial stage passes `+envelope / 2`, which moves
the stage cylinder from the envelope centre onto the origin and leaves the
retained upper envelope above it. The offset is applied once to vertex positions,
so normals, UVs and indices are untouched.

This is the minimal change that keeps one mesh builder and one physical frame.

### Decision: measurement controls live in the existing capture modules

A capture has no keyboard, so neither the body-axis gizmos (F1/F6) nor a launch
order (Space) can be produced. The two controls are opt-in environment switches
in the modules that already own capture measurement:

- `COSMIC_SYSTEMS_CAPTURE_BODY_AXES` initialises the existing read-only gizmo
  config with only `show_frames` enabled.
- `COSMIC_SYSTEMS_CAPTURE_WARMUP_STEPS` sets the pre-launch mission to `Launch`
  and runs that many production `FixedUpdate` steps once, before the first
  frame.

The warm start runs the real schedule — guidance, propulsion, staging, gravity,
contact — so the captured pose is reachable by a piloted flight. It writes no
transform and changes nothing when unset.

## Risks / Trade-offs

- **Warm-start cost.** A long warm start runs thousands of fixed ticks in one
  frame. It is bounded to 200 s of simulated time and documented as a
  measurement-only path; a normal run never enters it.
- **Debris interpolation.** Debris now interpolates like any other body. Its
  render state is one tick behind the authoritative state by design, which is
  the same latency the primary vehicle already has.
- **Envelope geometry.** The nose cone stays presentation geometry. Physics does
  not model its mass distribution separately; the fix aligns the visible stage
  with the physical stage, which is the part collision and contact use.

## Migration Plan

No data or API migration. Unset environment variables restore normal
interactive behaviour exactly. Reverting the mesh offset restores the previous
(incorrect) staging pose.

## Open Questions

None.
