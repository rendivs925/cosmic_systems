## ADDED Requirements

### Requirement: Ocean cap classifies water at the sea datum

The ocean cap SHALL decide whether a vertex is wet by comparing its bed against
the source's sea-level datum, matching the terrain source's own land/sea
classification. A vertical offset applied to the water surface for depth
stability SHALL NOT be used as a flood threshold.

#### Scenario: Measured land just above the datum

- **WHEN** a coastal vertex sits a few centimetres above the sea datum
- **THEN** no ocean cap is emitted for it, even when an adjacent vertex is wet
  and the water surface carries a small depth-stability offset

#### Scenario: Bed at or below the datum

- **WHEN** a vertex sits at or below the sea datum
- **THEN** an ocean cap is emitted and its surface stays on the offset datum so
  it does not z-fight the bed or the far-field globe

#### Scenario: Surface offset is not a flood threshold

- **WHEN** the depth-stability offset changes
- **THEN** the set of vertices classified as water is unchanged
