# terrain-rendering Specification

## Purpose

Spawns GPU meshes and materials for cube-sphere LOD terrain patches from the streaming manager, with PBR shaders for planetary surfaces so the rocket sees procedural terrain from orbit to surface.
## Requirements
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

### Requirement: Planetary surface materials are physically based

The system SHALL provide PBR materials whose properties (albedo, roughness, normal)
vary continuously by altitude, slope, moisture, latitude, biome, and river-channel
strength. Terrain surface materials SHALL blend bounded ground layers (grass, soil,
rock, sand, snow) with per-layer PBR texture sets, triplanar projection on steep
faces, and macro, micro, and near-camera detail overlays. Layer weights MUST derive
from authoritative terrain inputs and MUST remain presentation-only.

#### Scenario: Material variation by biome

- **WHEN** a patch is in a mountain biome
- **THEN** its material uses rocky albedo/normal/roughness maps distinct from plains
  or ocean biomes

#### Scenario: Material variation by altitude

- **WHEN** a patch is above the snow line
- **THEN** its blended albedo shifts toward white and its roughness decreases

#### Scenario: Layered blend across a gradient

- **WHEN** a rendered patch spans a height, slope, or moisture gradient
- **THEN** its material blends grass, soil, rock, sand, and snow continuously with
  per-layer PBR texture sets and no hard bands

#### Scenario: Detail overlay fade

- **WHEN** a rendered terrain surface is viewed near and then far
- **THEN** its macro, micro, and near-camera detail overlays fade with camera
  distance without changing patch geometry, streaming, or LOD

#### Scenario: Material variation by drainage

- **WHEN** a location lies in a carved river channel or a wet, high-moisture basin
- **THEN** its albedo, roughness, and normal reflect the wet or submerged appearance derived from the authoritative hydrology signals

### Requirement: Rendering uses local-origin coordinates

The system SHALL render terrain patches relative to a floating origin near the camera to avoid f32 precision artifacts at planetary scale.

#### Scenario: Origin re-centering

- **WHEN** the camera moves beyond a threshold from the current render origin
- **THEN** the render origin shifts and all patch transforms update without visual discontinuity

#### Scenario: Precision at high altitude

- **WHEN** the rocket is at orbital altitude (100+ km)
- **THEN** terrain patches render without z-fighting or vertex jitter

### Requirement: Terrain scatter renders grounded procedural rocks

Terrain rendering SHALL present scattered rocks as procedurally generated,
varied, slope-grounded bodies with base contact darkening, produced within the
existing per-patch merged scatter mesh and its LOD count budget, without
changing terrain geometry, collision, streaming, render-origin, or terrain-source
authority.

#### Scenario: Rocks appear within the merged scatter mesh

- **WHEN** a close-range vegetated or rocky patch is prepared for rendering
- **THEN** its procedurally generated rocks are merged into the same per-patch
  scatter mesh as the other surface detail and rendered without spawning a
  separate entity per rock

#### Scenario: Rock presentation respects the patch budget

- **WHEN** a patch is rendered at a coarser LOD level
- **THEN** the number of rock bodies is reduced by the existing scatter LOD
  decimation and remains within the bounded per-patch rock count

#### Scenario: Rock grounding and occlusion are presentation only

- **WHEN** a rock is embedded and darkened against the slope
- **THEN** the operation only modifies the rock's presentation vertices and does
  not alter authoritative terrain height, normals, collision, or physics state

### Requirement: Terrain ocean patches present a wave-shaded water surface

Terrain patches that contain ocean SHALL spawn a sea-level water surface that
reuses the patch grid topology, shares the existing water material, and preserves
the normalized depth vertex channel, instead of a separate flat cap.

#### Scenario: Ocean patch spawns a wave-shaded surface

- **WHEN** a ready terrain patch contains ocean
- **THEN** it spawns a sea-level water surface whose normalized depth channel is
  preserved for shading and whose topology matches the patch grid

#### Scenario: Water surface stays within the patch lifetime

- **WHEN** a terrain patch is evicted
- **THEN** its water surface is released with the patch

#### Scenario: Coastline remains sealed

- **WHEN** the ocean surface meets the coastline
- **THEN** the surface continues to meet the land without holes

### Requirement: Terrain patch rivers follow the drainage network

Terrain patches crossed by an authoritative river channel SHALL build flow-directed
channel geometry with width scaled by discharge/flow accumulation and defined
banks, replacing the flat river ribbon.

#### Scenario: Flow-directed channel replaces ribbon

- **WHEN** a ready terrain patch is crossed by the authoritative drainage network
- **THEN** it spawns a flow-directed channel with banks rather than a flat ribbon

#### Scenario: Channel width reflects discharge

- **WHEN** the patch samples a higher discharge/flow accumulation
- **THEN** the generated channel is wider than at a lower discharge

#### Scenario: Dry patch allocates no channel

- **WHEN** a ready terrain patch has no authoritative drainage signal
- **THEN** no river channel geometry is allocated for that patch

### Requirement: Water presentation consumes authoritative terrain data only

Terrain water geometry SHALL derive its ocean depth, drainage alignment, and
discharge from the authoritative terrain source without maintaining a competing
hydrology or water field.

#### Scenario: Single drainage authority

- **WHEN** river geometry is built for a patch
- **THEN** its alignment and width come from the authoritative terrain source
  hydrology signal and not from an independent render-time computation

#### Scenario: Water does not feed terrain

- **WHEN** water geometry or materials are updated
- **THEN** no authoritative terrain height, collision, or streaming state changes

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

### Requirement: Terrain shading samples baked self-shadow and occlusion terms

Terrain rendering SHALL sample the baked self-shadow and ambient/sky-occlusion
terms produced from the authoritative height field and apply them to the direct
and indirect lighting contributions respectively. Sampling SHALL use the
existing terrain patch identity and material path and SHALL NOT re-run a
per-pixel real-time terrain shadow pass where the baked term is available.

#### Scenario: Direct light is scaled by self-shadow

- **WHEN** a terrain fragment samples a reduced self-shadow term
- **THEN** only its direct-sun contribution is attenuated by that term

#### Scenario: Indirect light is scaled by sky occlusion

- **WHEN** a terrain fragment samples a reduced ambient/sky-occlusion term
- **THEN** its sky/ambient contribution is attenuated while its direct-sun
  contribution is unaffected by that term

### Requirement: Water receives terrain shadow

Ocean and river water surfaces SHALL receive the shared directional shadow and
the terrain-landscape shadow so that terrain occludes water instead of water
being excluded from shadow receiving. Enabling water shadow receiving SHALL NOT
change water geometry, water simulation, or terrain authority.

#### Scenario: Hill casts shadow on the water surface

- **WHEN** terrain lies between a water fragment and the shared ephemeris Sun
- **THEN** the water fragment's direct-sun contribution is attenuated

#### Scenario: Water remains presentation-only

- **WHEN** water shadow receiving is enabled
- **THEN** water meshes still do not cast shadows and no terrain, collision, or
  simulation state changes

### Requirement: Patch geometry renders the eroded seam-safe surface

The system SHALL build rendered patch geometry from the erosion-consistent mesh height field, so eroded ridgelines, talus slopes, and carved valleys are visible and adjacent patches at equal or differing LOD remain crack-free.

#### Scenario: Eroded relief is visible

- **WHEN** a patch overlaps an eroded region
- **THEN** its rendered geometry reflects the eroded height field rather than the un-eroded analytic sculpt

#### Scenario: No cracks at shared boundaries

- **WHEN** two adjacent rendered patches sample a shared boundary
- **THEN** the rendered surfaces meet without a gap or T-vertex crack

### Requirement: Materials and scatter consume drainage signals

The system SHALL derive river and wet-biome appearance from the authoritative moisture and river-channel-strength signals, so visible drainage matches the hydrology that carved the surface.

#### Scenario: River channels read as water

- **WHEN** river-channel strength indicates a channel
- **THEN** the rendered surface uses the river appearance (darker, smoother, blue-shifted) for that location

#### Scenario: Wet biomes follow moisture

- **WHEN** moisture rises along a drainage network
- **THEN** the selected material and vegetation reflect the wetter biome

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

### Requirement: Surface microdetail remains presentation-only and source-derived
Terrain rendering SHALL derive microdetail appearance from the authoritative terrain source and prepared surface data while keeping geometry, streaming, collision, and LOD authority unchanged.

#### Scenario: Near terrain material detail
- **WHEN** a sufficiently detailed terrain patch is rendered near the flight camera
- **THEN** its material can provide bounded local color, normal, and roughness variation derived from prepared source data

#### Scenario: Patch eviction
- **WHEN** a terrain patch is evicted from the streaming cache
- **THEN** all presentation-only surface assets associated with that patch are released with the patch and no terrain authority data is changed

### Requirement: Papua surface presentation remains offline and deterministic
The Papua launch-region terrain presentation SHALL derive its tropical biome, vegetation, and material variation from existing offline terrain samples and deterministic patch inputs. It MUST NOT fetch terrain, imagery, land-cover, or vegetation data at runtime.

#### Scenario: Reproducible Papua patch presentation
- **WHEN** a Papua-region patch is prepared repeatedly with the same terrain source and configuration
- **THEN** its presentation data is equivalent regardless of preparation order or runtime network availability

