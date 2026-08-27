## MODIFIED Requirements

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
