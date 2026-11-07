## Why

TD-08 asked whether the geoid/ellipsoid correction is applied exactly once.
The risk was structural: the global ETOPO package stores ellipsoidal heights,
the regional NASADEM package stores EGM96 orthometric heights converted at
ingest, and every consumer needs orthometric height for land/sea work. A second
addition at ingest or a second subtraction in one consumer would move a coastal
point by the full EGM2008 undulation — 71 m at Papua — without making the
surface look obviously wrong at a glance.

The audit found one correction and one subtraction. No production code changed;
the missing piece was proof.

## What Changes

- Add a synthetic once-only test across the source, mesh, classification,
  lake-depth, and collision consumers with a known 5 m geoid.
- Add a real control-point test at the Papua site recording the EGM2008
  undulation, the measured payload's orthometric height, the graded pad height,
  the classification, the mesh height, and the collision altitude.
- State the rule in `terrain-source`: heights are ellipsoidal, the sea datum is
  the local geoid undulation, orthometric height is one subtraction, and a
  producer that ingests orthometric data converts once.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-source`: adds an explicit exactly-once sea-datum correction
  requirement.

## Impact

- `src/domain/services/dem_terrain_source.rs`: synthetic once-only test.
- `src/domain/services/terrain_source/mod.rs`: Papua control-point test.
- No production code, no data, no baseline change.
