# Terrain Source Specification

## Purpose

Defines the terrain data abstraction for the rocket simulator: a `TerrainSource` interface separating terrain data from rendering and collision, with procedural, heightmap, and DEM implementations planned so the renderer never depends on the data source.
## Requirements
### Requirement: Terrain data is separate from rendering and collision

The system SHALL model terrain as a data source (`TerrainSource`) with render mesh and collision as separate consumers, so replacing the data source does not rewrite the renderer or collision code.

#### Scenario: Source-to-mesh independence

- **WHEN** the terrain source implementation changes (procedural to DEM)
- **THEN** the render mesh and collision systems continue to work unchanged

#### Scenario: Shared height function

- **WHEN** any consumer needs terrain height at a position
- **THEN** it calls the shared terrain height function provided by the active source

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

### Requirement: Heightmap and DEM sources are supported

The system SHALL support heightmap-based terrain sources and provide an interface for real planetary DEM data without rewiring the renderer.

#### Scenario: Heightmap source

- **WHEN** a heightmap terrain source is active
- **THEN** terrain height is sampled from the heightmap data

#### Scenario: DEM-ready interface

- **WHEN** real DEM data becomes available
- **THEN** a DEM terrain source can be added behind the same `TerrainSource` interface

### Requirement: Launch-site patches reuse the shared source

Existing localized launch-site patches (KSC, RTLS, drone ship, lunar) SHALL continue to exist as detailed site objects but MUST sample their height from the shared terrain source architecture rather than a bespoke path.

#### Scenario: Site height via shared source

- **WHEN** a launch-site patch is rendered or queried for collision
- **THEN** its height comes from the shared terrain source interface

#### Scenario: Existing sites preserved

- **WHEN** the terrain source architecture is introduced
- **THEN** the existing launch-site patches remain available as before

### Requirement: Climate vegetation density is the procedural fallback

The terrain source SHALL expose its deterministic climate-derived `vegetation_density` as the documented fallback signal for vegetation when no measured land-cover package is available, computed from climate inputs such as moisture, latitude, and altitude rather than from rendered color.

#### Scenario: Fallback drives placement without a package

- **WHEN** no measured land-cover package is loaded
- **THEN** vegetation placement uses the source's climate-derived density

#### Scenario: Fallback is deterministic and climate-based

- **WHEN** the same coordinate is sampled twice
- **THEN** the fallback density is identical, and it tracks climate rather than the rendered albedo

### Requirement: Measured land cover is a presentation-only overlay

The system SHALL allow a measured land-cover package to override the fallback density and species mix for vegetation presentation only. The terrain source's sampled height, collision surface, and physics authority MUST NOT depend on measured land cover.

#### Scenario: Overlay does not change terrain authority

- **WHEN** a measured land-cover package replaces the fallback for a coordinate
- **THEN** the source's height and collision samples for that coordinate are unchanged

#### Scenario: Fallback remains available

- **WHEN** the measured package is absent or out of coverage
- **THEN** the source continues to supply its climate-derived vegetation density

### Requirement: Tile-backed terrain source preserves a resident fallback

The terrain authority SHALL always provide a resident coarse elevation surface
for every body with terrain, so height queries succeed even when no
high-resolution tile is resident.

#### Scenario: Query outside resident tile coverage

- **WHEN** a height or collision query is made where no high-resolution tile is
  resident
- **THEN** the authority returns the resident coarse surface value with its
  declared conservative error

#### Scenario: Query inside resident coverage

- **WHEN** a height or collision query is made where a high-resolution tile is
  resident
- **THEN** the authority returns the tile's measured elevation

### Requirement: Terrain height sampling never performs I/O or blocks

Authoritative terrain sampling used by collision, radar altitude, and physics
SHALL read only already-resident data and SHALL NOT load, decode, or await a
payload tile.

#### Scenario: Fixed-step collision during tile load

- **WHEN** the rocket queries terrain height while a payload tile is being
  decoded by a worker task
- **THEN** the query completes synchronously from resident data without
  blocking on the worker

### Requirement: Tile residency is bounded and deterministic

The set of resident elevation tiles SHALL be bounded by an explicit budget with
deterministic eviction, and a cache miss SHALL produce the same samples as a
cache hit for the same tile.

#### Scenario: Eviction then reload

- **WHEN** a resident tile is evicted and later reloaded
- **THEN** the reloaded tile produces identical elevation samples to the
  original

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

### Requirement: Layered terrain elevation is coherent across LOD

The system SHALL compose base planetary shape, global elevation, optional local DEM elevation, and bounded procedural detail through the active shared terrain source. The same geographic coordinate SHALL resolve to a continuous terrain surface independent of the current render LOD.

#### Scenario: Parent-child elevation agreement
- **WHEN** a parent terrain tile is replaced by child tiles at the same geographic boundary
- **THEN** their shared edge samples resolve to the same terrain height within the configured numerical tolerance

#### Scenario: Procedural detail fade
- **WHEN** procedural detail is unavailable or intentionally omitted at a coarse LOD
- **THEN** its contribution fades continuously to the shared base surface rather than producing a height step

#### Scenario: DEM fallback
- **WHEN** local DEM coverage is unavailable for a coordinate
- **THEN** the terrain source remains deterministic and supplies its configured global or procedural fallback height

