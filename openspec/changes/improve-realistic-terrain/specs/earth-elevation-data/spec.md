## Purpose

Lets rocket mode use locally supplied real Earth elevation data while keeping rendering, collision, spawning, and map queries consistent when data coverage is partial.

## ADDED Requirements

### Requirement: Local SRTM elevation selection

The system SHALL select locally supplied SRTM elevation tiles as Earth's terrain height authority when DEM support and an elevation-data directory are configured.

#### Scenario: Available local coverage

- **WHEN** a rocket-mode height query falls in a configured local SRTM tile
- **THEN** rendering, collision, spawning, and terrain-map consumers receive the same interpolated SRTM height

#### Scenario: No configured elevation directory

- **WHEN** DEM support or its local elevation-data directory is not configured
- **THEN** Earth continues using deterministic procedural terrain without startup failure

### Requirement: Deterministic partial-coverage fallback

The system SHALL use deterministic procedural terrain when configured elevation data does not cover a queried Earth coordinate.

#### Scenario: Missing SRTM tile

- **WHEN** a query falls outside the supplied SRTM tile set
- **THEN** it returns the deterministic procedural fallback height without changing the active terrain consumer

#### Scenario: Repeated mixed-coverage query

- **WHEN** the same covered or uncovered coordinate is queried repeatedly
- **THEN** it returns bitwise-identical heights for the active terrain configuration

### Requirement: Elevation tile reuse

The system SHALL reuse a loaded elevation tile for repeated queries until its cache eviction policy removes it.

#### Scenario: Repeated query within a tile

- **WHEN** multiple height queries fall within the same resident elevation tile
- **THEN** the tile is served from cache without reparsing its source file
