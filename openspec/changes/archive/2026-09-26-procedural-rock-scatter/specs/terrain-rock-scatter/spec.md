## Purpose

Generates the deterministic procedural rock and scree geometry scattered across close-range terrain patches, placing it on steeper ground and seating each rock into the slope as presentation-only detail.

## ADDED Requirements

### Requirement: Rock bodies are procedurally generated, not uniform spheres

The system SHALL generate each scattered rock from a base shape whose surface is
displaced by deterministic noise, optionally smoothed, producing an irregular
rock silhouette instead of a uniform UV-sphere boulder.

#### Scenario: Deterministic rock geometry

- **WHEN** the same patch identity, seed, and per-slot index are generated twice
- **THEN** the produced rock geometry and vertex colours are identical

#### Scenario: Rock silhouette is non-spherical

- **WHEN** a rock is generated from its deterministic noise seed
- **THEN** its vertices are displaced from the undeformed base shape so the
  rendered form is irregular rather than a sphere

### Requirement: Rock size and aspect vary per instance

The system SHALL vary each rock's size and aspect ratio deterministically so a
cluster does not read as a repeated identical body, while remaining within the
patch's bounded rock budget.

#### Scenario: Varied rock scale and aspect

- **WHEN** multiple rocks are generated within one patch
- **THEN** their scales and axis proportions differ deterministically and no two
  adjacent rocks present an identical footprint

### Requirement: Rock placement uses slope-weighted blue-noise sampling

The system SHALL place rocks using blue-noise sampling whose acceptance
increases with terrain slope, so scree and outcrop concentrate on steeper ground
while remaining deterministic from patch identity and seed.

#### Scenario: Steeper ground gathers more rock

- **WHEN** two patches differ only in steepness and share the same patch-level
  budget
- **THEN** the steeper patch accepts more rock candidates than the gentle patch

#### Scenario: Placement avoids clustering artifacts

- **WHEN** rock placement runs for a patch
- **THEN** accepted positions are distributed by the deterministic blue-noise
  rule rather than independent uncorrelated per-slot hashing

### Requirement: Rocks are embedded into the local slope

The system SHALL ground each rock against the authoritative terrain height and
surface normal, sinking its base below the sampled surface so it appears seated
in the slope rather than resting on top of it.

#### Scenario: Rock base sits below the surface

- **WHEN** a rock is placed on sloped terrain
- **THEN** its base is positioned below the authoritative terrain height along
  the surface normal for that location

#### Scenario: Placement follows the terrain source

- **WHEN** rock placement samples height, normal, or slope
- **THEN** it reads those values from the shared `TerrainSource` authority and
  does not compute an independent terrain field

### Requirement: Contact ambient occlusion darkens rock bases

The system SHALL darken rock vertex colour toward its ground contact so the rock
reads as occluded where it meets the terrain, without adding a per-frame or
per-entity lighting pass.

#### Scenario: Base is darker than crown

- **WHEN** contact occlusion is applied to a rock's vertices
- **THEN** vertices nearest the embedded base are darker than vertices at the
  exposed crown

### Requirement: Rock scatter is presentation-only and budget-bounded

The system SHALL emit rock geometry only into the existing per-patch merged
scatter mesh, SHALL NOT create per-rock ECS entities or feed any collision,
altitude, landing, or physics computation, and SHALL retain the existing
per-patch count cap and LOD decimation.

#### Scenario: No per-rock entities

- **WHEN** rocks are scattered for a patch
- **THEN** their geometry is merged into the single per-patch scatter mesh and no
  new ECS entity is spawned per rock

#### Scenario: LOD decimation is preserved

- **WHEN** a patch is rendered at a coarser LOD level
- **THEN** the rock candidate count is reduced by the existing scatter LOD
  budgeting and never exceeds the per-patch cap

#### Scenario: Rock geometry never affects physics

- **WHEN** rocket collision, altitude, or landing queries the terrain
- **THEN** they resolve against the authoritative `TerrainSource` and are
  unaffected by scattered rock geometry
