## ADDED Requirements

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
