## Purpose

Defines the aerodynamic force and torque model for the rocket: drag, lift, and side force from dynamic pressure and coefficients, angle of attack, aerodynamic torque about the center of mass, and Max Q detection, all consuming the shared atmosphere model.

## ADDED Requirements

### Requirement: Aerodynamic forces derive from atmosphere and geometry

The system SHALL compute drag, lift, and side force from dynamic pressure, aerodynamic coefficients, reference area, and the rocket's orientation relative to its velocity, using the shared atmosphere model.

#### Scenario: Drag magnitude and direction

- **WHEN** the rocket moves through air with dynamic pressure q, drag coefficient Cd, and reference area A
- **THEN** drag equals q times Cd times A and acts opposite the velocity direction

#### Scenario: Angle-of-attack dependence

- **WHEN** the rocket body axis and velocity vector differ
- **THEN** angle of attack is computed and influences the lift coefficient and lift magnitude

#### Scenario: Side force

- **WHEN** the rocket's orientation produces a sideslip component
- **THEN** a side force is computed from its coefficient

### Requirement: Dynamic pressure and Mach are computed from the atmosphere

The system SHALL compute dynamic pressure and Mach number using the local density and speed of sound from the atmosphere model.

#### Scenario: q from density and velocity

- **WHEN** the rocket is at velocity v and local density ρ
- **THEN** dynamic pressure is ½ ρ v²

#### Scenario: Mach from speed of sound

- **WHEN** the rocket speed and local speed of sound are known
- **THEN** Mach number is speed divided by speed of sound

### Requirement: Aerodynamic torque about the center of mass

The system SHALL apply aerodynamic force at the center of pressure to produce a torque about the center of mass.

#### Scenario: CoP offset torque

- **WHEN** aerodynamic force acts at the center of pressure offset from the center of mass
- **THEN** a torque is produced and added to the rotational accumulator

### Requirement: Max Q detection

The system SHALL track and expose the maximum dynamic pressure reached during flight.

#### Scenario: Peak dynamic pressure recorded

- **WHEN** dynamic pressure rises to a peak and then falls
- **THEN** the peak is recorded as Max Q and available for telemetry

### Requirement: Aerodynamics never write the transform

Aerodynamic systems SHALL deliver forces and torques to the 6-DOF accumulator only.

#### Scenario: Accumulator-only output

- **WHEN** aerodynamic forces and torques are computed
- **THEN** the rocket transform is not modified directly