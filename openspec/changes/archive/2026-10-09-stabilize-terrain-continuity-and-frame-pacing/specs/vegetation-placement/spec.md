## MODIFIED Requirements

### Requirement: Placement is deterministic per patch

The system SHALL derive every vegetation candidate within a terrain patch from
the patch identity and the active simulation seed, independent of frame rate,
patch generation order, cache history, and worker scheduling. Identical inputs
SHALL produce identical placements.

#### Scenario: Repeated generation is identical
- **WHEN** the same patch identity and seed are placed twice
- **THEN** the two candidate sets are identical in position, count, and order

#### Scenario: Placement is independent of generation order
- **WHEN** a patch is generated before or after its neighbors, or after a cache eviction
- **THEN** its candidate set is unchanged

## ADDED Requirements

### Requirement: Coarse vegetation preserves physical size within a bounded budget

Coarse, tree-only patches SHALL place trees at their true physical size and
within a bounded budget. The system MUST NOT enlarge trees to fake distance
coverage and MUST NOT rely on a hard level cutoff that would remove all trees the
moment a patch crosses a single level, as long as the patch still builds a tree
representation.

#### Scenario: Trees are not enlarged
- **WHEN** a patch builds the coarse tree-only representation
- **THEN** each tree uses its species' physical dimensions

#### Scenario: Coarse cost is bounded
- **WHEN** a coarse patch builds its tree representation
- **THEN** its tree count does not exceed the coarse budget and its geometry does
  not exceed the finer representation for the same ground

#### Scenario: Refinement does not zero cover
- **WHEN** a patch refines from a coarse tree-only level to the next level
- **THEN** the region still contains trees and does not drop to no vegetation

### Requirement: Patch memory estimate covers every generated representation

The system SHALL include every representation a patch can generate in its
streaming memory estimate and budget, including coarse tree-only vegetation. No
generated mesh or texture may reside outside the accounted patch cost.

#### Scenario: Coarse vegetation is accounted
- **WHEN** a patch can generate tree-only coarse vegetation
- **THEN** that cost is included in the patch's estimated and reserved bytes

#### Scenario: Accounting matches generation
- **WHEN** a patch generates its vegetation representation
- **THEN** the representation's size is bounded by the accounted estimate
