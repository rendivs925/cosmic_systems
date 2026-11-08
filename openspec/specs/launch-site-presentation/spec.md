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

### Requirement: Ground-based facility hardware sits on the pad plane

Every ground-based launch-site structure SHALL have its lowest mesh vertex within
0.05 m of the pad plane. Mounted items (tower levels, braces, service arms,
beacons and floodlight heads) and the deliberately embedded apron/deck slabs are
excluded.

#### Scenario: Spawned scene

- **WHEN** the procedural pad is spawned at a site
- **THEN** the hold-down clamps, deluge rails, water tanks, blockhouse,
  floodlight poles and tower legs meet the plane within 0.05 m

#### Scenario: Mesh resize

- **WHEN** a facility mesh changes size
- **THEN** its centre is updated or the spawned-scene test fails

### Requirement: The flame trench is an open excavation in the pad

The flame trench SHALL be an excavation in the authoritative terrain surface.
The deck and apron SHALL stop at the trench mouth instead of spanning it, and the
trench lining SHALL be grounded: every lining part either rests on the excavated
floor or spans from that floor to the pad plane. The deflector SHALL rest on the
excavated floor and SHALL stay below the pad plane.

The excavation SHALL NOT cut the launch point. Its near edge SHALL clear the
terrain normal's finite-difference footprint so the resting height, launch
attitude and flight baselines are unchanged.

#### Scenario: Spawned scene

- **WHEN** the procedural pad is spawned at a site
- **THEN** no apron or deck slab overlaps the trench mouth, the lining has one
  floor and four walls, and the deflector sits on the floor below the pad plane

#### Scenario: Launch point

- **WHEN** the excavation is applied to the terrain authority
- **THEN** the height and the surface normal at the launch point are unchanged,
  and the trench mouth starts beyond the hold-down clamps

#### Scenario: Consumers agree

- **WHEN** the mesh, collision and radar altitude sample the pad
- **THEN** they all see the same excavation, because the cut is applied after the
  site grade in the one terrain authority

### Requirement: The rendered terrain yields to the covering structure

A structure that spans ground the streamed mesh cannot resolve SHALL be able to
declare a mesh cutout. The terrain mesh SHALL omit only cells that lie entirely
inside the declared cap, so the opening is a subset of the cap and never escapes
the structure covering it. Coarse patches whose cells do not fit inside the cap
SHALL NOT be cut. The authoritative height field SHALL NOT change: collision,
radar altitude and physics continue to sample the excavation.

The structure SHALL span the cap, and the cap SHALL be large enough that the
guaranteed opening — the cap minus half a cell diagonal at the finest streamed
LOD — still exposes everything the structure covers.

#### Scenario: Launch hardstand

- **WHEN** the launch facility is spawned at a site
- **THEN** the hardstand registers its cutout, the terrain mesh under it loses
  only cells inside the cap, and the below-grade trench is visible

#### Scenario: No facility

- **WHEN** a site has no presented facility
- **THEN** no cutout is registered and the terrain mesh is unchanged

#### Scenario: Distant view

- **WHEN** the pad is covered only by patches coarser than the cap
- **THEN** no cell is removed and the planet keeps a closed surface

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

