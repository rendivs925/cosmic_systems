## MODIFIED Requirements

### Requirement: Primary-body states have an explicit epoch and frame

The system SHALL evaluate the Sun, planets, and kernel-covered moons from a
shared TDB epoch as f64 barycentric ICRF/J2000 states in meters and
meters-per-second. All presentation and flight consumers SHALL derive their
positions from that physical state only at their respective frame and rendering
boundaries.

#### Scenario: Startup epoch

- **WHEN** the solar epoch has no elapsed simulation time
- **THEN** it resolves to JD TDB 2451545.0

#### Scenario: Render projection

- **WHEN** a solar-map position is rendered
- **THEN** its f64 physical state is converted to camera-relative display units
  only at the presentation boundary

#### Scenario: Flight-frame Sun presentation

- **WHEN** rocket-mode Sun geometry or directional lighting is updated
- **THEN** it consumes the same evaluated TDB state as other solar-system
  consumers rather than a solar-map transform or an artistic day/night orbit

#### Scenario: Physical primary-body state

- **WHEN** a solar-system flight feature needs a primary-body state
- **THEN** it receives the shared f64 barycentric ICRF/J2000 meter and
  meters-per-second state or an explicitly derived relative state

### Requirement: Primary bodies use one published ephemeris authority

The system SHALL evaluate primary-body states from one versioned local
NAIF SPICE kernel set backed by a declared JPL Development Ephemeris. No
consumer SHALL maintain a separate runtime primary-body Kepler table, numerical
velocity derivative, or network ephemeris query.

#### Scenario: Kernel state evaluation

- **WHEN** a supported primary-body state is requested at a supported epoch
- **THEN** the system evaluates it from the configured local kernel authority

#### Scenario: External reference regression

- **WHEN** the Earth, Moon, or a planet is evaluated at a recorded TDB epoch
- **THEN** its position and velocity compare with a recorded JPL Horizons
  reference within the declared kernel-validation tolerance

### Requirement: Model limitations remain explicit

The system SHALL expose only the bodies, time coverage, center/frame choices,
and orientation fidelity supplied by the selected manifest. It SHALL not claim
coverage or precision absent from the loaded kernels. Planetary rotation SHALL
be derived from the selected orientation kernels once available for that body.

#### Scenario: Unsupported body or epoch

- **WHEN** a request targets a body, orientation, or epoch outside the selected
  kernel coverage
- **THEN** the request fails explicitly with its missing coverage information

#### Scenario: Moon propagation

- **WHEN** a moon position is required and the selected kernel set covers it
- **THEN** the system uses the shared kernel-backed state rather than a
  parent-relative analytic moon model

#### Scenario: Future flight physics

- **WHEN** solar-system vehicle dynamics are introduced
- **THEN** they consume the shared physical body-state authority rather than
  solar-map transforms or presentation proxies

#### Scenario: Primary-bound solar perturbation

- **WHEN** a rocket is bound to a primary body with a physical ephemeris state
- **THEN** its gravity stage adds the Sun's differential acceleration between
  the rocket and the bound-body origin, without changing the rocket's
  planet-centered inertial coordinates
