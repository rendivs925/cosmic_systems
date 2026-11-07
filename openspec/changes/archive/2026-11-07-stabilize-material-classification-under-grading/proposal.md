## Why

A launch pad is graded flat: the composed height is pulled to the site anchor
inside the flat radius and restored over the blend. That is correct for geometry,
collision, radar altitude and the rendered mesh. But the surface material law
also reads the slope, and the slope came from the graded height. On a steep site
the footprint therefore classified as flat ground — a rock hillside turned into
a grass/soil disc the size of the grade.

LF-01 reduced the footprint to 110 m to make the disc less visible. TM-01 fixes
the cause: classification reads the natural surface, geometry stays graded.

## What Changes

- Add `TerrainSource::appearance_height_m`, an appearance-only height that may
  exclude artificial grading. It defaults to `height_m`.
- `LayeredTerrainSource` returns its ungraded composition from that method.
- `slope_deg_at` and the per-vertex surface maps derive their slope from the
  appearance height, while elevation bands (shore, snow) still follow the graded
  surface actually shown.
- Add tests for a gentle grade and a 45° hillside, including the control that a
  flat classification would recolour the pad.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `planetary-terrain-material`: surface classification ignores an artificial
  site grade while geometry and elevation bands follow the shown surface.

## Impact

- `src/domain/services/terrain_source/mod.rs`: trait method.
- `src/domain/services/terrain_source/layered.rs`: override and tests.
- `src/domain/services/terrain_source/appearance.rs`: slope sampling.
- `src/infrastructure/bevy_adapters/terrain/surface/surface_maps.rs`: appearance
  slope grid.
- No collision, physics, or baseline change.
