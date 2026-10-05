# Terrain Collision Specification

## Purpose

Defines collision terrain for the rocket, separated from render terrain: accurate altitude, surface normal, slope, ground contact, and landing detection near the rocket, without a full-planet physics mesh.
## Requirements
### Requirement: Collision terrain is separate from render terrain

The system SHALL maintain collision terrain separately from render terrain, with higher resolution near the rocket and no full-planet physics mesh.

#### Scenario: Near-rocket collision detail

- **WHEN** the rocket is close to a surface region
- **THEN** collision height and normal are computed at an appropriate resolution for that region

#### Scenario: No global physics mesh

- **WHEN** the simulation runs
- **THEN** it does not create a full-planet high-resolution physics mesh

### Requirement: Ground contact and landing detection

The system SHALL detect ground contact and support landing: altitude above terrain, surface normal, slope, touchdown, and crash conditions.

#### Scenario: Radar altitude

- **WHEN** the rocket is near terrain
- **THEN** the system computes its altitude above the terrain surface along the surface normal

#### Scenario: Slope and normal

- **WHEN** a surface is queried
- **THEN** the surface normal and slope at the position are available

#### Scenario: Landing detection

- **WHEN** the rocket contacts the ground with low enough velocity
- **THEN** the system reports a landed state

#### Scenario: Crash detection

- **WHEN** the rocket contacts the ground with excessive velocity or attitude
- **THEN** the system reports a crash condition

### Requirement: Collision sampling uses the shared terrain source

Collision SHALL sample height from the same composed `TerrainSource` surface used by rendered terrain tiles, including global base elevation, available local DEM elevation, and active procedural detail.

#### Scenario: Consistency with render terrain
- **WHEN** collision height is sampled at a position represented by a rendered terrain tile
- **THEN** it matches that tile's terrain surface within the configured collision resolution

#### Scenario: Near-surface resolution increase
- **WHEN** the rocket approaches a landing region
- **THEN** collision resolution increases for that region without creating a discontinuity with the coarser terrain surface

