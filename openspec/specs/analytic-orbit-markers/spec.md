# Analytic Orbit Markers Specification

## Purpose

Provides physically defined apoapsis and periapsis markers for the rocket trajectory display from authoritative f64 two-body orbital geometry.

## Requirements

### Requirement: Apsis markers use analytic orbital geometry
The system SHALL derive apoapsis and periapsis positions from the current bound state vector and gravitational parameter, rather than selecting the nearest propagated sample.

#### Scenario: Eccentric bound orbit
- **WHEN** a bound orbit has non-negligible eccentricity
- **THEN** the displayed apoapsis and periapsis markers SHALL lie at the analytic extrema of the osculating conic

#### Scenario: Circular orbit
- **WHEN** an orbit has no unique apsis direction within the configured eccentricity tolerance
- **THEN** the system SHALL omit apsis markers

### Requirement: Surface-intersecting predictions do not report a false periapsis
The system SHALL omit a periapsis marker when a predicted trajectory intersects the planet surface before reaching its physical periapsis.

#### Scenario: Ballistic impact
- **WHEN** a propagated trajectory reaches the planet surface before periapsis
- **THEN** the trajectory SHALL end at the impact intersection and no periapsis marker SHALL be displayed
