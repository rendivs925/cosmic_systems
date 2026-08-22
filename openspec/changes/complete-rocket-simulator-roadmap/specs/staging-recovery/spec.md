## Purpose

Implements autonomous first-stage recovery: boostback burn guidance, grid-fin atmospheric control, landing-leg deployment, drone-ship station-keeping, and catch-tower operations so stages return for reuse.

## ADDED Requirements

### Requirement: Boostback burn guidance

The system SHALL compute a boostback burn that reverses the stage's downrange velocity and targets a recovery zone (RTLS pad, drone ship, or catch tower).

#### Scenario: RTLS boostback

- **WHEN** the stage separates and the mission is RTLS
- **THEN** guidance computes a boostback burn targeting the launch pad with margin for entry and landing

#### Scenario: Downrange drone ship boostback

- **WHEN** the mission targets a drone ship
- **THEN** guidance computes a partial boostback targeting the ship's predicted position at landing

### Requirement: Grid-fin atmospheric control

The system SHALL control four grid fins to steer the stage during atmospheric entry and descent, modulating pitch/yaw/roll to follow the guidance trajectory.

#### Scenario: Grid-fin deflection limits

- **WHEN** guidance commands a grid-fin deflection
- **THEN** the actuation system clamps to the fin's mechanical range (±30° typical)

#### Scenario: Hypersonic grid-fin effectiveness

- **WHEN** the stage is at hypersonic speeds
- **THEN** the control model accounts for reduced fin effectiveness and shock interactions

### Requirement: Landing leg deployment

The system SHALL deploy landing legs at a configured altitude/velocity gate and lock them down for touchdown.

#### Scenario: Leg deployment sequence

- **WHEN** the stage reaches the deployment trigger
- **THEN** legs extend and lock within a configured time, and the system verifies lock status

#### Scenario: Leg load at touchdown

- **WHEN** the stage touches down
- **THEN** the collision system computes leg loads and reports if they exceed design limits

### Requirement: Drone ship station-keeping

The system SHALL simulate a drone ship that holds position within a tolerance using its own thrusters, and the landing guidance targets the ship's predicted position.

#### Scenario: Ship position prediction

- **WHEN** the stage is in terminal descent
- **THEN** guidance uses the ship's reported position + predicted drift over the remaining time

#### Scenario: Ship motion compensation

- **WHEN** the ship moves due to waves/wind
- **THEN** the guidance divert adjusts the target point in real time

### Requirement: Catch tower chopstick capture

The system SHALL model a catch tower with articulated arms that can capture a descending stage by its hardpoints.

#### Scenario: Catch envelope

- **WHEN** the stage enters the tower's catch volume
- **THEN** the tower arms close on the hardpoints if position/velocity are within tolerances

#### Scenario: Capture success criteria

- **WHEN** the arms engage
- **THEN** success requires relative velocity < threshold, attitude within limits, and hardpoint alignment