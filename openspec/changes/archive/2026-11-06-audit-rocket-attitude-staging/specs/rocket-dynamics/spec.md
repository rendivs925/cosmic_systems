## ADDED Requirements

### Requirement: Separation preserves the attitude state

A stage separation SHALL preserve the pre-separation body attitude for both the
surviving vehicle and the detached hardware. Orientation and body-frame angular
velocity SHALL be carried over, not re-derived. Separation MAY change linear
velocity, mass, inertia, centre of mass and the body origin, and SHALL clear
angular acceleration for the following tick.

#### Scenario: Serial stage separation

- **WHEN** a serial stage separates
- **THEN** the upper stage and the spent stage keep the pre-separation
  orientation and body angular velocity

#### Scenario: Parallel booster jettison

- **WHEN** parallel boosters separate
- **THEN** every booster keeps the parent stack's attitude and inherited angular
  velocity

#### Scenario: Body origin moves, attitude does not

- **WHEN** the separated bodies are re-origined about their own centres of mass
- **THEN** their orientation is unchanged by the re-origin
