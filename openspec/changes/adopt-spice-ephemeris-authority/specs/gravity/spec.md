## MODIFIED Requirements

### Requirement: Perturbing gravity respects the accelerating origin

The system SHALL calculate a third body's contribution to a planet-centered
inertial vehicle state as the difference between the body's acceleration at the
vehicle and at the bound-planet origin. The third-body position SHALL come from
the shared kernel-backed ephemeris at the same TDB epoch as the bound-body
state.

#### Scenario: Sun perturbation at the origin

- **WHEN** the vehicle is at the planet-centered origin
- **THEN** the Sun's differential acceleration is zero

#### Scenario: Local rocket flight

- **WHEN** a primary-bound rocket evaluates gravity
- **THEN** it combines bound-planet gravity with the Sun's differential term
  from the shared kernel-backed ephemeris, not the Sun's full heliocentric force
