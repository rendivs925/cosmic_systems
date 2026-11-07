## ADDED Requirements

### Requirement: Vegetation exclusion uses height above the sea datum

Vegetation placement SHALL derive its land/sea, species and treeline decisions
from height above the terrain source's local sea datum, not from the raw
coordinate. A body that stores ellipsoidal heights SHALL subtract its datum
before applying the 0.5 m land threshold or the treeline band.

#### Scenario: Shallow shelf

- **WHEN** a coastal patch contains a shelf that is 30 m below the sea but 40 m
  above the ellipsoid
- **THEN** no tree site is accepted there

#### Scenario: Forested coast

- **WHEN** a coastal patch contains forested land above the datum
- **THEN** it still hosts its accepted tree sites

#### Scenario: Treeline

- **WHEN** the ground rises through the treeline
- **THEN** density falls to zero by the band's upper edge, measured above the
  same datum

### Requirement: Placement identity is versioned

The placement generator SHALL expose a committed version constant in addition
to its fixed seed, so a replay or terrain cache can distinguish worlds whose
placement law changed while the seed stayed the same.

#### Scenario: Identity

- **WHEN** the placement law changes
- **THEN** the generator version changes with it, and the current seed, salt and
  lattice level are pinned by a test
