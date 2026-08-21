## Purpose

Defines per-planet atmosphere models (temperature, pressure, density, speed of sound by altitude) and the aerodynamic force model (drag, lift, side force, dynamic pressure, Mach, angle of attack, aerodynamic torque, Max Q) that consumes them.

## ADDED Requirements

### Requirement: Atmosphere is per-planet and centralized

The system SHALL provide an atmosphere model per planet that returns temperature, pressure, density, and speed of sound for a given altitude, with one authoritative implementation reused by all consumers.

#### Scenario: Earth atmosphere by altitude

- **WHEN** atmosphere properties are requested for Earth at a given altitude
- **THEN** the model returns physically consistent temperature, pressure, density, and speed of sound

#### Scenario: Different planets differ

- **WHEN** atmosphere properties are requested for different planets at the same altitude
- **THEN** the values reflect each planet's own atmosphere model

#### Scenario: No scattered density formulas

- **WHEN** any subsystem needs atmospheric density or pressure
- **THEN** it consumes the shared atmosphere model rather than defining its own formulas

### Requirement: Dynamic pressure and Mach are computed

The system SHALL compute dynamic pressure `q = ½ρv²` and Mach number from density, velocity, and speed of sound.

#### Scenario: Dynamic pressure

- **WHEN** the rocket moves at velocity v through air of density ρ
- **THEN** dynamic pressure equals ½ ρ v²

#### Scenario: Mach number

- **WHEN** the rocket's speed and local speed of sound are known
- **THEN** Mach equals speed divided by the local speed of sound

### Requirement: Aerodynamic forces are physical

The system SHALL compute drag, lift, and side force from dynamic pressure, aerodynamic coefficients, reference area, and the rocket's orientation relative to its velocity.

#### Scenario: Drag opposes velocity

- **WHEN** the rocket moves through air
- **THEN** drag acts opposite the velocity vector with magnitude q times Cd times reference area

#### Scenario: Angle of attack

- **WHEN** the rocket's body axis is not aligned with its velocity
- **THEN** angle of attack is computed and used in aerodynamic coefficient evaluation

#### Scenario: Lift and side force

- **WHEN** the rocket is at an angle to the flow
- **THEN** lift and side force components are computed from their respective coefficients

### Requirement: Aerodynamic torque is produced

The system SHALL compute aerodynamic torque from the aerodynamic force applied at the center of pressure offset from the center of mass.

#### Scenario: CoP offset torque

- **WHEN** aerodynamic force acts at the center of pressure
- **THEN** a torque is produced from the force's offset relative to the center of mass

### Requirement: Max Q is detected

The system SHALL track dynamic pressure and report the maximum dynamic pressure (Max Q) reached during flight.

#### Scenario: Max Q recorded

- **WHEN** dynamic pressure peaks during ascent
- **THEN** the maximum value is recorded and exposed for telemetry

### Requirement: Aero forces feed the dynamics pipeline

Aerodynamic forces and torques SHALL be added to the 6-DOF force/torque accumulator and MUST NOT modify the rocket transform directly.

#### Scenario: Accumulator integration

- **WHEN** aerodynamic forces and torques are computed
- **THEN** they are delivered to the translational and rotational accumulators for integration