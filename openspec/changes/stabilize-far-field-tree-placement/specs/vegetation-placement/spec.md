## ADDED Requirements

### Requirement: Tree sites are stable across LOD refinement

Tree candidate positions and the ecological acceptance decision SHALL derive only
from the world direction, never from the containing patch. When a patch refines,
every tree already placed within it MUST keep its position and remain placed. A
coarse, tree-only patch MAY thin its accepted sites to a bounded budget, but the
retained sites MUST be a subset of the sites the finer representation accepts.

#### Scenario: Refinement keeps trees in place

- **WHEN** a patch refines to the next level
- **THEN** every tree placed on the coarse patch is still placed at the same world
  position

#### Scenario: Acceptance is level-independent

- **WHEN** the ecological gate is evaluated for one world direction at two
  different LOD levels
- **THEN** it reaches the same decision, because it reads only level-independent
  signals

#### Scenario: Coarse thinning nests

- **WHEN** a coarse patch thins its sites to its bounded budget
- **THEN** the retained sites are a subset of those the finer patch accepts, so no
  coarse tree is lost on refinement

#### Scenario: Coarse cost stays bounded

- **WHEN** a coarse, tree-only patch builds its tree representation
- **THEN** its tree count does not exceed the coarse budget and its geometry does
  not exceed the finer representation for the same ground
