# Terrain LOD Specification

## Purpose

Defines hierarchical terrain rendering and streaming: a cube-sphere planetary surface with quadtree subdivision, screen-space LOD with crack-free transitions, deterministic generation, and a streaming lifecycle (requested, generating, loading, ready, visible, cached, evicted) with explicit memory limits.

## Requirements

### Requirement: Planetary surface uses cube-sphere topology

The system SHALL represent a planet's surface as a cube-sphere mesh, not a flat plane, so the rocket can fly from orbit to the surface.

#### Scenario: Spherical surface

- **WHEN** terrain is generated for a planet
- **THEN** it conforms to the planet's spherical surface at the planet's radius plus local height

#### Scenario: Flight continuity

- **WHEN** the rocket descends from orbit toward the surface
- **THEN** the terrain is continuous and aligned with the planet's body-fixed frame

### Requirement: Terrain is hierarchical via quadtree

The system SHALL subdivide the surface into a quadtree with level-of-detail, from planet-wide coarse patches to local fine patches.

#### Scenario: Coarse to fine subdivision

- **WHEN** terrain is requested at increasing detail
- **THEN** patches subdivide into finer patches down to a defined minimum resolution

#### Scenario: Local detail near the rocket

- **WHEN** the rocket is near a region
- **THEN** that region is refined to a higher LOD than distant regions

### Requirement: LOD selection is screen-space aware

The system SHALL select patch detail based on rendering requirements such as camera distance and projected geometric error, not arbitrary fixed thresholds alone.

#### Scenario: Distance-driven LOD

- **WHEN** a patch is farther from the camera
- **THEN** a coarser LOD is used, and the patch is refined as the camera approaches

#### Scenario: Crack-free transitions

- **WHEN** adjacent patches have different LOD levels
- **THEN** the surface remains crack-free across patch boundaries

### Requirement: Terrain streams with a defined lifecycle

The system SHALL manage terrain patches through a lifecycle (requested, generating, loading, ready, visible, cached, evicted) with memory limits and eviction.

#### Scenario: Patch lifecycle

- **WHEN** a patch becomes needed
- **THEN** it transitions requested → generating/loading → ready → visible

#### Scenario: Memory bound

- **WHEN** resident terrain exceeds the configured limit
- **THEN** the system evicts cached (non-visible) patches to stay within the limit

#### Scenario: No full-planet max resolution

- **WHEN** the simulation runs
- **THEN** it does not generate the entire planet at maximum resolution

### Requirement: Generation is deterministic

Procedural terrain patches SHALL be generated deterministically from seed and patch coordinates, independent of runtime conditions.

#### Scenario: Identical regeneration

- **WHEN** the same patch coordinates and seed generate twice
- **THEN** the meshes are identical

#### Scenario: Runtime independence

- **WHEN** terrain patches are generated
- **THEN** results do not depend on frame rate or camera movement