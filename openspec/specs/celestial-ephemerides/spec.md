# Celestial Ephemerides Specification

## Purpose

Defines the authoritative analytic ephemeris used by solar-map presentation and
future solar-system flight integration seams. This specification does not claim
N-body or DE/SPICE fidelity.

## Requirements

### Requirement: Primary-body states have an explicit epoch and frame

The system SHALL evaluate the Sun and eight planets from days relative to JD
TDB 2451545.0. States SHALL remain f64 heliocentric J2000 ecliptic values in
astronomical units and AU/day until a presentation consumer applies its visual
scale or a physical consumer adapts them through the authoritative
solar-inertial AU-to-meter conversion.

#### Scenario: Startup epoch

- **WHEN** the solar epoch has no elapsed simulation time
- **THEN** it resolves to JD TDB 2451545.0

#### Scenario: Render projection

- **WHEN** a solar-map position is rendered
- **THEN** its f64 ephemeris state is converted to display units only at the
  presentation boundary

#### Scenario: Flight-frame Sun presentation

- **WHEN** rocket-mode Sun geometry or directional lighting is updated
- **THEN** it consumes the same f64 state at `SimulationTime` rather than a
  solar-map transform or an artistic day/night orbit

#### Scenario: Physical primary-body state

- **WHEN** a future solar-system flight feature needs a primary-body state
- **THEN** it receives f64 heliocentric solar-inertial meters and
  meters-per-second derived from the shared AU/AU-day ephemeris

### Requirement: Primary bodies use one published secular-element authority

The system SHALL evaluate primary-body positions from JPL approximate-position
Table 1 elements and rates for the stated 1800 AD through 2050 AD validity
range. No consumer SHALL maintain a separate primary-body Kepler table.

#### Scenario: Secular evolution

- **WHEN** an epoch advances by one Julian century
- **THEN** the evaluated orbital shape includes the published element rates

#### Scenario: External reference regression

- **WHEN** the Earth-Moon barycenter is evaluated at JD TDB 2451545.0
- **THEN** its position and velocity are compared with a recorded JPL Horizons
  DE441 state within the published approximation error budget

### Requirement: Model limitations remain explicit

The system SHALL treat this model as an analytic, heliocentric approximation.
It SHALL NOT be presented as a barycentric N-body, lunar, Earth-center, or
body-orientation model. Primary-bound rocket flight MAY consume the Sun as a
differential third-body perturbation in its planet-centered inertial frame; it
is not yet a general multi-body spacecraft force model.

#### Scenario: Moon propagation

- **WHEN** a moon position is required
- **THEN** the current parent-relative analytic moon model is used and is not
  represented as JPL ephemeris accuracy

#### Scenario: Future flight physics

- **WHEN** solar-system vehicle dynamics are introduced
- **THEN** they consume a shared physical body-state authority rather than
  solar-map transforms or presentation proxies

#### Scenario: Primary-bound solar perturbation

- **WHEN** a rocket is bound to a primary body with a physical ephemeris state
- **THEN** its gravity stage adds the Sun's differential acceleration between
  the rocket and the bound-body origin, without changing the rocket's
  planet-centered inertial coordinates
