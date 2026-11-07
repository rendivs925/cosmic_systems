## Why

TD-06 left two angular water bodies on the Papua plain and recorded them as an
open question: the detail guard provably cannot lower land above the datum, so
the water had to come from the base surface — measured or eroded.

The investigation found a third cause. The ocean cap classifies a vertex by
comparing the bed radius against `sea level + WATER_SURFACE_OFFSET_M`. The
offset exists so the translucent water wins the depth test against the
coincident far-field globe, but it was also acting as a flood threshold: every
piece of measured land within 0.5 m of the datum was drawn as sea. The Papua
regional NASADEM payload puts the plain at 0.28–0.46 m above the datum, so the
offset submerged land that the terrain source itself classifies as `Land`.

At the observed coordinates the measured payload is a few centimetres below the
datum (71.363 m against a 71.406–71.408 m sea), and the eroded base is metres
above it. The residual water is therefore measurement, not erosion.

## What Changes

- Ocean-cap classification uses the sea datum, matching `surface_class`. The
  surface radius keeps a small offset for depth stability, but that offset no
  longer decides whether a vertex is wet.
- `WATER_SURFACE_OFFSET_M` drops from 0.5 m to 0.05 m. The 0.5 m margin was
  also the height above the datum at which land was submerged by an adjacent wet
  triangle, so it hid measured coastal plains. The surface stays above the
  datum and the far-field globe stays below it.
- The Papua composition at the observed coordinates is pinned by a test:
  measured payload, sea datum, composed height, eroded base and classification.
- No erosion, hydrology, collision, or physics change.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-rendering`: the ocean cap classifies wet vertices at the sea datum,
  and the depth-stability offset is separated from that classification.

## Impact

- `src/infrastructure/bevy_adapters/terrain/render.rs`: classification rule,
  offset constant, regression test.
- `src/domain/services/terrain_source/catalog.rs`: data-backed Papua composition
  test.
- No change to terrain heights, erosion, collision, or the three flight
  baselines.
