# rocket-flight-presentation Specification

## Purpose
Provides a physically informed Rocket-mode presentation that visualizes authoritative flight state without becoming an alternative simulation or coordinate authority.
## Requirements
### Requirement: Flight presentation consumes authoritative state
Rocket flight presentation SHALL derive visual and audio parameters from authoritative propulsion, flight-condition, thermal, terrain-contact, lifecycle, ephemeris, and camera-observer state. It MUST NOT mutate rocket dynamics, propulsion, terrain, force, torque, or simulation-time state.

#### Scenario: Presentation updates during flight
- **WHEN** the rocket's authoritative state changes during a fixed simulation step
- **THEN** the next presentation update reflects that state without writing to authoritative flight components

#### Scenario: Render-origin rebase
- **WHEN** the flight render origin recenters
- **THEN** presentation attached to the rocket, pad, or terrain remains visually continuous

### Requirement: Flight presentation is quality bounded
Rocket flight presentation SHALL use bounded, distance- and visibility-aware work for transient effects and SHALL omit non-contributing effects outside their quality budget.

#### Scenario: Distant effect
- **WHEN** an effect has negligible screen contribution or is outside the active camera view
- **THEN** the system uses a lower-cost representation or suppresses it without changing flight state

### Requirement: Atmospheric presentation evolves with flight conditions
Rocket presentation SHALL transition sky, fog, external-effect, and external-audio behavior continuously from dense atmosphere toward vacuum using the authoritative atmospheric sample and altitude.

#### Scenario: Thin atmosphere transition
- **WHEN** atmospheric density decreases during ascent
- **THEN** atmospheric visual and external-audio contributions reduce continuously rather than switching abruptly

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

