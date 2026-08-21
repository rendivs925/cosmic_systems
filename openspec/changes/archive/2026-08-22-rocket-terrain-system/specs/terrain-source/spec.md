## Purpose

Defines the terrain data abstraction for the rocket simulator: a `TerrainSource` interface separating terrain data from rendering and collision, with procedural, heightmap, and DEM implementations planned so the renderer never depends on the data source.

## ADDED Requirements

### Requirement: Terrain data is separate from rendering and collision

The system SHALL model terrain as a data source (`TerrainSource`) with render mesh and collision as separate consumers, so replacing the data source does not rewrite the renderer or collision code.

#### Scenario: Source-to-mesh independence

- **WHEN** the terrain source implementation changes (procedural to DEM)
- **THEN** the render mesh and collision systems continue to work unchanged

#### Scenario: Shared height function

- **WHEN** any consumer needs terrain height at a position
- **THEN** it calls the shared terrain height function provided by the active source

### Requirement: Procedural generation is deterministic

The system SHALL generate procedural terrain deterministically from a seed, coordinates, resolution, and parameters, with identical inputs producing identical output.

#### Scenario: Deterministic regeneration

- **WHEN** the same seed, coordinates, and resolution are used to generate terrain twice
- **THEN** the two outputs are identical

#### Scenario: Generation independence from runtime

- **WHEN** terrain is generated
- **THEN** the result does not depend on frame rate, spawn order, or camera movement

### Requirement: Heightmap and DEM sources are supported

The system SHALL support heightmap-based terrain sources and provide an interface for real planetary DEM data without rewiring the renderer.

#### Scenario: Heightmap source

- **WHEN** a heightmap terrain source is active
- **THEN** terrain height is sampled from the heightmap data

#### Scenario: DEM-ready interface

- **WHEN** real DEM data becomes available
- **THEN** a DEM terrain source can be added behind the same `TerrainSource` interface

### Requirement: Launch-site patches reuse the shared source

Existing localized launch-site patches (KSC, RTLS, drone ship, lunar) SHALL continue to exist as detailed site objects but MUST sample their height from the shared terrain source architecture rather than a bespoke path.

#### Scenario: Site height via shared source

- **WHEN** a launch-site patch is rendered or queried for collision
- **THEN** its height comes from the shared terrain source interface

#### Scenario: Existing sites preserved

- **WHEN** the terrain source architecture is introduced
- **THEN** the existing launch-site patches remain available as before