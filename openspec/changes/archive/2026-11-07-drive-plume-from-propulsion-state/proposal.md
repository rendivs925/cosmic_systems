## Why

RK-02 asked whether the engine plume is driven by propulsion state or is a
static cone. It was already partly state-driven: visibility required a running
engine, intensity used the commanded throttle, expansion used ambient pressure,
and the three layers were quality-gated. Three gaps remained.

1. **Intensity was binary per engine.** `thrust_fraction_unit` was
   `engine_running as u8`, so a propellant-limited final tick, a partially
   throttled stage, or a stopped engine with a stale sample all looked the same
   until the engines were marked off.
2. **Altitude changed width but not length.** The inner and outer radii scaled
   with the pressure-derived expansion ratio, but their length did not. A
   vacuum plume is also longer, so the state was only half presented.
3. **Direction ignored the gimbal.** Every plume pointed along body `-Y`, even
   when the authoritative force used a gimbaled thrust vector. The picture
   contradicted the force being applied.

The audit also found that recovering stages carry `RocketPropulsion` but not
`AppliedPropulsionState`, so their landing-burn plume fell back to a commanded
throttle guess.

## What Changes

- `AppliedPropulsionState` now records the propellant mass flow actually burned
  this tick. `propulsion_thrust` writes it next to the applied thrust.
- Plume intensity is the ratio of burned mass flow to the full-throttle mass
  flow of the same running engines. Mass flow is the plume's physical driver and
  is unaffected by the entry-flow force multiplier, which reduces net axial
  force without reducing combustion.
- Plume length grows with the pressure-derived expansion ratio as well as the
  burn fraction. The core stays anchored near the nozzle; the inner and outer
  layers carry the altitude response.
- Plume direction follows the engine's gimbaled thrust axis, using the same
  `clamp_gimbal` and `gimbaled_thrust_direction_body` functions as the force
  path, so a gimballed engine tilts its plume.
- Recovering stages receive `AppliedPropulsionState`, so their own recovery
  burn drives their own plume.
- Two measurement-only capture controls make the evidence producible without a
  window: `COSMIC_SYSTEMS_CAPTURE_EFFECTS` logs the driving state, and
  `COSMIC_SYSTEMS_CAPTURE_TRACK=side` places the capture camera beside the
  vehicle (a chase camera looks down the plume axis and foreshortens it).

No force, mass, guidance, atmosphere, or terrain value changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rocket-engine-effects`: adds mass-flow-driven intensity, pressure-driven
  plume length, gimbaled plume direction, and per-vehicle plume state for
  recovering hardware.

## Impact

- `src/infrastructure/bevy_adapters/rocket/components/mod.rs`: one diagnostic
  field on `AppliedPropulsionState`.
- `src/infrastructure/bevy_adapters/rocket/propulsion.rs`: write the burned mass
  flow; give recovering stages the diagnostic component.
- `src/infrastructure/bevy_adapters/rocket/presentation_parameters.rs`: the
  input is the burned mass-flow fraction; intensity no longer multiplies the
  throttle a second time.
- `src/infrastructure/bevy_adapters/rocket/effects.rs`: measured fraction,
  gimbaled axis, axial expansion, capture log.
- `src/infrastructure/bevy_adapters/rocket/audio.rs`,
  `ground_presentation.rs`: call-site rename only.
- `src/infrastructure/bevy_adapters/rocket/capture_view.rs`: side tracking
  camera.
- No change to the three committed flight baselines.
