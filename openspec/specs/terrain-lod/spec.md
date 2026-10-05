# Terrain LOD Specification

## Purpose

Defines hierarchical terrain rendering and streaming: a cube-sphere planetary surface with quadtree subdivision, screen-space LOD with crack-free transitions, deterministic generation, and a streaming lifecycle (requested, generating, loading, ready, visible, cached, evicted) with explicit memory limits.
## Requirements
### Requirement: Planetary surface uses cube-sphere topology

The system SHALL represent each active planet's complete surface as a cube-sphere terrain hierarchy, not a flat plane or a separate visual proxy, so the rocket can fly continuously from orbit to the surface.

#### Scenario: Spherical surface
- **WHEN** terrain is generated for a planet
- **THEN** every rendered tile conforms to the planet's spherical surface at the planet's radius plus the active shared terrain height

#### Scenario: Flight continuity
- **WHEN** the rocket descends from orbit toward the surface
- **THEN** a terrain surface remains continuously rendered and aligned with the planet's body-fixed frame

### Requirement: Terrain is hierarchical via quadtree

The system SHALL subdivide each cube-sphere face as a quadtree from permanently available planet-wide root patches to local fine patches, while retaining a complete visible leaf cover of the surface.

#### Scenario: Coarse to fine subdivision
- **WHEN** terrain is requested at increasing detail
- **THEN** a covered parent patch is replaced by its finer child patches only after those children are ready

#### Scenario: Local detail near the rocket
- **WHEN** the rocket is near a region
- **THEN** that region is refined to a higher LOD than distant regions while coarser patches continue to cover the remaining planet

#### Scenario: Root coverage
- **WHEN** rocket-mode terrain initializes or its detail cache is empty
- **THEN** all six cube-sphere root faces remain represented by terrain tiles without requiring a separate globe mesh

### Requirement: LOD selection is screen-space aware

The system SHALL select patch detail from projected geometric error and camera visibility, preserve a renderable parent while required descendants load, and keep neighboring visible leaves crack-free across both same-face and cube-face boundaries.

#### Scenario: Distance-driven LOD
- **WHEN** a visible patch's projected geometric error exceeds the configured tolerance
- **THEN** the patch is refined, subject to the configured maximum LOD and memory budget

#### Scenario: Parent fallback during generation
- **WHEN** selected child patches are not ready
- **THEN** their parent remains visible and no hole exposes empty space

#### Scenario: Crack-free transitions
- **WHEN** adjacent visible patches have different LOD levels or meet at a cube-face edge
- **THEN** the surface remains crack-free and neighboring leaf levels differ by no more than the configured balance limit

#### Scenario: No vertex popping
- **WHEN** a visible patch is replaced by a ready refinement or coarsening result
- **THEN** its surface transition occurs without a sudden visible position discontinuity

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

### Requirement: LOD uses per-tile geometric error from the elevation payload

Terrain LOD selection SHALL derive a patch's geometric error from the
corresponding elevation payload tile's declared error and elevation range,
rather than only from a planet-wide conservative envelope.

#### Scenario: Detailed region refines earlier

- **WHEN** a patch covers a payload tile with high measured relief
- **THEN** its projected error causes refinement at a greater distance than a
  low-relief patch at the same level

#### Scenario: Coarse coverage stays conservative

- **WHEN** a patch has no high-resolution payload tile
- **THEN** its geometric error uses the declared conservative fallback bound

