## ADDED Requirements

### Requirement: The facility scales with the vehicle

The presented facility SHALL scale with the selected vehicle so that no part
intersects the vehicle and the tower reads as a service structure at every
catalogue size.

The hold-down clamp ring SHALL be derived from the hull radius. The tower SHALL
choose its platform count from the vehicle height so the bay height stays in a
believable band, and every service arm SHALL attach to an existing platform.
Support structures that belong to the vehicle SHALL scale with its height,
bounded so they stay visible.

The hardstand and the flame trench SHALL NOT scale: the trench width is bounded
by the terrain-mesh cutout contract, and shrinking it would hide the excavation.

#### Scenario: Smallest family

- **WHEN** an 18 m vehicle is presented
- **THEN** its clamps sit outside its hull, the tower has at least three
  platforms, and the service arms reach both the platform edge and the hull

#### Scenario: Largest family

- **WHEN** a 121 m vehicle is presented
- **THEN** its clamps sit outside its 4.5 m hull radius, the tower has enough
  platforms that no bay exceeds 20 m, and no platform rises above the tower

#### Scenario: Short tower

- **WHEN** the tower has fewer than four platforms
- **THEN** the service arms still attach to platforms that exist
