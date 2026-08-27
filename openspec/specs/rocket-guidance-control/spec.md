# Rocket Guidance Control Specification

## Purpose

Defines the separation of guidance, control, actuation, and physics for the rocket: guidance computes targets, control commands actuators, actuation applies physical limits, and physics integrates the actual state, with no layer directly manipulating the rocket's motion.

## Requirements

### Requirement: Guidance, control, actuation, and physics are separate

The system SHALL implement guidance, control, actuation, and physics as distinct concepts with explicit ordering and no cross-layer direct state manipulation.

#### Scenario: Ordered pipeline

- **WHEN** the flight loop runs
- **THEN** guidance runs before control, control before actuation, and actuation before physics integration

#### Scenario: No layer teleports the rocket

- **WHEN** guidance, control, or actuation act
- **THEN** none of them directly writes the rocket's transform or physical motion; they produce commands consumed by physics

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

### Requirement: Actuation enforces physical limits

The actuation layer SHALL apply physical actuator constraints (gimbal range, throttle slew, RCS maximum) before forces/torques reach physics.

#### Scenario: Gimbal clamp

- **WHEN** a gimbal command exceeds the engine range
- **THEN** the actuation layer clamps it to the range

#### Scenario: Throttle slew

- **WHEN** a throttle command changes faster than the actuator allows
- **THEN** the actuation layer limits the rate of change

### Requirement: Physics remains authoritative

Physics SHALL integrate forces and torques from the actuated commands and external forces (gravity, aero); guidance/control/actuation SHALL NOT write physical state directly.

#### Scenario: Physics integration of commands

- **WHEN** actuated commands and external forces are available
- **THEN** physics integrates them into new translational and rotational state

#### Scenario: Closed loop

- **WHEN** the flight loop iterates
- **THEN** guidance reads the latest integrated state to compute the next targets
