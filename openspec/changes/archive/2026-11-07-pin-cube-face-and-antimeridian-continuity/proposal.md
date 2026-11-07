## Why

TD-09 asked for proof that terrain and imagery are continuous across cube-face
seams and the antimeridian. The risk is structural: the cube-sphere stores the
same geographic edge in two face grids, and a consumer or a bake could
interpolate the two sides differently, producing a metre-scale step exactly
where a face boundary runs through a coastline or a launch site.

The audit found the global DEM shares its edge samples exactly, the antimeridian
is continuous to millimetres at a 0.1 m separation, and every longitude
collapses onto the same polar sample. Imagery already had antimeridian wrap and
pole-clamp tests. No production code changed; the missing piece was proof.

## What Changes

- Add a face-seam test over all twelve adjacent face pairs on the resident global
  DEM: the same direction sampled through both faces must agree exactly.
- Add antimeridian and polar-clamp tests for the same DEM.
- State the rule in `terrain-source`: adjacent faces share edge samples, the
  antimeridian wraps, and the poles collapse to one sample.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-source`: adds a seam- and pole-continuity requirement.

## Impact

- `src/domain/services/dem_terrain_source.rs`: two tests.
- No production code, data, imagery, or baseline change.
