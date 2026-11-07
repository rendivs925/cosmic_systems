## ADDED Requirements

### Requirement: The sea datum is applied exactly once

The terrain authority SHALL expose heights on one body frame and one sea-level
datum: ellipsoidal heights plus the local geoid undulation. A consumer that needs
orthometric height SHALL subtract that datum exactly once. A producer that
ingests orthometric source data SHALL convert it to the ellipsoidal frame once
and no runtime consumer SHALL repeat the correction.

#### Scenario: Consumer agreement at a control point

- **WHEN** the same coastal control point is sampled by the terrain source, the
  rendered mesh, the land/sea classification, and collision
- **THEN** they agree on one ellipsoidal height and on one orthometric height
  within the documented tolerance

#### Scenario: Orthometric consumer

- **WHEN** a consumer reports height above sea level
- **THEN** it subtracts the exposed datum once and does not apply a second
  correction of its own

#### Scenario: Orthometric payload ingest

- **WHEN** a regional payload stores orthometric heights
- **THEN** the ingest adds the local undulation once and the runtime samples the
  resulting ellipsoidal height without adding it again

#### Scenario: Double correction is visible

- **WHEN** a correction is applied twice or omitted
- **THEN** the control point moves by the full undulation, which is metres at
  the reviewed coastal sites, not a rounding difference
