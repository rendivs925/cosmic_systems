## MODIFIED Requirements

### Requirement: Guidance produces targets
Guidance SHALL compute desired trajectory targets, including attitude and throttle targets for ascent and orbit insertion, from the mission and current state without controlling actuators directly. Control SHALL preserve guidance throttle targets while computing attitude-control outputs.

#### Scenario: Ascent guidance target
- **WHEN** a launch is in progress
- **THEN** guidance produces a target attitude and throttle for the current ascent phase

#### Scenario: Orbit insertion target
- **WHEN** the mission requires orbit insertion
- **THEN** guidance provides the target state, attitude, and throttle required to reach the insertion objective

### Requirement: Control commands actuators
The control layer SHALL convert guidance attitude targets and current state into gimbal and RCS commands using a controller, while preserving the throttle target produced by guidance for actuation.

#### Scenario: Attitude convergence
- **WHEN** the rocket attitude differs from the commanded target
- **THEN** control produces commands that drive the attitude toward the target with bounded overshoot

#### Scenario: Command bounds
- **WHEN** the controller produces a command
- **THEN** it is bounded to actuator limits before being applied

#### Scenario: Insertion throttle preservation
- **WHEN** guidance commands a nonzero insertion throttle
- **THEN** control SHALL NOT replace it based solely on the current mission phase
