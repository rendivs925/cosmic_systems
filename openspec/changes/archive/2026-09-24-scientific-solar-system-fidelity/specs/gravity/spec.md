## MODIFIED Requirements

### Requirement: Gravity uses real planet masses

The system SHALL compute high-fidelity gravitational acceleration from validated
gravitational parameters (GM) tied to the selected scientific dataset. Catalog
mass times a gravitational constant MAY remain only for bodies without approved
GM data and SHALL be labelled as an approximation.

#### Scenario: Surface acceleration on Earth

- **WHEN** gravitational acceleration is computed at Earth's reference radius
- **THEN** the magnitude is derived from the validated Earth GM and the selected
  gravity model's documented reference surface

#### Scenario: Inverse-square behavior

- **WHEN** distance from a point-mass body center doubles
- **THEN** the point-mass component of gravitational acceleration decreases to
  approximately one quarter

### Requirement: Perturbing gravity respects the accelerating origin

The system SHALL calculate each enabled third body's contribution to a
planet-centered inertial vehicle state as the difference between that body's
acceleration at the vehicle and at the bound-planet origin. The active force
model SHALL declare its enabled perturbing bodies and harmonics.

#### Scenario: Sun perturbation at the origin

- **WHEN** the vehicle is at the planet-centered origin
- **THEN** the Sun's differential acceleration is zero

#### Scenario: Lunar perturbation

- **WHEN** the Earth-Moon-Sun force tier is active near Earth
- **THEN** the Moon and Sun both contribute same-epoch differential
  accelerations from the shared physical body-state authority

### Requirement: Force-model fidelity is selectable and observable
The system SHALL expose named, deterministic force-model tiers with documented
included forces, valid use cases, and limits. Selecting a tier SHALL not change
the coordinate frame or units of the vehicle state.

#### Scenario: Earth J2 tier
- **WHEN** an Earth J2 force tier is selected
- **THEN** the model adds the documented zonal-harmonic acceleration to the
  Earth point-mass term and reports the tier in telemetry and validation output
