## Purpose

Selects deterministic vegetation species for each site and builds distinct, bounded, baked geometry per species so patches show recognizable forest types and ground cover instead of one repeated tree.

## ADDED Requirements

### Requirement: Species selection is deterministic and ecological

The system SHALL choose each accepted candidate's species deterministically from measured or fallback land cover, moisture, altitude, slope, and latitude, so the same site selects the same species.

#### Scenario: Same site selects the same species

- **WHEN** the same site is classified twice with the same land-cover inputs
- **THEN** the selected species is identical

#### Scenario: Species follow climate and land cover

- **WHEN** land cover, moisture, or altitude changes between two sites
- **THEN** the selected species mix changes to match the local ecological conditions

### Requirement: Distinct bounded species geometry

The system SHALL provide distinct deterministic geometry for tropical broadleaf, temperate broadleaf, conifer/boreal, palm, shrub/understory, and grass, each built from bounded baked skeleton data rather than runtime randomness.

#### Scenario: Species are visually distinct

- **WHEN** two different species are rendered side by side
- **THEN** their silhouettes and proportions are distinguishable (for example palm fronds versus conifer crown versus grass tuft)

#### Scenario: Geometry is bounded and reproducible

- **WHEN** a species skeleton is built twice
- **THEN** the output geometry is identical and stays within its declared vertex and index bounds

### Requirement: Merged per-patch mesh and budgets are retained

The system SHALL merge all species geometry for a patch into the existing single per-patch vegetation mesh and SHALL keep the current scatter budgets. Budget increases MUST be deferred until profiling evidence justifies them.

#### Scenario: One merged mesh per patch

- **WHEN** a patch contains multiple species
- **THEN** they are emitted into one merged mesh with no per-plant ECS entities

#### Scenario: Budgets unchanged

- **WHEN** this change is implemented
- **THEN** the per-patch scatter vertex/index budgets do not increase without recorded profiling evidence

### Requirement: Surface-aligned grounding

The system SHALL embed each plant base into the local slope and align the plant's up axis to the terrain surface normal so no plant floats above or clips through the ground.

#### Scenario: Bases embed into slopes

- **WHEN** a plant is placed on sloped ground
- **THEN** its base sits into the slope rather than floating at a point on the surface

#### Scenario: Plants align to the surface normal

- **WHEN** a plant is placed
- **THEN** its up axis matches the terrain surface normal at that location
