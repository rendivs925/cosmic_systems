## ADDED Requirements

### Requirement: Body orientation has an explicit scientific authority
The system SHALL derive a high-fidelity body's body-fixed orientation from a
versioned orientation authority at the shared epoch. The orientation contract
SHALL identify its inertial frame, body-fixed frame, time scale, pole model, and
prime-meridian convention.

#### Scenario: Body-fixed reference case
- **WHEN** a body orientation is evaluated at a recorded reference epoch
- **THEN** its inertial-to-body-fixed rotation matches the declared reference
  case within the published angular tolerance

#### Scenario: Presentation isolation
- **WHEN** a texture, terrain patch, or rendered globe is oriented
- **THEN** it consumes the authoritative body orientation without becoming a
  source of physical orientation

### Requirement: Earth geodesy distinguishes ellipsoidal and spherical bodies
The system SHALL use WGS-84 ellipsoidal geodetic conversions for Earth flight,
including documented latitude, longitude, and ellipsoidal-height conventions.
Bodies without an approved ellipsoid SHALL retain an explicitly spherical
geodetic model.

#### Scenario: Earth launch site
- **WHEN** a WGS-84 Earth launch site is converted to Earth-fixed coordinates
- **THEN** its position uses the WGS-84 semi-major axis and flattening rather
  than Earth's catalog mean radius

#### Scenario: Non-Earth body
- **WHEN** a body has no approved ellipsoid
- **THEN** its documented spherical conversion remains available and is not
  presented as ellipsoidal geodesy
