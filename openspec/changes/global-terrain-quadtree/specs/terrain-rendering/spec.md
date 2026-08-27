## MODIFIED Requirements

### Requirement: Cube-sphere patches render as Bevy meshes

The system SHALL convert each ready terrain patch into a Bevy `Mesh` asset and spawn it with a material. The active terrain leaf set SHALL provide the bound planet's visible surface in rocket mode, including coarse global tiles outside the local detail region.

#### Scenario: Patch mesh spawned on ready
- **WHEN** a terrain patch transitions to the `Ready` state in the streaming lifecycle
- **THEN** a corresponding Bevy `Mesh` is created or updated and an entity with `Mesh3d` and `Material3d` is spawned

#### Scenario: Patch mesh despawned on evict
- **WHEN** a non-visible terrain patch is evicted from the streaming cache
- **THEN** its Bevy mesh entity is despawned and the mesh asset is released without leaving its parent coverage absent

#### Scenario: Whole-planet presentation
- **WHEN** rocket mode presents a bound planet at any supported flight altitude
- **THEN** the rendered terrain hierarchy supplies the planet silhouette and horizon without a separate bound-planet globe proxy

### Requirement: LOD transitions are crack-free in rendering

The system SHALL render adjacent patches at different LOD levels, including patches joined across cube-face boundaries, without visible cracks or T-vertex artifacts.

#### Scenario: Neighbor stitching
- **WHEN** two adjacent visible terrain leaves differ in LOD or share a cube-face edge
- **THEN** their shared boundary is stitched or otherwise covered without a visible gap

#### Scenario: No vertex popping
- **WHEN** the camera moves and terrain leaves refine or coarsen
- **THEN** the transition occurs without sudden visible surface-position jumps
