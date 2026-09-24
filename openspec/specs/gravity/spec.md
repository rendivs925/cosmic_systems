# Gravity Specification

## Purpose

Defines authoritative planetary gravity for vehicles: Newtonian inverse-square gravity computed from real planet masses (f64) and the shared reference-frame module, with one gravity implementation reused by all consumers.

## Requirements

### Requirement: Perturbing gravity respects the accelerating origin

The system SHALL calculate a third body's contribution to a planet-centered
inertial vehicle state as the difference between the body's acceleration at the
vehicle and at the bound-planet origin. The third-body position SHALL come from
the shared kernel-backed ephemeris at the same TDB epoch as the bound-body
state. The active force model SHALL declare its enabled perturbing bodies and
harmonics.

#### Scenario: Sun perturbation at the origin

- **WHEN** the vehicle is at the planet-centered origin
- **THEN** the Sun's differential acceleration is zero

#### Scenario: Local rocket flight

- **WHEN** a primary-bound rocket evaluates gravity
- **THEN** it combines bound-planet gravity with the Sun's differential term
  from the shared kernel-backed ephemeris, not the Sun's full heliocentric force

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

### Requirement: One authoritative gravity implementation

The system SHALL provide a single gravity implementation reused by all vehicle and terrain consumers, with no duplicate gravity calculations in different subsystems.

#### Scenario: Rocket and craft share the source

- **WHEN** any vehicle subsystem requires gravity
- **THEN** it consumes the shared gravity implementation rather than defining its own

#### Scenario: No rendering gravity

- **WHEN** gravity is applied
- **THEN** the rendering layer does not compute a separate gravitational value for visuals

### Requirement: Gravity integrates with reference frames

Gravity SHALL be computed in the physical meter scale and the appropriate frame from the reference-frame module.

#### Scenario: Planet-centered frame

- **WHEN** gravity is computed for a vehicle near a planet
- **THEN** the computation uses the planet-centered position in meters

#### Scenario: Frame-consistent result

- **WHEN** the gravity vector is converted to another frame
- **THEN** the magnitude is preserved within numerical tolerance

### Requirement: Gravity is testable without Bevy

Gravity calculations SHALL be pure functions testable without launching the application.

#### Scenario: Unit-tested acceleration

- **WHEN** a unit test runs gravity for a known body
- **THEN** the expected acceleration, inverse-square behavior, and orbital period consistency are asserted

#### Scenario: Circular-orbit consistency

- **WHEN** an orbital velocity consistent with the computed gravity is applied
- **THEN** the resulting orbit period matches the Keplerian prediction within a documented tolerance
