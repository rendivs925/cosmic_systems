## ADDED Requirements

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
