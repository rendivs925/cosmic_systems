## ADDED Requirements

### Requirement: Patch geometry renders the eroded seam-safe surface

The system SHALL build rendered patch geometry from the erosion-consistent mesh height field, so eroded ridgelines, talus slopes, and carved valleys are visible and adjacent patches at equal or differing LOD remain crack-free.

#### Scenario: Eroded relief is visible

- **WHEN** a patch overlaps an eroded region
- **THEN** its rendered geometry reflects the eroded height field rather than the un-eroded analytic sculpt

#### Scenario: No cracks at shared boundaries

- **WHEN** two adjacent rendered patches sample a shared boundary
- **THEN** the rendered surfaces meet without a gap or T-vertex crack

### Requirement: Materials and scatter consume drainage signals

The system SHALL derive river and wet-biome appearance from the authoritative moisture and river-channel-strength signals, so visible drainage matches the hydrology that carved the surface.

#### Scenario: River channels read as water

- **WHEN** river-channel strength indicates a channel
- **THEN** the rendered surface uses the river appearance (darker, smoother, blue-shifted) for that location

#### Scenario: Wet biomes follow moisture

- **WHEN** moisture rises along a drainage network
- **THEN** the selected material and vegetation reflect the wetter biome

## MODIFIED Requirements

### Requirement: Planetary surface materials are physically based

The system SHALL provide PBR materials whose properties (albedo, roughness, normal)
vary continuously by altitude, slope, moisture, latitude, biome, and river-channel
strength. Terrain surface materials SHALL blend bounded ground layers (grass, soil,
rock, sand, snow) with per-layer PBR texture sets, triplanar projection on steep
faces, and macro, micro, and near-camera detail overlays. Layer weights MUST derive
from authoritative terrain inputs and MUST remain presentation-only.

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

#### Scenario: Material variation by drainage

- **WHEN** a location lies in a carved river channel or a wet, high-moisture basin
- **THEN** its albedo, roughness, and normal reflect the wet or submerged appearance derived from the authoritative hydrology signals
