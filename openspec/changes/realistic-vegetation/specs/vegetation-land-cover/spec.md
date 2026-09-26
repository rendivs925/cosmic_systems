## Purpose

Consumes an offline measured land-cover package (for example ESA WorldCover) as a presentation-only resource that drives vegetation species mix and density without affecting terrain height, collision, altitude, or physics.

## ADDED Requirements

### Requirement: Offline measured land-cover package

The system SHALL load a versioned, offline measured land-cover package with documented provenance and coverage, and SHALL provide a deterministic fallback when the package is absent or a sample is outside its coverage.

#### Scenario: Package drives land cover

- **WHEN** a valid land-cover package is present
- **THEN** samples inside its coverage report the package's land-cover class

#### Scenario: Missing package falls back deterministically

- **WHEN** the land-cover package is absent or a coordinate is outside its coverage
- **THEN** the system uses the fallback climate-derived cover and remains deterministic

### Requirement: Land cover is presentation-only

The system SHALL use measured land cover only for vegetation placement and species selection. Land cover MUST NOT feed terrain height, collision, radar altitude, surface normals, or any physics or simulation authority.

#### Scenario: Physics is unaffected

- **WHEN** the land-cover package is present or absent
- **THEN** terrain height samples, collision results, and rocket trajectories are identical

#### Scenario: Collision does not read land cover

- **WHEN** terrain collision queries a coordinate
- **THEN** its result does not depend on the land-cover package

### Requirement: Land cover drives species mix and density

The system SHALL translate land-cover classes into a bounded species mix and a density in the range `[0, 1]` consumed by vegetation placement.

#### Scenario: Forest, grass, and bare differ

- **WHEN** land cover is forest, grassland, and bare ground at otherwise similar sites
- **THEN** forest yields the highest canopy density, grassland yields grass-dominated low density, and bare ground yields little or no vegetation

#### Scenario: Classification is deterministic

- **WHEN** the same coordinate and package are sampled twice
- **THEN** the resulting species mix and density are identical
