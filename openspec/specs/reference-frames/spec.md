# Reference Frames Specification

## Purpose

Defines the physical scale and reference-frame architecture for flight: how rocket dynamics use real meters in f64 while rendering stays in the visualization scale, and the authoritative conversions between solar-inertial, planet-centered, planet body-fixed, local tangent, and rocket-body frames.

## Requirements

### Requirement: Flight dynamics use real physical units

The rocket dynamics core SHALL operate in real SI units (meters, seconds, kilograms) independent of the visualization world scale used for rendering planets.

#### Scenario: Meter-based state

- **WHEN** the rocket dynamics integrates a state
- **THEN** position and velocity are expressed in meters and meters-per-second relative to a chosen frame

#### Scenario: Render mapping is explicit

- **WHEN** a rocket physical position is converted for rendering
- **THEN** the conversion applies the documented scale mapping between physical meters and display units

### Requirement: Reference-frame conversions are authoritative and single-sourced

The system SHALL provide one authoritative implementation for each frame conversion, and other subsystems SHALL reuse it rather than re-implementing coordinate math.

#### Scenario: Shared conversion utilities

- **WHEN** any subsystem needs a frame conversion (gravity, aero, terrain, camera)
- **THEN** it calls the shared reference-frame module rather than duplicating the math

#### Scenario: Supported frames

- **WHEN** the reference-frame module is used
- **THEN** it supports solar-inertial, planet-centered, planet body-fixed, local tangent (lat/lon/alt), and rocket-body frames

#### Scenario: Physical solar-inertial ephemeris boundary

- **WHEN** a primary-body ephemeris crosses from AU and AU/day into flight
  physics
- **THEN** its position and velocity are converted once to f64 solar-inertial
  meters and meters-per-second through the shared reference-frame module

### Requirement: High-precision dynamics with render boundary

The rocket dynamics SHALL be computed in f64 precision, and only converted to f32 render coordinates at the presentation boundary.

#### Scenario: f64 integration

- **WHEN** the rocket state is integrated
- **THEN** the integration operates on f64 (DVec3) values to avoid precision loss at large distances

#### Scenario: Local-origin rendering

- **WHEN** the rocket is rendered far from the solar-system origin
- **THEN** the render transform is computed relative to a local origin rather than the absolute solar position

### Requirement: Launch-site geography maps to body-fixed position

The system SHALL map real launch-site latitude/longitude/altitude (via `LaunchSiteCoordinates`) into planet body-fixed and inertial positions.

#### Scenario: KSC on Earth

- **WHEN** the Kennedy Space Center coordinates are converted to a body-fixed position
- **THEN** the resulting position is consistent with Earth's radius, axial tilt, and rotation

#### Scenario: Round-trip consistency

- **WHEN** a body-fixed position is converted to inertial and back through the same chain
- **THEN** the result matches the original within numerical tolerance

### Requirement: Physical scale is centralized

The mapping between physical meters and visualization units SHALL be defined in one place and reused by all rocket and terrain subsystems.

#### Scenario: Single scale source

- **WHEN** any subsystem maps between meters and display units
- **THEN** it uses the centralized scale definition rather than hardcoded factors
