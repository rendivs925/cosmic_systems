## ADDED Requirements

### Requirement: Synthetic detail must not fabricate coastline

Where a body has an ocean, bounded synthetic detail SHALL NOT lower ground that
is at or above the local sea datum. On such ground the layer's negative
contribution MAY be reduced to zero, but its positive contribution SHALL always
be applied. Below the datum the guard SHALL NOT rewrite measured bathymetry: the
signed detail SHALL be restored, and the removal SHALL fade continuously so the
composed height does not step at the waterline.

The land guard SHALL span at least enough height to cover the layer's own
negative amplitude, so that no point inside the window can be lowered below the
datum by the layer's worst case. The same rule SHALL apply in the physical
height, the mesh height and every level of detail.

The mechanism SHALL be opt-in per detail layer, because a body without an ocean
cannot be flooded by it and must keep its signed relief.

#### Scenario: Negative detail at the datum

- **WHEN** a detail layer with a sea-level guard contributes a negative value to
  ground at or just above the datum
- **THEN** the composed height is not lowered below the datum, while the same
  layer's positive contribution is applied unchanged

#### Scenario: Worst case cannot cross the datum

- **WHEN** the layer's minimum possible contribution is applied across the whole
  guard window
- **THEN** the composed height remains at or above the datum everywhere in that
  window

#### Scenario: Ocean floor keeps its relief

- **WHEN** the measured base is well below the datum
- **THEN** the layer keeps its signed contribution and the bathymetry is not
  changed by the guard

#### Scenario: Waterline stays continuous

- **WHEN** the base height crosses the datum
- **THEN** the guard fades out below it and the composed height has no step

#### Scenario: Mesh and collision agree

- **WHEN** the same coordinate is queried through the physical height and the
  mesh height at any level of detail
- **THEN** both apply the same guard and resolve the same composed height
