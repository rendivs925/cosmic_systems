## Purpose

Loads real planetary heightmap data (NASA SRTM for Earth, LRO LOLA for Moon, MOLA for Mars) behind the `TerrainSource` interface so the renderer and collision systems consume real topography without code changes.

## ADDED Requirements

### Requirement: SRTM heightmap loading for Earth

The system SHALL load SRTM (Shuttle Radar Topography Mission) 1-arcsecond or 3-arcsecond tiles and serve height queries at arbitrary lat/lon.

#### Scenario: SRTM tile fetch

- **WHEN** a height query falls within an SRTM tile not yet loaded
- **THEN** the system fetches/parses the tile and caches it

#### Scenario: SRTM bilinear interpolation

- **WHEN** a query position falls between grid posts
- **THEN** height is bilinearly interpolated from the four nearest posts

### Requirement: LRO LOLA heightmap loading for Moon

The system SHALL load LRO LOLA (Lunar Orbiter Laser Altimeter) DEM data and serve height queries.

#### Scenario: LRO tile fetch

- **WHEN** a lunar height query falls within an unloaded LRO tile
- **THEN** the system fetches/parses the tile and caches it

#### Scenario: Polar coverage

- **WHEN** querying lunar polar regions
- **THEN** the system serves heights from the polar-stereographic LRO product

### Requirement: MOLA heightmap loading for Mars

The system SHALL load MOLA (Mars Orbiter Laser Altimeter) DEM data and serve height queries.

#### Scenario: MOLA tile fetch

- **WHEN** a Martian height query falls within an unloaded MOLA tile
- **THEN** the system fetches/parses the tile and caches it

### Requirement: Unified caching and streaming integration

The system SHALL cache loaded DEM tiles and integrate with the existing terrain streaming lifecycle (requested → loading → ready → cached → evicted).

#### Scenario: DEM tile memory budget

- **WHEN** resident DEM tiles exceed the configured memory budget
- **THEN** the system evicts least-recently-used tiles per the streaming policy

#### Scenario: DEM + procedural fallback

- **WHEN** a query falls outside available DEM coverage
- **THEN** the system falls back to procedural generation for that region

### Requirement: Deterministic height queries

The system SHALL return identical heights for identical lat/lon queries across runs.

#### Scenario: Repeatable sampling

- **WHEN** the same lat/lon is queried twice
- **THEN** the returned height is bitwise identical