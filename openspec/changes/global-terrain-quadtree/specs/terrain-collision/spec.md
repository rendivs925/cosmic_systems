## MODIFIED Requirements

### Requirement: Collision sampling uses the shared terrain source

Collision SHALL sample height from the same composed `TerrainSource` surface used by rendered terrain tiles, including global base elevation, available local DEM elevation, and active procedural detail.

#### Scenario: Consistency with render terrain
- **WHEN** collision height is sampled at a position represented by a rendered terrain tile
- **THEN** it matches that tile's terrain surface within the configured collision resolution

#### Scenario: Near-surface resolution increase
- **WHEN** the rocket approaches a landing region
- **THEN** collision resolution increases for that region without creating a discontinuity with the coarser terrain surface
