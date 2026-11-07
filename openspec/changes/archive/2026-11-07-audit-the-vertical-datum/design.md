## Context

Three height frames exist and must not be mixed:

1. **Ellipsoidal** (`h`): the runtime terrain frame. Global ETOPO 2022 is stored
   this way; regional orthometric sources are converted at ingest.
2. **Geoid undulation** (`N`): the local height of mean sea level above the
   ellipsoid, from the EGM2008 0.5° grid.
3. **Orthometric** (`H`): height above sea level, `H = h − N`.

`DemTerrainSource::height_m` returns `h`; `sea_level_height_m` returns `N`;
`surface_class` compares `h <= N`. Regional NASADEM is EGM96 orthometric and the
ingest adds `N` (converted to EGM2008) once in `RegionalDem::height_ellipsoid_m`.

## Goals / Non-Goals

**Goals:**

- Prove no consumer adds or subtracts `N` a second time.
- Prove the real Papua control point yields a plausible orthometric plain.
- Make the rule explicit in `terrain-source` so a future payload producer cannot
  silently add a second correction.

**Non-Goals:**

- Replace the geoid grid or change its resolution.
- Model the EGM96→EGM2008 difference separately; it is below NASADEM's vertical
  accuracy and already documented.
- Change imagery (it has no height channel).

## Decisions

### Decision: a synthetic known-geoid test enumerates the consumers

A 4×2 raster with a uniform 6 m ellipsoidal height and a uniform 5 m geoid gives
1 m orthometric land; a 3 m raster gives 2 m below sea. The test asserts
`height_m`, `mesh_height_m`, `sea_level_height_m`, `surface_class`,
`lake_depth_m`, the orthometric difference, and `radar_altitude_m` at the
surface. Any second subtraction flips the classification; any second addition
changes the difference by 5 m.

### Decision: the real control point is the Papua site

The EGM2008 undulation there is about 71.4 m, so a correction error is
unmistakable. The measured NASADEM payload is 71.978 m ellipsoidal — 0.56 m
orthometric — and the graded pad is 3.0 m over the same datum. The test records
all of it plus mesh/collision agreement at the surface.

### Decision: imagery is out of scope by construction

The imagery pipeline is a 2D albedo texture and never carries height. The
requirement names the height consumers only.

## Risks / Trade-offs

- **Test data dependence.** The control-point test reads the resident global DEM
  and the Papua payload, like the other site tests. If the package is removed the
  test fails rather than silently passing on a fallback.
- **Tolerance.** The control point records a 0.8 m window for the payload and
  0.2 m for the platform, far below the 71 m error a double correction produces.

## Migration Plan

None; tests and a specification only.

## Open Questions

None.
