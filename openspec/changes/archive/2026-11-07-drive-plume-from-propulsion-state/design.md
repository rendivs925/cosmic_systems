## Context

`rocket-engine-effects` already requires state-driven engine effects. The
existing implementation met that requirement for visibility, commanded throttle
and pressure-driven width, but the remaining channels were coarse:

- `map_presentation_parameters` computed `plume_intensity = throttle × running`.
  A running flag is not a physical quantity; it cannot show a tank running dry
  or a stage that is throttled below its command.
- `AppliedPropulsionState.thrust_n` is the net axial force. It includes an
  entry-flow effectiveness multiplier that models how external flow reduces the
  force the engine can deliver. Using it for intensity would dim the plume during
  high-Mach flight even though propellant is still burning.
- `plume_local_transform` scaled every layer's radius by the expansion ratio but
  only its length by intensity. A vacuum plume is longer as well as wider.
- The transform had no rotation, so the plume always pointed along body `-Y`
  while the force path used the gimbaled thrust axis.

## Goals / Non-Goals

**Goals:**

- Drive intensity from the burned propellant, the quantity that actually leaves
  the nozzle.
- Make plume length respond to the same expansion that widens it.
- Make plume direction agree with the force being applied.
- Give each independently flying vehicle its own plume state.
- Keep the mapping pure and testable, and keep runtime cost bounded.

**Non-Goals:**

- Computational fluid dynamics, shocks, or reacting flow.
- Per-effect materials or per-pixel brightness; materials stay shared per layer.
- Changing thrust, mass flow, propellant consumption, or staging.
- Changing the committed flight baselines.

## Decisions

### Decision: mass flow, not applied axial thrust

`propulsion_thrust` already computes each running engine's mass flow through
`stage_gimbaled_thrust_body`, then discards it. It now sums the flow actually
burned (multiplied by the propellant-limited `burn_fraction`) into
`AppliedPropulsionState::mass_flow_kg_s`. Presentation divides that by the
full-throttle mass flow of the same running engine set.

Mass flow is preferred over the applied thrust because:

- the engine model already states `T = m_dot · Isp(p) · g0`, so mass flow is the
  upstream physical quantity;
- the entry-flow multiplier is a force model, not a combustion model;
- the mass-flow normalizer is pressure-independent, so altitude enters only
  through the expansion channel and is not counted twice.

`AppliedPropulsionState` is the existing one-writer diagnostic snapshot used by
telemetry. Extending it reuses the force computation instead of duplicating an
engine operating point in presentation.

### Decision: intensity no longer multiplies throttle

The parameter input is renamed `mass_flow_fraction_unit` because the measured
fraction already contains the commanded throttle and any propellant limit. Audio
and ground presentation do not have the diagnostic sample, so they pass the
commanded throttle gated by their existing running check. Their output is
unchanged.

### Decision: length carries expansion too

For the inner layer, length grows by `1 + 0.6 (e - 1)`; for the outer layer by
`e`, where `e` is the pressure-derived expansion ratio in `1..3`. The core layer
keeps a constant length so the effect still reads as a nozzle. The ratios are
presentation constants, documented as an approximation; the driving quantity is
the measured ambient pressure.

### Decision: direction comes from the force path

`gimbaled_plume_axis_body` calls `clamp_gimbal` and
`gimbaled_thrust_direction_body` with the same stage deflection the force path
uses. The exhaust direction is the negative axis; the cone's authored apex is
`+Y`, so the transform is `Quat::from_rotation_arc(NEG_Y, -axis)`. A degenerate
catalog axis falls back to `+Y`.

### Decision: recovering stages get the diagnostic sample

The recovery spawn inserted a clone of the spent stage's propulsion but no
`AppliedPropulsionState`, so presentation fell back to the commanded throttle
(the recovery autopilot was at 5%). Adding the default component lets the shared
force writer fill it, and the recovery plume now follows real burned flow. This
is a diagnostic component; it does not change the force path.

### Decision: capture controls stay measurement-only

A capture has no keyboard and a chase camera looks along the thrust axis, so a
plume foreshortens to a dot. Two opt-in controls make the evidence producible:
`COSMIC_SYSTEMS_CAPTURE_EFFECTS` logs the driving state every 30 frames, and
`COSMIC_SYSTEMS_CAPTURE_TRACK=side` places the capture camera along the body's
lateral axis. Both are inert when unset and neither is read by simulation code.

## Risks / Trade-offs

- **One fixed tick of latency.** Presentation reads the last completed fixed
  tick, the same latency as every other effect channel.
- **Approximate geometry.** Layer lengths and radii are bounded constants, not a
  gas-dynamic solution. The specification requires the response direction, not a
  numerical plume model.
- **Diagnostic component growth.** `AppliedPropulsionState` gained one field. It
  remains a derived snapshot, not physics state.
- **Log volume.** The capture log prints every 30 frames and only when the
  measurement variable is set.

## Migration Plan

No data migration. Unset environment variables restore the previous interactive
behaviour except for the corrected plume, which is the intended change. Reverting
the effect code restores the previous visual; the diagnostic field is additive.

## Open Questions

None.
