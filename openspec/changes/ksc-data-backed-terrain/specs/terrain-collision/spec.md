## MODIFIED Requirements

### Requirement: Collision sampling uses the shared terrain source

Collision SHALL sample height from the shared `TerrainSource` height function,
consistent with the render terrain. Valid measured local DEM coverage SHALL be
used by collision, altitude, normals, and landing evaluation at the same
coordinates as terrain rendering.

#### Scenario: Consistency with render terrain
- **WHEN** collision height is sampled at a position
- **THEN** it matches the render terrain surface within the configured collision resolution

#### Scenario: Near-surface resolution increase
- **WHEN** the rocket approaches a landing region
- **THEN** the collision resolution increases for that region

#### Scenario: Local DEM collision
- **WHEN** a KSC query lies within valid local measured DEM coverage
- **THEN** collision uses that measured height rather than a visual-only terrain mesh
