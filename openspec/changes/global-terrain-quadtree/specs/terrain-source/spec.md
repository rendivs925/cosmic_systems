## ADDED Requirements

### Requirement: Layered terrain elevation is coherent across LOD

The system SHALL compose base planetary shape, global elevation, optional local DEM elevation, and bounded procedural detail through the active shared terrain source. The same geographic coordinate SHALL resolve to a continuous terrain surface independent of the current render LOD.

#### Scenario: Parent-child elevation agreement
- **WHEN** a parent terrain tile is replaced by child tiles at the same geographic boundary
- **THEN** their shared edge samples resolve to the same terrain height within the configured numerical tolerance

#### Scenario: Procedural detail fade
- **WHEN** procedural detail is unavailable or intentionally omitted at a coarse LOD
- **THEN** its contribution fades continuously to the shared base surface rather than producing a height step

#### Scenario: DEM fallback
- **WHEN** local DEM coverage is unavailable for a coordinate
- **THEN** the terrain source remains deterministic and supplies its configured global or procedural fallback height
