## MODIFIED Requirements

### Requirement: Cube-sphere patches render as Bevy meshes

The system SHALL convert each ready terrain patch into a Bevy `Mesh` asset and spawn it with a `Material` so the patch appears in the rendered scene. For close-range patches that support vegetation, the rendered patch SHALL also include a single merged vegetation/scatter mesh produced from deterministic placement, per-species geometry, and land cover. The active terrain leaf set SHALL provide the bound planet's visible surface in rocket mode, including coarse global tiles outside the local detail region.

#### Scenario: Patch mesh spawned on ready

- **WHEN** a terrain patch transitions to the `Ready` state in the streaming lifecycle
- **THEN** a corresponding Bevy `Mesh` is created/updated and an entity with `Mesh3d` and `Material3d` is spawned

#### Scenario: Patch mesh despawned on evict

- **WHEN** a non-visible terrain patch is evicted from the streaming cache
- **THEN** its Bevy mesh entity is despawned and the mesh asset is released without leaving its parent coverage absent

#### Scenario: Merged vegetation mesh accompanies a close patch

- **WHEN** a ready patch is at or finer than the vegetation LOD threshold and its ground is vegetated
- **THEN** its merged vegetation mesh is spawned with the patch in the same local frame, as one mesh rather than per-plant entities

#### Scenario: Whole-planet presentation

- **WHEN** rocket mode presents a bound planet at any supported flight altitude
- **THEN** the rendered terrain hierarchy supplies the planet silhouette and horizon without a separate bound-planet globe proxy

### Requirement: LOD transitions are crack-free in rendering

The system SHALL render adjacent patches at different LOD levels, including patches joined across cube-face boundaries, without visible cracks or T-vertex artifacts.

#### Scenario: Skirt geometry stitches edges

- **WHEN** two adjacent patches have different LOD levels
- **THEN** the finer patch's skirt vertices align with the coarser patch's edge vertices and no gaps appear

#### Scenario: Neighbor stitching

- **WHEN** two adjacent visible terrain leaves differ in LOD or share a cube-face edge
- **THEN** their shared boundary is stitched or otherwise covered without a visible gap

#### Scenario: No vertex popping

- **WHEN** the camera moves and LOD levels change
- **THEN** vertices morph smoothly or transition without sudden position jumps
