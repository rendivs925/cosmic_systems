## ADDED Requirements

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
