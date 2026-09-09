## MODIFIED Requirements

### Requirement: Terrain data is separate from rendering and collision

The system SHALL model terrain as a data source (`TerrainSource`) with render
mesh and collision as separate consumers, so replacing the data source does not
rewrite the renderer or collision code. Valid measured local DEM coverage SHALL
be composed by that shared source rather than by a visual-only mesh.

#### Scenario: Source-to-mesh independence
- **WHEN** the terrain source implementation changes (procedural to DEM)
- **THEN** the render mesh and collision systems continue to work unchanged

#### Scenario: Shared height function
- **WHEN** any consumer needs terrain height at a position
- **THEN** it calls the shared terrain height function provided by the active source

#### Scenario: Measured local elevation
- **WHEN** a query falls within valid local DEM coverage
- **THEN** the shared terrain source supplies measured local elevation instead of a
  separate presentation height
