## Why

The bounded procedural detail layer is signed. On a coastal plain whose measured
base sits a few metres above the sea datum, its negative half can push land below
the datum, where the ocean cap renders the depression as open water. The
landscape then grows angular lakes that are in no dataset. TD-06 implemented and
unit-tested a guard, but it could not be enabled because the mechanism also
suppressed negative detail across the ocean floor, which rewrote bathymetry and
perturbed a committed orbital baseline for an unrelated reason.

## What Changes

- The sea-level guard becomes a **land** rule: on ground at or above the datum,
  negative synthetic detail may raise but must not lower the surface; positive
  detail is always applied.
- Below the datum the guard fades back out over the detail octave's own depth, so
  measured ocean bathymetry keeps its signed relief and the composed height stays
  continuous across the waterline.
- The guard window is bounded analytically from the layer's own negative
  amplitude, so the composed surface cannot cross the datum anywhere in the
  window rather than merely at the sampled cases.
- `height_m`, `mesh_height_m` and every LOD apply the same rule.
- Earth enables the guard on its detail layer. The mechanism remains opt-in per
  layer because only a body with an ocean can be flooded by it.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-source`: adds a requirement that bounded synthetic detail must not
  fabricate coastline near a planetary sea datum, with an explicit land-only
  scope and a continuous underwater transition.

## Impact

- `src/domain/services/terrain_source/layered.rs`: the removal-weight function,
  its detail-amplitude input, and the worst-case continuity tests.
- `src/domain/services/terrain_source/catalog.rs`: Earth enables the guard with a
  window derived from the detail layer's negative bound.
- No change to measured elevation, erosion, collision authority, atmosphere,
  rendering shaders, or the three committed flight baselines.
- Visual measurement only: one Papua ground-frame A/B at the same pose, time and
  frame count.
