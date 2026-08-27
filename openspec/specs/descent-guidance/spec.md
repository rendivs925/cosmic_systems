# descent-guidance Specification

## Purpose

Computes deorbit burns, manages reentry corridor constraints, and executes terminal guidance for powered and unpowered landings (RTLS, drone ship, lunar) so the rocket returns safely from orbit.

## Requirements

### Requirement: Deorbit burn targeting

The system SHALL compute a retrograde burn that lowers periapsis into a targeted reentry corridor at a specified entry interface altitude.

#### Scenario: Deorbit to entry interface

- **WHEN** the mission requests deorbit from a circular orbit
- **THEN** guidance computes a burn delta-v, attitude, and ignition time that places periapsis at the configured entry interface altitude

#### Scenario: Targeted landing site

- **WHEN** a landing site (latitude/longitude) is specified
- **THEN** the deorbit burn targets a ground track that passes over the site at entry interface

### Requirement: Reentry corridor management

The system SHALL maintain the vehicle within a defined reentry corridor (dynamic pressure, heat flux, g-load limits) by modulating bank angle or angle of attack.

#### Scenario: Corridor entry

- **WHEN** the vehicle crosses the entry interface
- **THEN** guidance initializes a bank-angle profile that keeps q, heat flux, and g-load within configured bounds

#### Scenario: Cross-range capability

- **WHEN** the vehicle has cross-range to the landing site
- **THEN** guidance modulates bank angle sign to steer toward the site while staying in the corridor

### Requirement: Terminal guidance for powered landing

The system SHALL guide the vehicle from the end of atmospheric flight to a soft touchdown using engine thrust, including hover, divert, and final descent phases.

#### Scenario: Powered descent initiation

- **WHEN** the vehicle exits the dense atmosphere at subsonic speed
- **THEN** guidance transitions to powered descent and computes a thrust profile for hover and divert

#### Scenario: RTLS landing

- **WHEN** the mission is Return to Launch Site
- **THEN** terminal guidance targets the launch pad coordinates with zero terminal velocity

#### Scenario: Drone ship landing

- **WHEN** the mission targets a drone ship
- **THEN** terminal guidance targets the ship's predicted position with station-keeping compensation

#### Scenario: Lunar landing

- **WHEN** the mission is a lunar landing
- **THEN** terminal guidance accounts for no atmosphere, lower gravity, and terrain-relative navigation

### Requirement: Terminal guidance for unpowered landing

The system SHALL support parachute/parafoil terminal guidance for vehicles without powered landing capability.

#### Scenario: Parachute deployment altitude

- **WHEN** the vehicle reaches the configured deployment altitude
- **THEN** guidance triggers parachute deployment and transitions to parafoil guidance

#### Scenario: Parafoil steering to target

- **WHEN** the parafoil is deployed
- **THEN** guidance steers the parafoil toward the landing target using asymmetric brake inputs
