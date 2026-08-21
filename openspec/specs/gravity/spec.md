# Gravity Specification

## Purpose

Defines authoritative planetary gravity for vehicles: Newtonian inverse-square gravity computed from real planet masses (f64) and the shared reference-frame module, with one gravity implementation reused by all consumers.

## Requirements

### Requirement: Gravity uses real planet masses

The system SHALL compute gravitational acceleration from a celestial body's mass using Newton's law of universal gravitation, consuming the existing `Planet.mass_kg` (f64) value.

#### Scenario: Surface acceleration on Earth

- **WHEN** gravitational acceleration is computed at Earth's surface radius
- **THEN** the magnitude is approximately `GM_earth / r_earth²` and within a documented tolerance of 9.8 m/s²

#### Scenario: Inverse-square behavior

- **WHEN** distance from the body center doubles
- **THEN** the gravitational acceleration magnitude decreases to approximately one quarter

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