## Purpose

Defines 6-DOF rigid-body dynamics for the rocket: physically consistent translational and rotational motion driven by accumulated forces and torques, where physics is the authoritative source of rocket motion and the rendered transform is derived from it.

## ADDED Requirements

### Requirement: Translation dynamics are physically consistent

The rocket SHALL integrate position and velocity from net force and mass using a proper integrator, with gravity and other forces as inputs.

#### Scenario: Net force drives acceleration

- **WHEN** a net force acts on the rocket of mass m
- **THEN** acceleration is net_force / m and velocity and position integrate accordingly

#### Scenario: Gravity affects trajectory

- **WHEN** the rocket is under gravitational force
- **THEN** the trajectory reflects gravitational acceleration (falls without thrust)

#### Scenario: Mass is a state

- **WHEN** the rocket's mass changes (e.g., propellant consumption)
- **THEN** acceleration under the same force reflects the updated mass

### Requirement: Rotational dynamics use an inertia model

The rocket SHALL integrate orientation and angular velocity from net torque, the current angular velocity, and an inertia tensor.

#### Scenario: Torque produces angular acceleration

- **WHEN** a net torque is applied about a principal axis
- **THEN** angular velocity changes proportional to torque divided by the moment of inertia about that axis

#### Scenario: Stable zero-torque rotation

- **WHEN** no torque is applied
- **THEN** angular velocity remains constant and orientation integrates without drift or unbounded growth

#### Scenario: Quaternion validity

- **WHEN** orientation is integrated
- **THEN** the quaternion is normalized and represents a valid rotation

### Requirement: Physics is the authoritative motion source

The rocket's rendered transform SHALL be derived from the physical state; no system SHALL directly teleport or rotate the rocket's transform to fake motion.

#### Scenario: Transform follows state

- **WHEN** physics updates the rocket state
- **THEN** the transform is synchronized from the physical position and orientation

#### Scenario: No direct transform manipulation

- **WHEN** control or guidance systems act
- **THEN** they modify forces/torques or commanded state, not the transform directly

### Requirement: 6-DOF state is cohesive

The rocket SHALL expose a physical state carrying position, velocity, acceleration, mass, orientation, angular velocity, angular acceleration, center of mass, and inertia tensor.

#### Scenario: State completeness

- **WHEN** any dynamics system reads the rocket state
- **THEN** the required translational and rotational quantities are available from the state

#### Scenario: Inertia reflects mass distribution

- **WHEN** the rocket consumes propellant
- **THEN** the inertia tensor and center of mass update to reflect the changing mass distribution