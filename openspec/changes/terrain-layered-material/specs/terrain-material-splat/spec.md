## Purpose

Provide one deterministic, presentation-only terrain material that blends multiple
ground layers (grass, soil, rock, sand, snow) from authoritative terrain inputs,
with triplanar projection on steep faces and multi-scale detail overlays that hide
tiling, without changing terrain geometry, collision, streaming, or simulation
authority.

## ADDED Requirements

### Requirement: Ground layers blend continuously from authoritative inputs

The system SHALL blend a bounded set of ground layers (grass, soil, rock, sand,
snow) using continuous weights derived only from authoritative terrain samples:
elevation, slope, moisture, and latitude zone. The blend MUST be continuous across
layer transitions and MUST NOT use hard biome bands.

#### Scenario: Slope exposes rock

- **WHEN** a terrain point's slope increases through the configured rock range
- **THEN** its blended material transitions smoothly from vegetation and soil
  toward rock without a visible hard edge

#### Scenario: Snow line

- **WHEN** a terrain point's elevation crosses the configured snow band
- **THEN** the blended material transitions continuously toward snow and returns to
  the lower-elevation layers below the band

#### Scenario: Adjacent LOD agreement

- **WHEN** two adjacent patches at different LOD levels share a boundary
- **THEN** their layer weights agree at shared source samples so the seam shows no
  material discontinuity

#### Scenario: Deterministic weights

- **WHEN** the same source, seed, patch, and configuration are evaluated twice
- **THEN** the resulting layer weights are identical and independent of frame rate,
  spawn order, and cache history

### Requirement: Terrain layers use PBR texture sets

The system SHALL provide each ground layer with an albedo, tangent-space normal,
and roughness PBR texture set sampled with tiling coordinates, and MUST combine
layer normals and roughness with the same weighted mix used for albedo.

#### Scenario: Per-layer normal and roughness

- **WHEN** a point is blended from more than one layer
- **THEN** its rendered normal and roughness reflect the same weighted layer mix as
  its albedo

#### Scenario: Single dominant layer

- **WHEN** one layer dominates a large visible extent
- **THEN** low-frequency macro variation and multi-scale detail break up the repeat
  so the layer does not read as an obvious tile grid

### Requirement: Steep faces use triplanar projection

The system SHALL project layer detail on steep faces using triplanar (world-axis)
sampling, blended continuously with the patch-local projection by surface
orientation, so steep terrain neither stretches nor seams.

#### Scenario: Steep face projection

- **WHEN** a rendered surface is near-vertical
- **THEN** its layer detail is sampled triplanar without visible UV stretching

#### Scenario: Continuous projection blend

- **WHEN** surface orientation varies across a patch
- **THEN** the projection blend changes continuously and introduces no hard boundary
  or cube-sphere seam

### Requirement: Detail overlays fade with camera distance

The system SHALL add a low-frequency macro albedo variation, a mid-frequency
micro-detail normal and roughness overlay, and a near-camera detail overlay that
fades to zero with camera distance. The overlay fade MUST be evaluated per pixel so
shared patch edges remain continuous.

#### Scenario: Near-camera detail

- **WHEN** the camera is close to rendered terrain
- **THEN** the near-camera detail overlay adds high-frequency surface grain that
  reduces visible texture tiling

#### Scenario: Distance fallback

- **WHEN** the same terrain is viewed from far away
- **THEN** the overlay contribution fades out so the material does not alias or
  shimmer

### Requirement: Terrain material stays presentation-only

Layer selection, layer weights, and layer textures SHALL be presentation-only and
MUST NOT be read by terrain height, collision, altitude, landing, or any physical
system. Collision and altitude MUST continue to sample the authoritative terrain
source.

#### Scenario: Collision independence

- **WHEN** collision or altitude logic samples the terrain surface
- **THEN** it uses the authoritative `TerrainSource` and is unaffected by layer
  weights, layer textures, or material residency

#### Scenario: Geometry independence

- **WHEN** layer weights or textures change
- **THEN** terrain geometry, streaming LOD selection, and collision data are
  unchanged

### Requirement: Layered material respects existing budgets and has a single-layer fallback

The system SHALL keep layered-material texture residency and uploads within
data-driven budgets and MUST NOT stall terrain geometry streaming. When the layered
PBR sets or the required build capability are unavailable, the terrain SHALL fall
back to the existing single-layer surface appearance path.

#### Scenario: Browser fallback

- **WHEN** the layered material path is unavailable, such as a browser WASM build
  without `dem`
- **THEN** visible terrain renders through the existing single-layer appearance path

#### Scenario: Budget pressure

- **WHEN** layered-material residency would exceed its configured budget
- **THEN** the system keeps a bounded working set and continues to publish terrain
  geometry without stalling

#### Scenario: Budget changes are evidence-gated

- **WHEN** a change would raise a terrain material texture or upload budget
- **THEN** it is justified by measured evidence before the budget is raised
