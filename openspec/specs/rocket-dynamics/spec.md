# Rocket Dynamics Specification

## Purpose

Defines 6-DOF rigid-body dynamics for the rocket: physically consistent translational and rotational motion driven by accumulated forces and torques, where physics is the authoritative source of rocket motion and the rendered transform is derived from it.
## Requirements
### Requirement: Translation dynamics are physically consistent

The rocket SHALL integrate position and velocity from net force and mass using a proper integrator with a bounded authoritative physics timestep. Gravity and other forces SHALL be inputs, and time acceleration SHALL NOT enlarge an individual powered-flight integration step beyond the configured bound.

#### Scenario: Net force drives acceleration

- **WHEN** a net force acts on the rocket of mass m
- **THEN** acceleration is net_force / m and velocity and position integrate accordingly

#### Scenario: Gravity affects trajectory

- **WHEN** the rocket is under gravitational force
- **THEN** the trajectory reflects gravitational acceleration (falls without thrust)

#### Scenario: Mass is a state

- **WHEN** the rocket's mass changes (e.g., propellant consumption)
- **THEN** acceleration under the same force reflects the updated mass

#### Scenario: Time-accelerated powered flight

- **WHEN** time acceleration is active during a powered burn
- **THEN** the simulation SHALL use bounded fixed substeps and produce deterministic state evolution for the same inputs

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

### Requirement: Completed fixed ticks share a simulation epoch
The fixed flight pipeline SHALL advance the authoritative simulation epoch immediately after integration and before post-integration terrain, orbital, render-capture, and telemetry consumers execute.

#### Scenario: Post-integration terrain sample
- **WHEN** a fixed tick integrates motion on a rotating body
- **THEN** terrain contact samples the body-fixed surface at the completed tick epoch

#### Scenario: Post-integration telemetry
- **WHEN** a fixed tick completes
- **THEN** recorded telemetry and orbital elements describe the integrated state at that tick's epoch

### Requirement: Pause gates fixed flight simulation
The system SHALL not run fixed flight simulation stages or advance the simulation epoch while simulation time is paused.

#### Scenario: Paused powered vehicle
- **WHEN** simulation time is paused while engines are active
- **THEN** position, velocity, attitude, mass, propellant, and simulation time remain unchanged until unpaused

### Requirement: Long-arc propagation has an explicit accuracy contract

The system SHALL provide a deterministic long-arc propagation path distinct from
high-rate powered-flight and contact integration. It SHALL expose its force
model, step or error-control configuration, and documented validity envelope.

#### Scenario: Propagation configuration

- **WHEN** a long-arc trajectory is requested
- **THEN** the result identifies the integration method, tolerances or fixed
  step, maximum propagation step, and selected force-model tier

#### Scenario: Contact isolation

- **WHEN** a vehicle is in powered flight or ground contact
- **THEN** the high-rate fixed pipeline remains authoritative and long-arc
  propagation does not directly mutate its state

### Requirement: Numerical accuracy is validated by scenario

The system SHALL publish scenario-specific numerical error budgets for at least
LEO, J2-precessing orbit, lunar transfer, and escape or interplanetary cases.

#### Scenario: LEO checkpoint validation

- **WHEN** a stated LEO validation duration completes
- **THEN** position and velocity residuals at each checkpoint satisfy the LEO
  budget for the selected force model

