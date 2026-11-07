## Context

`water_mesh_for_patch` builds a sea-level cap per terrain patch from the patch
geometry. For every vertex it computes:

```text
sea_surface_radius = planet_radius + sea_level(lat, lon) + WATER_SURFACE_OFFSET_M
ocean_depth        = max(0, sea_surface_radius - bed_radius)
has_water          = ocean_depth > 0
```

The offset is documented as a depth-test lift against the coincident far-field
globe, not as a physical sea level. Using it in `has_water` made the rendered
sea 0.5 m higher than the datum for classification purposes.

The Papua regional payload (NASADEM, 64×64 face-UV tiles) measures the plain at
0.28–0.46 m above the EGM2008 datum. The source classifies those samples as
`Land`, but the cap flooded them because they were within the offset. The same
rule also covered measured land wherever an adjacent vertex was wet, because the
cap triangle spans both and its surface sits 0.5 m above the datum.

## Goals / Non-Goals

**Goals:**

- Make the rendered wet/dry boundary agree with the terrain source's
  `surface_class` rule.
- Keep the depth-stability lift that lets water beat the far-field globe.
- Stop flooding measured land that the data calls land.
- Record the Papua determination (measured vs eroded) in a test.

**Non-Goals:**

- Change erosion, hydrology, `lake_depth_m`, collision, or terrain heights.
- Model tides, storm surge, or a time-varying sea level.
- Blend or resample the NASADEM payload; any payload seam remains a data
  concern for the ingest pipeline.
- Suppress water where the measured data is genuinely below the datum.

## Decisions

### Decision: classification at the datum, offset on the surface only

`has_water` now tests `bed_radius <= planet_radius + sea_level` (the same rule as
`TerrainSource::surface_class`), while the surface position keeps
`sea_level + WATER_SURFACE_OFFSET_M`. The two roles are separated.

### Decision: offset reduced to 0.05 m

The offset raises the *surface*, so any triangle spanning a wet and a dry vertex
draws water over the dry side up to the offset height. At 0.5 m that covered
measured land at 0.28–0.46 m. At 0.05 m it stays above the datum for depth
stability and below the measured coastal plain. The far-field globe is built
several metres below the surface (`bound_planet_fallback_radius_m` subtracts a
chord and a 2 m margin), so the lift is not load-bearing at 0.5 m.

### Decision: the Papua result is measurement, not erosion

At the observed strip the finest measured payload is 71.363 m against a
71.406–71.408 m sea: 4.3 cm below the datum, and `surface_class` already returns
`Ocean`. The eroded base over the stable global DEM is 4.0–4.1 m above the
datum. No erosion change is justified, and the `terrain-source` composition test
records all four numbers so a future change cannot silently re-attribute it.

## Risks / Trade-offs

- **Shallowest water is not drawn.** A bed less than 5 cm below the datum now
  renders as land, because the surface no longer reaches it. That is inside the
  vertical uncertainty of the measured coast and is the price of not flooding
  land.
- **Far-field depth test.** The water surface is 0.05 m above the datum instead
  of 0.5 m. A 300 km ocean/coast capture shows no fallback poking through; the
  fallback radius has a multi-metre margin.
- **Data, not rendering.** The residual Papua strip is a real below-datum
  payload sample. Removing it would require cross-tile payload smoothing in the
  ingest pipeline, which is out of scope and would also smooth real channels.

## Migration Plan

Presentation-only. Reverting the constant restores the previous depth margin;
reverting the classification restores the flooding. No persisted data changes.

## Open Questions

None.
