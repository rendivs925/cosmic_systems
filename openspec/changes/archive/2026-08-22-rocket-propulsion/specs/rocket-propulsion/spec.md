## Purpose

Defines physically consistent rocket propulsion: thrust derived from mass flow and specific impulse, throttle control, propellant consumption with variable mass, engine startup/shutdown, staging, and engine gimbal torque feeding the 6-DOF dynamics.

## ADDED Requirements

### Requirement: Thrust follows the rocket equation

The system SHALL compute thrust from mass flow rate and specific impulse as `T = m_dot * Isp * g0`, honoring the selected ISP (sea level or vacuum by environment).

#### Scenario: Thrust from ISP and mass flow

- **WHEN** an engine is throttled to a given level
- **THEN** thrust equals mass flow times specific impulse times standard gravity

#### Scenario: Vacuum ISP in space

- **WHEN** the rocket is above the significant atmosphere
- **THEN** thrust uses the vacuum ISP value

#### Scenario: Sea-level ISP in atmosphere

- **WHEN** the rocket is in the dense lower atmosphere
- **THEN** thrust uses the sea-level ISP value

### Requirement: Propellant consumption is mass-conserving

The system SHALL deplete propellant at `m_dot` per second and update the vehicle mass accordingly.

#### Scenario: Mass loss matches flow

- **WHEN** an engine burns at a constant throttle for time t
- **THEN** propellant mass decreases by m_dot times t and total mass decreases accordingly

#### Scenario: Propellant exhaustion

- **WHEN** propellant reaches zero
- **THEN** the engine shuts down and thrust drops to zero

### Requirement: Throttle control is explicit

The system SHALL accept a throttle command in a bounded range and apply it to engine thrust.

#### Scenario: Throttle range

- **WHEN** throttle is commanded outside the valid range
- **THEN** it is clamped to the engine's allowed minimum and maximum

#### Scenario: Zero throttle

- **WHEN** throttle is zero or engines are off
- **THEN** no thrust or mass flow occurs

### Requirement: Staging is supported

The system SHALL support multiple stages, shedding the spent stage's dry and residual mass and igniting the next stage.

#### Scenario: Stage separation

- **WHEN** the current stage is exhausted or separation is commanded
- **THEN** the spent stage mass is removed and the next stage's engines become active

#### Scenario: Final stage burnout

- **WHEN** the final stage's propellant is exhausted
- **THEN** no engines remain active and thrust is zero

### Requirement: Engine gimbal produces torque

The system SHALL apply gimbal deflection within the engine's range to produce a torque about the rocket's center of mass from the engine thrust offset.

#### Scenario: Gimbal deflection torque

- **WHEN** an engine gimbals by an angle within its limit
- **THEN** a torque is produced proportional to thrust and the thrust-line offset from the center of mass

#### Scenario: Gimbal limits

- **WHEN** a gimbal command exceeds the engine range
- **THEN** the deflection is clamped to the engine's `gimbal_range_deg`

### Requirement: Propulsion feeds the dynamics pipeline

Thrust forces and gimbal torques SHALL be delivered to the 6-DOF force/torque accumulator; propulsion SHALL NOT write the rocket transform directly.

#### Scenario: Force and torque accumulation

- **WHEN** engines are active
- **THEN** thrust is added to the translational accumulator and gimbal torque to the rotational accumulator for integration