## ADDED Requirements

### Requirement: Terrain participates in shared lighting and aerial perspective

Terrain rendering SHALL cast and receive the shared ephemeris-derived
directional shadows and SHALL derive aerial perspective from the shared
atmospheric optics, while keeping terrain geometry, LOD, collision, streaming,
and render-origin authority unchanged.

#### Scenario: Terrain casts and receives directional shadow

- **WHEN** a sunlit terrain patch occludes the shared directional Sun
- **THEN** it both casts shadow onto other geometry and receives shadow from
  geometry in front of it, excluding the enclosing far-field globe

#### Scenario: Terrain aerial perspective matches the sky

- **WHEN** a terrain fragment is viewed through a long air path
- **THEN** its in-scattered and transmitted colour is computed from the same
  atmospheric optics used by the sky and does not use an independent fog colour
