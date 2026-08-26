## MODIFIED Requirements

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
