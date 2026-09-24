## Purpose

Defines one explicit civil and dynamical epoch authority so all simulation and
presentation consumers evaluate the same physical instant in every mode.

## ADDED Requirements

### Requirement: One authoritative simulation epoch
The system SHALL maintain one authoritative epoch that carries UTC, TAI, TT,
TDB, and, when Earth orientation is enabled, UT1 representations of the same
instant. All body translation, orientation, force, telemetry, and presentation
consumers SHALL derive their epoch from this authority rather than from Bevy
wall-clock, fixed-clock, or presentation-scale clocks.

#### Scenario: Time acceleration
- **WHEN** time acceleration changes
- **THEN** it changes the rate at which the authoritative epoch advances without
  changing the epoch represented by an already completed fixed tick

#### Scenario: Cross-mode epoch agreement
- **WHEN** normal, craft, and rocket modes start with the same epoch and run the
  same completed fixed ticks
- **THEN** each mode exposes the same TDB epoch to shared ephemeris consumers

### Requirement: Civil-time conversion is explicit and bounded
The system SHALL convert selectable UTC start times through a versioned
leap-second source to TAI, TT, and TDB. Earth-specific UT1 conversion SHALL use
a versioned Earth-orientation dataset and SHALL report unavailable or
out-of-coverage data rather than silently substituting UTC.

#### Scenario: Leap-second conversion
- **WHEN** a UTC epoch crosses a recorded leap second
- **THEN** its TAI representation differs by the recorded discontinuity while
  TT and TDB remain continuous dynamical-time representations

#### Scenario: Missing Earth orientation data
- **WHEN** Earth orientation is requested outside the available EOP coverage
- **THEN** the system reports that the requested orientation is unavailable and
  does not claim reference-grade Earth-fixed output
