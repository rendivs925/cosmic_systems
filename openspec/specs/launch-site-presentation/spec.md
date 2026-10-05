# launch-site-presentation Specification

## Purpose
Defines the visual launch-site assets (pad, service tower, and supporting
structures) that are geodetically anchored to the active launch site while
remaining strictly presentational and separate from terrain height and
collision authority.
## Requirements
### Requirement: Launch-site structures are geodetically anchored

The system SHALL place the launch pad, service tower, and supporting structures
at the active launch site's body-fixed position and local vertical/heading, so
they track the correct terrain point and orientation for any configured site.

#### Scenario: Anchor follows the active site

- **WHEN** the active launch site is the Papua coastal lowland site
- **THEN** the pad and tower are placed at that site's body-fixed point with the
  local surface normal as up and the site heading as the pad's north

#### Scenario: Relocation without code changes

- **WHEN** the configured launch site coordinates change
- **THEN** the presentation assets move to the new site without editing asset
  placement constants

### Requirement: Launch-site presentation has no physics authority

The launch pad, service tower, and structures SHALL be presentation-only and
MUST NOT contribute terrain height, collision surfaces, or forces to the
simulation.

#### Scenario: No collision contribution

- **WHEN** the rocket or terrain collision queries the surface near the pad
- **THEN** the returned height and normals come only from the terrain source and
  not from any launch-site structure

#### Scenario: No force contribution

- **WHEN** the simulation integrates the vehicle state on the pad
- **THEN** no launch-site structure applies a force or torque

### Requirement: Launch-site presentation is quality-bounded

Launch-site structures SHALL be bounded in detail, draw distance, and light
count so their cost stays proportional to the camera's proximity and does not
grow without limit.

#### Scenario: Distant pad is cheap

- **WHEN** the camera is far from the launch site
- **THEN** pad children, lights, and effects are hidden or reduced and do not
  incur their full per-frame cost

#### Scenario: Near pad is detailed

- **WHEN** the camera is near the launch site
- **THEN** the pad, tower, and supporting structures are visible at full detail

