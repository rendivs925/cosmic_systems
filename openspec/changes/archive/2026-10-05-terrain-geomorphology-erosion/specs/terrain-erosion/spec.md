## Purpose

Defines a deterministic, seeded geomorphology field that applies thermal talus slump, hydraulic droplet erosion, and D8 flow-accumulation river carving to the analytic terrain sculpt, producing the eroded height, flow, moisture, and river-channel strengths that the terrain authority exposes to every consumer.

## ADDED Requirements

### Requirement: Erosion is a deterministic static field

The system SHALL produce the eroded surface as a deterministic, seeded, static field sampled from a fixed per-tile grid, so identical seed and tile inputs always produce identical height, flow, moisture, and river-strength channels regardless of frame rate, evaluation order, or cache history.

#### Scenario: Identical regeneration

- **WHEN** the same tile is eroded twice with the same seed and configuration
- **THEN** the height, flow, and moisture channels are bit-identical

#### Scenario: Cache history independence

- **WHEN** a tile is evicted after already being sampled, then regenerated
- **THEN** every terrain channel reproduces the earlier values exactly

#### Scenario: Not a per-frame simulation

- **WHEN** the simulation advances render frames without changing terrain parameters or seed
- **THEN** the eroded field does not evolve and no erosion work is performed per frame

### Requirement: Thermal erosion limits slopes to the angle of repose

The system SHALL apply thermal slump that lowers any slope steeper than the configured talus angle toward its steepest downhill neighbor, moving material without creating or destroying it.

#### Scenario: Steep step is softened

- **WHEN** a tile contains a slope steeper than the configured talus slope
- **THEN** the steepest slope after thermal erosion is lower than before

#### Scenario: Volume is conserved

- **WHEN** thermal erosion moves material on a closed tile
- **THEN** the total material in the tile is unchanged within numerical tolerance

### Requirement: Hydraulic erosion is deterministic and mass-consistent

The system SHALL apply seeded droplet erosion that erodes where sediment is under capacity and deposits where it exceeds capacity, removing only material above the configured floor.

#### Scenario: Seed controls the result

- **WHEN** two runs use the same seed
- **THEN** the resulting heights are identical, and both differ from the un-eroded flat input where erosion occurred

#### Scenario: The floor creates no sediment

- **WHEN** a droplet reaches the configured elevation floor
- **THEN** no material is added below the floor and total mass is not increased by the floor

### Requirement: D8 flow accumulation routes water downslope

The system SHALL accumulate flow so each cell contributes a unit of rain routed to its steepest downhill neighbor, accounting for latitude-dependent cell spacing, and processed so upslope accumulation reaches downslope cells.

#### Scenario: Flow accumulates downhill

- **WHEN** a tile slopes from high to low
- **THEN** the low side receives strictly more accumulated flow than the high side

#### Scenario: Steeper cardinal beats lower diagonal

- **WHEN** a cell has a steeper cardinal descent than a diagonal neighbor with a larger raw height drop
- **THEN** flow is routed to the steeper cardinal neighbor

#### Scenario: Polar cell width is respected

- **WHEN** a tile approaches a pole so its east-west cell width differs from its north-south spacing
- **THEN** flow routing uses the latitude-adjusted distance rather than a uniform grid spacing

### Requirement: Rivers carve channels and raise moisture

The system SHALL carve river channels where flow accumulation exceeds the configured threshold, with carve depth capped, and raise the moisture channel there so the same hydrology drives wet biomes.

#### Scenario: Channels sit below their surroundings

- **WHEN** flow accumulation exceeds the river threshold
- **THEN** the carved height is below the surrounding terrain and the moisture at that cell increases

#### Scenario: River strength is bounded

- **WHEN** river-channel strength is exposed to consumers
- **THEN** it is normalized to the range [0, 1]

### Requirement: Independent tiles are seam-safe

The system SHALL feather eroded height, moisture, and river strength back toward the analytic base near tile boundaries so adjacent independently-eroded tiles remain continuous and show no seam.

#### Scenario: Tile edge blends to base

- **WHEN** a sample lies at a tile boundary
- **THEN** the eroded contribution is fully faded to the analytic base value, and the interior is unchanged by the feather

#### Scenario: Equivalent coordinates share a tile

- **WHEN** two coordinates represent the same geographic point (across the longitude wrap or reflected over a pole)
- **THEN** they resolve to the same tile and return identical height, moisture, and river strength

### Requirement: Erosion caching is bounded and optional

The system SHALL bound resident eroded tiles and evict least-recently-used tiles, and it SHALL support using an offline-baked elevation/hydrology payload in place of runtime baking without changing the sampled field.

#### Scenario: Resident tiles stay bounded

- **WHEN** more distinct tiles are requested than the configured cache limit
- **THEN** the number of resident tiles never exceeds the limit and eviction uses a deterministic recency order

#### Scenario: Offline bake and runtime cache agree

- **WHEN** an offline-baked field exists for a tile
- **THEN** sampling that tile returns the same height, flow, moisture, and river strength as baking it at runtime
