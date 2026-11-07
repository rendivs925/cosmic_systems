# planetary-terrain-material Specification

## Purpose
Defines the planet-terrain visual layers that preserve a coherent global Earth appearance while adding close-range procedural surface detail on streamed terrain tiles.
## Requirements
### Requirement: Global imagery covers terrain hierarchy

The system SHALL apply configured global planetary imagery across the complete terrain hierarchy so distant terrain presents a continuous planet surface.

#### Scenario: Earth global texture at distance
- **WHEN** Earth is viewed outside the local high-detail region
- **THEN** the configured global Earth albedo remains mapped across visible coarse terrain tiles and the planetary horizon

#### Scenario: Imagery fallback
- **WHEN** no global imagery asset is available for a planet
- **THEN** terrain remains visibly continuous using a deterministic material fallback

### Requirement: Local detail blends with global appearance

The system SHALL blend global imagery with tile-local procedural material detail according to terrain LOD or projected resolution without a visible color, normal, or roughness seam. Ground-layer detail SHALL modulate the continuous geographic base rather than replace it, so a tile whose layer or imagery data becomes ready or unavailable MUST NOT step its base color relative to a neighbour.

#### Scenario: Close-range terrain appearance
- **WHEN** a terrain tile is refined into the close-range detail band
- **THEN** local biome, slope, and procedural surface detail contribute to its appearance

#### Scenario: Refinement material transition
- **WHEN** a terrain tile refines or coarsens across the material-detail threshold
- **THEN** global and local material layers transition continuously without a visible pop

#### Scenario: Asynchronous readiness keeps the base continuous
- **WHEN** a tile's layer maps or imagery become ready after the tile was first drawn, or are missing next to a neighbour that has them
- **THEN** the geographic base color is unchanged and only the added detail varies

### Requirement: Body-fixed texture phase is invariant under reference-frame changes

The system SHALL sample body-fixed ground detail so that the texture phase at a fixed point on the ground is unchanged when the render origin is rebased or the planet rotates. The per-axis fractional anchors that restore f64 precision MUST combine with the render-relative body coordinate such that the integer part of the absolute scaled coordinate cancels under texture wrapping.

#### Scenario: Projection is stable across origin rebases
- **WHEN** the render origin is rebased while the vehicle stays fixed over the ground
- **THEN** the reconstructed absolute body-fixed phase at that point is unchanged

#### Scenario: Projection is stable under planetary rotation
- **WHEN** the body rotates under time acceleration while the vehicle holds station
- **THEN** the ground texture phase stays pinned to the surface without jump or swim

### Requirement: Terrain material frame invalidates on any shader-relevant change

The system SHALL refresh every terrain material whenever any value it derives from authoritative state changes, including the f64-derived texture anchors. A cache comparison MUST include every such value; comparing only a reduced f32 projection of the state and skipping an update is not permitted.

#### Scenario: Anchor change forces an update
- **WHEN** a state change alters the derived texture anchors but leaves the compared position and rotation unchanged at f32 precision
- **THEN** the material's anchors are still updated

#### Scenario: Stationary scene does not churn
- **WHEN** the frame, origin, and rotation are unchanged
- **THEN** no terrain material is rewritten

### Requirement: Surface classification ignores artificial site grading

Terrain appearance classification SHALL sample the natural surface rather than
an artificial site grade. A graded launch platform SHALL keep the slope-derived
material of the land around it, while geometry, collision, radar altitude and
elevation bands (shoreline, snow) continue to follow the graded surface actually
shown.

#### Scenario: Graded pad on a hillside

- **WHEN** a launch platform is graded flat on a slope steep enough to classify
  as rock
- **THEN** the pad's surface material keeps the hillside's rock classification
  instead of turning into a flat-ground disc

#### Scenario: Geometry remains graded

- **WHEN** the same platform is sampled for collision or the rendered mesh
- **THEN** the surface is level and the natural slope is not restored

#### Scenario: Elevation bands follow the shown surface

- **WHEN** the grade changes the height enough to cross a shoreline or snow band
- **THEN** that band still follows the graded height actually shown

#### Scenario: No artificial grade

- **WHEN** a source applies no grading
- **THEN** the appearance height equals the geometry height and classification
  is unchanged

