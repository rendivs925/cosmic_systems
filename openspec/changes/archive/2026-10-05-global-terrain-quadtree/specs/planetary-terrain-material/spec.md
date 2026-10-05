## Purpose

Defines the planet-terrain visual layers that preserve a coherent global Earth appearance while adding close-range procedural surface detail on streamed terrain tiles.

## ADDED Requirements

### Requirement: Global imagery covers terrain hierarchy

The system SHALL apply configured global planetary imagery across the complete terrain hierarchy so distant terrain presents a continuous planet surface.

#### Scenario: Earth global texture at distance
- **WHEN** Earth is viewed outside the local high-detail region
- **THEN** the configured global Earth albedo remains mapped across visible coarse terrain tiles and the planetary horizon

#### Scenario: Imagery fallback
- **WHEN** no global imagery asset is available for a planet
- **THEN** terrain remains visibly continuous using a deterministic material fallback

### Requirement: Local detail blends with global appearance

The system SHALL blend global imagery with tile-local procedural material detail according to terrain LOD or projected resolution without a visible color, normal, or roughness seam.

#### Scenario: Close-range terrain appearance
- **WHEN** a terrain tile is refined into the close-range detail band
- **THEN** local biome, slope, and procedural surface detail contribute to its appearance

#### Scenario: Refinement material transition
- **WHEN** a terrain tile refines or coarsens across the material-detail threshold
- **THEN** global and local material layers transition continuously without a visible pop
