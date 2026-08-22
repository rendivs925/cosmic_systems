## Purpose

Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the streaming manager, with PBR shaders for planetary surfaces so the rocket sees procedural terrain from orbit to surface.

## ADDED Requirements

### Requirement: Cube-sphere patches render as Bevy meshes

The system SHALL convert each ready terrain patch into a Bevy `Mesh` asset and spawn it with a `Material` so the patch appears in the rendered scene.

#### Scenario: Patch mesh spawned on ready

- **WHEN** a terrain patch transitions to the `Ready` state in the streaming lifecycle
- **THEN** a corresponding Bevy `Mesh` is created/updated and an entity with `Mesh3d` and `Material3d` is spawned

#### Scenario: Patch mesh despawned on evict

- **WHEN** a terrain patch is evicted from the streaming cache
- **THEN** its Bevy mesh entity is despawned and the mesh asset is released

### Requirement: LOD transitions are crack-free in rendering

The system SHALL render adjacent patches at different LOD levels without visible cracks or T-vertex artifacts.

#### Scenario: Skirt geometry stitches edges

- **WHEN** two adjacent patches have different LOD levels
- **THEN** the finer patch's skirt vertices align with the coarser patch's edge vertices and no gaps appear

#### Scenario: No vertex popping

- **WHEN** the camera moves and LOD levels change
- **THEN** vertices morph smoothly or transition without sudden position jumps

### Requirement: Planetary surface materials are physically based

The system SHALL provide PBR materials whose properties (albedo, roughness, normal) vary by altitude, biome, slope, and temperature.

#### Scenario: Material variation by biome

- **WHEN** a patch is in a mountain biome
- **THEN** its material uses rocky albedo/normal maps distinct from plains or ocean biomes

#### Scenario: Material variation by altitude

- **WHEN** a patch is above the snow line
- **THEN** its albedo shifts toward white and roughness decreases

### Requirement: Rendering uses local-origin coordinates

The system SHALL render terrain patches relative to a floating origin near the camera to avoid f32 precision artifacts at planetary scale.

#### Scenario: Origin re-centering

- **WHEN** the camera moves beyond a threshold from the current render origin
- **THEN** the render origin shifts and all patch transforms update without visual discontinuity

#### Scenario: Precision at high altitude

- **WHEN** the rocket is at orbital altitude (100+ km)
- **THEN** terrain patches render without z-fighting or vertex jitter