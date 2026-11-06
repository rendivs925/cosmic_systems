## ADDED Requirements

### Requirement: Every simulated body renders from its own state

Flight presentation SHALL advance the render snapshot of every body that carries
authoritative dynamics, including jettisoned hardware that has no mission state.
A body without a mission state SHALL use the normal interpolation path. Only a
surface-constrained mission vehicle MAY pin its latest snapshot.

#### Scenario: Detached stage

- **WHEN** a spent stage or fairing half has authoritative dynamics but no
  mission state
- **THEN** its render snapshot advances every fixed tick and it follows its own
  position and attitude rather than staying at the separation pose

#### Scenario: Landed vehicle

- **WHEN** a vehicle is landing, landed, crashed, or resting on the ground
- **THEN** it pins the latest fixed snapshot instead of interpolating against
  the surface

### Requirement: The mesh frame matches the physical body frame

The rendered vehicle mesh SHALL be authored in the same body frame as the
dynamics: the longitudinal axis SHALL be `+Y`, the mesh's lowest structural face
SHALL coincide with the physical lower extent, and re-origining at staging SHALL
keep the surviving stage cylinder centred on the physical origin.

#### Scenario: Full stack

- **WHEN** the full vehicle is rendered
- **THEN** its stage cylinders occupy the same longitudinal extent as the
  physical envelope

#### Scenario: Serial stage after separation

- **WHEN** a lower serial stage separates and physics switches to a
  stage-centred frame
- **THEN** the rebuilt mesh centres that stage cylinder on the origin and keeps
  the retained upper envelope above it, without a visible jump

#### Scenario: Render-origin rebase

- **WHEN** the flight render origin recentres
- **THEN** translation is rebased before the `f32` conversion and the
  authoritative attitude is preserved
