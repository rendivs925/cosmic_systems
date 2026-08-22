## Purpose

Provides a comprehensive flight telemetry UI: orbital elements panel, patched-conics trajectory prediction, terrain map overlay, and flight recorder with replay so operators can monitor, plan, and review missions.

## ADDED Requirements

### Requirement: Orbital elements panel

The system SHALL display real-time osculating orbital elements (semi-major axis, eccentricity, inclination, RAAN, argument of periapsis, true anomaly, period, apoapsis/periapsis altitude) relative to the current central body.

#### Scenario: Live orbital updates

- **WHEN** the rocket is in orbit
- **THEN** the panel updates each physics tick with current elements

#### Scenario: Target orbit comparison

- **WHEN** a target orbit is set (e.g., by guidance)
- **THEN** the panel shows current vs. target elements with deltas

### Requirement: Trajectory prediction (patched conics)

The system SHALL predict and render the future trajectory using patched-conics propagation through SOI transitions, showing ground track, apoapsis/periapsis markers, and encounter predictions.

#### Scenario: Multi-body propagation

- **WHEN** the trajectory crosses a planetary SOI boundary
- **THEN** the prediction switches central bodies and continues propagating

#### Scenario: Maneuver nodes

- **WHEN** the user places a maneuver node
- **THEN** the predicted trajectory updates to include the impulse

### Requirement: Terrain map overlay

The system SHALL render a 2D map projection of the current planetary body with terrain coloring, the rocket's ground track, landing site markers, and the predicted impact point.

#### Scenario: Ground track rendering

- **WHEN** the rocket is over a planetary body
- **THEN** the map shows the past and predicted ground track

#### Scenario: Landing site marker

- **WHEN** a landing target is active
- **THEN** the map shows the target with uncertainty ellipse

### Requirement: Flight recorder and replay

The system SHALL record the complete simulation state (position, velocity, attitude, mass, throttle, gimbal, guidance mode, atmospheric state, terrain contacts) at a configurable rate and support deterministic replay.

#### Scenario: Continuous recording

- **WHEN** the simulation runs in rocket mode
- **THEN** state is appended to a circular buffer or file at the configured sample rate

#### Scenario: Deterministic replay

- **WHEN** a recorded flight is replayed
- **THEN** the simulation reproduces the exact same state sequence bit-for-bit

#### Scenario: Replay scrubbing

- **WHEN** the user seeks in the replay timeline
- **THEN** the simulation state jumps to that timestamp and the UI updates