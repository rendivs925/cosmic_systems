## Purpose

Deterministically places vegetation candidates inside each terrain patch using blue-noise sampling, a low-frequency clumping/clearing mask, ecological rules, and per-species spacing, so cover is stable and natural instead of uniformly random.

## ADDED Requirements

### Requirement: Placement is deterministic per patch

The system SHALL derive every vegetation candidate within a terrain patch from the patch identity and the active simulation seed, independent of frame rate, patch generation order, cache history, and worker scheduling. Identical inputs SHALL produce identical placements.

#### Scenario: Repeated generation is identical

- **WHEN** the same patch identity and seed are placed twice
- **THEN** the two candidate sets are identical in position, count, and order

#### Scenario: Placement is independent of generation order

- **WHEN** a patch is generated before or after its neighbors, or after a cache eviction
- **THEN** its candidate set is unchanged

### Requirement: Placement uses blue-noise sampling

The system SHALL distribute candidates with a blue-noise / Poisson-disk process so no two accepted candidates within a patch lie closer than the configured minimum radius for their species, without introducing regular grid artifacts.

#### Scenario: Minimum spacing respected

- **WHEN** a patch's accepted candidates are checked pairwise
- **THEN** no two same-species candidates are closer than that species' minimum radius

#### Scenario: Natural, artifact-free distribution

- **WHEN** a uniform-eligible patch's candidates are compared to a regular lattice
- **THEN** the distribution does not collapse onto a fixed grid or produce large empty bands

### Requirement: Placement obeys ecological rules

The system SHALL reject candidates that fall outside the accepted altitude band for their species, exceed the slope limit, lie at or below the water/sea-level datum, or where moisture and measured/climate cover are insufficient.

#### Scenario: Unsuitable ground is rejected

- **WHEN** a candidate falls on steep ground, below the water datum, or outside the species altitude band
- **THEN** no plant is placed at that candidate

#### Scenario: Moisture and cover gate placement

- **WHEN** a site's moisture or available vegetation cover is below the configured minimum
- **THEN** candidates are thinned or dropped rather than placed uniformly

### Requirement: Clumping and clearing mask

The system SHALL modulate candidate density with a deterministic low-frequency clumping/clearing mask so vegetation forms groves and clearings that vary at patch and multi-patch scale.

#### Scenario: Groves and clearings appear

- **WHEN** a vegetated region is placed
- **THEN** density varies smoothly between dense groves and near-empty clearings

#### Scenario: Mask is continuous across patch boundaries

- **WHEN** two adjacent patches are placed
- **THEN** the mask value along their shared edge is continuous and does not create a density seam

### Requirement: Per-species spacing

The system SHALL enforce species-specific minimum spacing so larger species such as canopy trees occupy more area than shrubs and grass.

#### Scenario: Different species use different spacing

- **WHEN** tree and shrub candidates share a region
- **THEN** minimum spacing for trees is greater than for shrubs, and greater again than for grass
