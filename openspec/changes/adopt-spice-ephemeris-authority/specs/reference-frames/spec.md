## MODIFIED Requirements

### Requirement: Reference-frame conversions are authoritative and single-sourced

The system SHALL provide one authoritative implementation for each frame
conversion, and other subsystems SHALL reuse it rather than re-implementing
coordinate math. The physical solar-system authority SHALL be barycentric
ICRF/J2000 f64 SI state at a TDB epoch; heliocentric, planet-centered,
body-fixed, local tangent, and render frames SHALL be explicitly derived from
that state.

#### Scenario: Shared conversion utilities

- **WHEN** any subsystem needs a frame conversion (gravity, aero, terrain, camera)
- **THEN** it calls the shared reference-frame module rather than duplicating the math

#### Scenario: Supported frames

- **WHEN** the reference-frame module is used
- **THEN** it supports barycentric ICRF/J2000, derived heliocentric,
  planet-centered inertial, planet body-fixed, local tangent (lat/lon/alt), and
  rocket-body frames

#### Scenario: Physical solar-system ephemeris boundary

- **WHEN** an ephemeris state enters flight physics
- **THEN** its position and velocity remain f64 barycentric ICRF/J2000 meters
  and meters-per-second until an explicit shared relative-frame conversion

#### Scenario: Render projection

- **WHEN** a physical solar-system position is projected for rendering
- **THEN** it is rebased in f64 against the selected render origin before the
  centralized meter-to-display and f32 conversion
