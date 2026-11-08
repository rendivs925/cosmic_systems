## ADDED Requirements

### Requirement: Coverage is the measured share of sky

The shared cloud density field SHALL treat the measured regional cloud fraction
as a share of the sky, not as a multiplier on the noise field. It SHALL threshold
the weather map's rank-equalised coverage channel so that the clear fraction is
genuinely clear air and the covered band ramps from a thin edge to a dense core.

#### Scenario: Measured fraction selects the share

- **WHEN** the deck coverage is set to a measured fraction
- **THEN** that fraction of the weather map lies above the clear-air threshold

### Requirement: Cloud type is regional and ranked

The deck SHALL derive its vertical type morph from the weather map's ranked type
and convective-vigour channels, so one deck contains flat sheets, broken cells
and towering convection in different places rather than one shape everywhere.

#### Scenario: One deck holds several types

- **WHEN** the type and convection ranks vary across a region
- **THEN** the height profile varies between sheet-like and cumuliform within the
  same deck

### Requirement: The deck carries a cell-size spectrum

The volumetric deck SHALL vary its horizontal cloud-cell size within one
regime, and SHALL derive that variation from channels already fetched by the
density field rather than from an additional texture fetch.

The selector SHALL change the carve amplitude, not the sample position: the
density field MUST NOT warp, shear or rescale the coordinate it samples.

Selector zero SHALL reproduce the existing density carve exactly, so the
measured coverage baseline remains the anchor of the spectrum.

The size field SHALL remain seamless across the periodic weather-map and noise
tiles, because both inputs are periodic and the blend is pointwise.

#### Scenario: Mixed cell sizes

- **WHEN** a deck region is drawn with a mid-range size selector
- **THEN** adjacent cells show different footprints rather than one repeated
  puff

#### Scenario: No extra fetch

- **WHEN** the shared density field is compiled
- **THEN** it issues exactly one fetch per texture: base volume, detail volume
  and weather map

### Requirement: One density field serves the deck and the ground shadow

The volumetric deck and the terrain's cloud shadow SHALL evaluate the same
density field, so a shadow cannot disagree with the cloud that casts it. The
shadow march SHALL include the documented diffuse floor rather than turning an
overcast day black.

#### Scenario: Shadow matches the deck

- **WHEN** a ground fragment lies under a dense part of the deck
- **THEN** the same field that draws that density reduces its direct sunlight

### Requirement: Convective cells align with the measured wind

The density field SHALL stretch cells along the measured wind direction, and
both the deck and its shadows SHALL use that same axis and factor. Coverage and
the density field SHALL advect at the same wind speed.

#### Scenario: Wind streets

- **WHEN** the deck advects with a measured wind
- **THEN** the cells and the coverage pattern drift together along that wind

### Requirement: The march samples the level its step can resolve

The density field SHALL be sampled at an explicit mip level derived from the
march step length, not with an implicit-derivative sample inside a divergent
loop. A step longer than a cell SHALL sample a coarser level.

#### Scenario: Distant deck

- **WHEN** a march step spans several noise cells
- **THEN** the density is read from a coarser mip instead of aliasing the finest
