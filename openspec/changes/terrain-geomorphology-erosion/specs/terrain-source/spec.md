## ADDED Requirements

### Requirement: Terrain authority composes one eroded hydrology field

The system SHALL compose the deterministic erosion/hydrology field into the single authoritative `TerrainSource`, so collision, mesh generation, and surface-material consumers all sample the same eroded elevation and hydrology rather than a separate erosion path.

#### Scenario: Single elevation authority

- **WHEN** any consumer needs a terrain height, moisture, or river strength
- **THEN** it samples the one composed terrain authority and no second elevation implementation is introduced

#### Scenario: Erosion is configured and validated at construction

- **WHEN** the erosion composition is constructed with non-physical or numerically undefined parameters
- **THEN** construction fails before any terrain is sampled

### Requirement: Mesh heights are erosion-consistent and seam-safe

The presentation height used for cube-sphere mesh generation SHALL be consistent with the authoritative physical height field at the patch's own level, and matching geographic samples on adjacent patches SHALL produce identical heights so LOD transitions and patch boundaries remain crack-free.

#### Scenario: Render and collision agree

- **WHEN** a location is sampled for both collision and rendered patch geometry at the same level
- **THEN** the two heights agree

#### Scenario: Shared edge sample is identical

- **WHEN** two adjacent patches share a boundary sample at the same level
- **THEN** both patches compute the same mesh height at that sample

### Requirement: Hydrology signals are shared across consumers

The system SHALL expose flow-derived moisture and river-channel strength from the authoritative terrain source so river presentation and wet-biome selection consume the hydrology that carved the surface, and SHALL keep moisture and river strength normalized to `[0, 1]`.

#### Scenario: River presentation uses carved hydrology

- **WHEN** presentation selects river geometry or a wet biome
- **THEN** it reads moisture and river strength from the same authority that produced the eroded height

#### Scenario: Signals stay normalized

- **WHEN** any consumer reads moisture or river-channel strength
- **THEN** the value lies within `[0, 1]`

## MODIFIED Requirements

### Requirement: Procedural generation is deterministic

The system SHALL generate procedural terrain deterministically from a seed, coordinates, resolution, and parameters, with identical inputs producing identical output. This includes any composed erosion and hydrology stage, whose output SHALL be independent of evaluation order and of cache history.

#### Scenario: Deterministic regeneration

- **WHEN** the same seed, coordinates, and resolution are used to generate terrain twice
- **THEN** the two outputs are identical

#### Scenario: Generation independence from runtime

- **WHEN** terrain is generated
- **THEN** the result does not depend on frame rate, spawn order, or camera movement

#### Scenario: Erosion determinism and cache history

- **WHEN** the composed source is queried again after a cached erosion tile has been evicted
- **THEN** the height, moisture, and river strength are identical to their first values
