# Precision Orbit Insertion Specification

## Purpose

Provides a deterministic Earth-orbit insertion objective that evaluates the authoritative f64 state against safe, observable orbital constraints.

## Requirements

### Requirement: Earth insertion has explicit target constraints
The system SHALL define a target Earth orbit using apoapsis altitude, periapsis altitude, inclination, eccentricity tolerance, and minimum safe periapsis.

#### Scenario: Default low-Earth target
- **WHEN** the standard Earth launch mission is initialized
- **THEN** it targets a bound low-Earth orbit with a periapsis above the configured atmospheric safety boundary

### Requirement: Orbit completion requires a valid osculating orbit
The system SHALL declare insertion complete only when the authoritative state is bound and satisfies the configured target-orbit safety and tolerance constraints.

#### Scenario: Unsafe high-speed ascent is rejected
- **WHEN** a vehicle exceeds a circular-speed fraction but has a periapsis below the safety boundary
- **THEN** the system SHALL continue insertion guidance and SHALL NOT declare orbit complete

#### Scenario: Safe circular orbit is accepted
- **WHEN** a vehicle state satisfies the target radius, eccentricity, inclination, and safe-periapsis constraints
- **THEN** the system SHALL enter the orbit mission phase and command engine cutoff

### Requirement: Insertion guidance retains authority until completion
The system SHALL preserve the guidance throttle target through control and actuation until the target orbit is achieved or propulsion is unavailable.

#### Scenario: Circularization burn
- **WHEN** insertion guidance requires a prograde burn
- **THEN** the commanded throttle SHALL reach propulsion without a mission-phase throttle override
