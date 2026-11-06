## Why

RK-01 audited the path from authoritative attitude and staging state to the
rendered vehicle. The physics is correct — separation copies orientation and
body angular velocity, and the render transform is a direct body→world
quaternion — but two presentation defects meant the picture did not follow it:

1. `capture_render_state` required a mission state, so jettisoned hardware
   (spent stages and fairing halves carry none) never advanced its render
   snapshot. The debris stayed frozen at the separation pose while its
   authoritative state flew away.
2. The rebuilt mesh for a surviving serial stage was centred on the stage plus
   its retained upper envelope, while physics switches to a stage-centred frame
   with `lower_extent_y_m = -stage.height / 2`. The rendered upper stage sat half
   an envelope too low and jumped at the staging tick.

Neither defect changed physics, and neither was covered by a test.

## What Changes

- Render snapshots advance for every body with authoritative dynamics. A
  surface-constrained mission vehicle still pins its latest snapshot; a body
  without a mission state uses normal interpolation.
- The serial-stage mesh is built in the stage's own body frame, with the
  retained upper envelope above it.
- Separation now has explicit attitude-preservation tests for serial stages and
  parallel boosters.
- Two measurement-only capture controls make the required evidence producible
  without a window: `COSMIC_SYSTEMS_CAPTURE_BODY_AXES` enables the existing
  read-only body-frame gizmos, and `COSMIC_SYSTEMS_CAPTURE_WARMUP_STEPS` issues a
  real launch order and runs the production fixed schedule before the first
  captured frame.

No physical model, force, mass, guidance or terrain value changes.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `rocket-dynamics`: adds an explicit requirement that separation preserves the
  pre-separation attitude state for both bodies.
- `rocket-flight-presentation`: adds requirements that every simulated body
  renders from its own state and that the mesh frame matches the physical body
  frame.

## Impact

- `src/infrastructure/bevy_adapters/rocket/presentation.rs`: optional mission
  read, debris snapshot advance, two regressions.
- `src/application/rocket_spawning.rs`: mesh body-origin parameter,
  stage-centred serial mesh, mesh-frame regression.
- `src/infrastructure/bevy_adapters/rocket/propulsion.rs`,
  `lifecycle.rs`: call-site update only.
- `src/domain/services/rocket_propulsion/mod.rs`: attitude assertions in the
  existing separation tests.
- `src/infrastructure/bevy_adapters/rocket/capture_view.rs`,
  `debug/mod.rs`: measurement-only capture controls.
- No change to the three committed flight baselines.
