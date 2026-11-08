## Context

Every vegetated patch at or above `VEGETATION_MIN_PATCH_LEVEL` builds a merged
trunk/branch/crown mesh. Below that gate the same trees are crossed impostor
cards. The crown itself was crossed billboards in both cases.

## Goals

- A near-band crown reads as a mass from every direction, not as intersecting
  planes.
- The change stays inside the streaming budget that already reserves the
  worst-case vegetation mesh for each patch.
- Placement is untouched.

## Non-Goals

- Higher-density foliage or a LOD transition for the crown itself.
- Translucency and wind (VG-03).
- Any change to the far-field impostor cards.

## Decisions

### A jittered ellipsoid shell, not a card stack

Three latitude rings plus two poles, six longitudes, with a deterministic radial
jitter of 0.82-1.18 so the crown is lobed rather than a smooth ball. Normals are
the outward radial direction, so the crown shades around its own silhouette.
`CANOPY_VOLUME_RINGS`/`CANOPY_VOLUME_SEGMENTS` are the single source of truth for
the size, and the byte reservation is computed from them.

### Outward winding

`(tangent, bitangent, up)` is right-handed and the azimuth runs tangent to
bitangent, so the outward normal is `up x azimuth`. The first emission order
faced inward; the crown rendered as a hollow ring with the ground visible through
the middle.

### Branch tips stay cards

A full change to nine shells per tree costs +50 % of the reservation and pushed
the finest terrain patch out of the 160 MiB streaming budget. The flatness that
mattered was in the stacked crown layers; a 2-plane tip lobe costs a twentieth of
a shell and is barely visible at its size.

## Risks

- The reservation is now within a few hundred kilobytes of the point where the
  finest patch stops fitting. `papua_prelaunch_selects_and_requests_scatter_lods`
  guards exactly that.
- The crown is still a shell, not a leaf mass: at very close range it reads as
  faceted. That is the intended low-poly style, not a measurement.

## Migration

None. No saved state, no artifact schema, no baseline.
