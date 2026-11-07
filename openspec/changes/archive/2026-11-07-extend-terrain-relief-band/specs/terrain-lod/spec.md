## ADDED Requirements

### Requirement: Distant mesh keeps a coarse relief band

The rendered terrain SHALL carry a bounded, deterministic coarse relief band at
the LODs where features of a few kilometres are representable, so ground at
5–50 km does not render as a flat sheet. The band SHALL be absent at
planet-scale LODs and where the fine detail octave is full, and SHALL NOT be
part of collision, radar altitude, the launch-site anchor, or physics.

#### Scenario: Distant ground

- **WHEN** a terrain patch is coarse enough that the fine detail octave would
  alias but fine enough to represent a few kilometres
- **THEN** the rendered mesh carries the coarse relief band

#### Scenario: Planet-scale and close-up LODs

- **WHEN** the patch is coarser than the band or fine enough for the detailed
  octave
- **THEN** the band contributes nothing

#### Scenario: Collision is unchanged

- **WHEN** collision, radar altitude or the launch-site anchor samples the
  terrain
- **THEN** the coarse band is absent and the measured surface is unchanged

#### Scenario: Coastline is unchanged

- **WHEN** the coarse band overlaps ocean or the coastal guard band
- **THEN** it is faded out and cannot raise a seabed above the datum or lower
  measured land below it
