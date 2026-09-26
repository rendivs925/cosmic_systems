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

The system SHALL provide PBR materials whose properties (albedo, roughness, normal) vary by altitude, biome, slope, temperature, moisture, and river-channel strength.

#### Scenario: Material variation by biome

- **WHEN** a patch is in a mountain biome
- **THEN** its material uses rocky albedo/normal maps distinct from plains or ocean biomes

#### Scenario: Material variation by altitude

- **WHEN** a patch is above the snow line
- **THEN** its albedo shifts toward white and roughness decreases

#### Scenario: Material variation by drainage

- **WHEN** a location lies in a carved river channel or a wet, high-moisture basin
- **THEN** its albedo, roughness, and normal reflect the wet or submerged appearance derived from the authoritative hydrology signals
