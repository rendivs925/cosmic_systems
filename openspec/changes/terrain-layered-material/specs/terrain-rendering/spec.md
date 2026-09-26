## MODIFIED Requirements

### Requirement: Planetary surface materials are physically based

The system SHALL provide PBR materials whose properties (albedo, roughness, normal)
vary continuously by altitude, slope, moisture, latitude, and biome. Terrain surface
materials SHALL blend bounded ground layers (grass, soil, rock, sand, snow) with
per-layer PBR texture sets, triplanar projection on steep faces, and macro, micro,
and near-camera detail overlays. Layer weights MUST derive from authoritative terrain
inputs and MUST remain presentation-only.

#### Scenario: Material variation by biome

- **WHEN** a patch is in a mountain biome
- **THEN** its material uses rocky albedo/normal/roughness maps distinct from plains
  or ocean biomes

#### Scenario: Material variation by altitude

- **WHEN** a patch is above the snow line
- **THEN** its blended albedo shifts toward white and its roughness decreases

#### Scenario: Layered blend across a gradient

- **WHEN** a rendered patch spans a height, slope, or moisture gradient
- **THEN** its material blends grass, soil, rock, sand, and snow continuously with
  per-layer PBR texture sets and no hard bands

#### Scenario: Detail overlay fade

- **WHEN** a rendered terrain surface is viewed near and then far
- **THEN** its macro, micro, and near-camera detail overlays fade with camera
  distance without changing patch geometry, streaming, or LOD
