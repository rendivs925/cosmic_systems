## MODIFIED Requirements

### Requirement: Cube-sphere patches render as Bevy meshes

The system SHALL convert each ready terrain patch into a Bevy `Mesh` asset and spawn it with a `Material` so the patch appears in the rendered scene. For close-range patches that support vegetation, the rendered patch SHALL also include a single merged vegetation/scatter mesh produced from deterministic placement, per-species geometry, and land cover.

#### Scenario: Patch mesh spawned on ready

- **WHEN** a terrain patch transitions to the `Ready` state in the streaming lifecycle
- **THEN** a corresponding Bevy `Mesh` is created/updated and an entity with `Mesh3d` and `Material3d` is spawned

#### Scenario: Patch mesh despawned on evict

- **WHEN** a terrain patch is evicted from the streaming cache
- **THEN** its Bevy mesh entity is despawned and the mesh asset is released

#### Scenario: Merged vegetation mesh accompanies a close patch

- **WHEN** a ready patch is at or finer than the vegetation LOD threshold and its ground is vegetated
- **THEN** its merged vegetation mesh is spawned with the patch in the same local frame, as one mesh rather than per-plant entities

## ADDED Requirements

### Requirement: Rendered vegetation consumes placement, species, and land cover

The system SHALL generate the rendered vegetation mesh from the deterministic placement, species, and land-cover signals, and SHALL NOT read rendered transforms, camera state, or frame timing as inputs to placement.

#### Scenario: Rendered scatter matches placement signals

- **WHEN** a patch's merged vegetation mesh is built
- **THEN** its plants correspond to the accepted placement candidates, their selected species, and the local land cover

#### Scenario: Presentation does not perturb simulation

- **WHEN** vegetation rendering changes or is disabled
- **THEN** terrain height, collision, and rocket physics results are unchanged

### Requirement: Rendered vegetation is deterministic and bounded

The system SHALL produce identical merged vegetation meshes for identical patch identity and seed, and SHALL keep each patch's vegetation geometry within the existing scatter budgets.

#### Scenario: Repeated render build is identical

- **WHEN** the same patch's vegetation mesh is built twice
- **THEN** the two meshes are identical

#### Scenario: Budgets are respected

- **WHEN** a patch's vegetation mesh is built on a fully vegetated site
- **THEN** its vertex and index counts stay within the configured per-patch budget
