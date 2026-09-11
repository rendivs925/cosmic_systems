## Purpose

Provides a physically informed Rocket-mode presentation that visualizes authoritative flight state without becoming an alternative simulation or coordinate authority.

## ADDED Requirements

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
