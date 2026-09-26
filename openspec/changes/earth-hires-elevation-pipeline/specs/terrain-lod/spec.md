## ADDED Requirements

### Requirement: LOD uses per-tile geometric error from the elevation payload

Terrain LOD selection SHALL derive a patch's geometric error from the
corresponding elevation payload tile's declared error and elevation range,
rather than only from a planet-wide conservative envelope.

#### Scenario: Detailed region refines earlier

- **WHEN** a patch covers a payload tile with high measured relief
- **THEN** its projected error causes refinement at a greater distance than a
  low-relief patch at the same level

#### Scenario: Coarse coverage stays conservative

- **WHEN** a patch has no high-resolution payload tile
- **THEN** its geometric error uses the declared conservative fallback bound
