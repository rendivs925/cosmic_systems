## MODIFIED Requirements

### Requirement: Cube-sphere patches render as Bevy meshes

The system SHALL convert each ready terrain patch into a Bevy `Mesh` asset and spawn it with a `Material` so the patch appears in the rendered scene. A patch whose terrain data and LOD remain valid SHALL retain its generated geometry instead of being rebuilt every update.

#### Scenario: Patch mesh spawned on ready

- **WHEN** a terrain patch transitions to the `Ready` state in the streaming lifecycle
- **THEN** a corresponding Bevy `Mesh` is created/updated and an entity with `Mesh3d` and `Material3d` is spawned

#### Scenario: Patch mesh despawned on evict

- **WHEN** a terrain patch is evicted from the streaming cache
- **THEN** its Bevy mesh entity is despawned and the mesh asset is released

#### Scenario: Stable visible patch reuse

- **WHEN** a visible patch remains at the same LOD with unchanged terrain data
- **THEN** the renderer reuses its existing geometry and mesh without regeneration

### Requirement: LOD transitions are crack-free in rendering

The system SHALL render adjacent patches at different LOD levels without visible cracks or T-vertex artifacts, while preserving source elevation detail within each patch.

#### Scenario: Skirt geometry stitches edges

- **WHEN** two adjacent patches have different LOD levels
- **THEN** the finer patch's skirt vertices align with the coarser patch's edge vertices and no gaps appear

#### Scenario: No vertex popping

- **WHEN** the camera moves and LOD levels change
- **THEN** vertices morph smoothly or transition without sudden position jumps

#### Scenario: Near-surface relief

- **WHEN** the camera observes terrain near the surface outside a level site
- **THEN** the rendered mesh reflects the active shared source's elevation variation
