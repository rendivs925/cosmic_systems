## Why

CL-06: the deck's edges were salt-and-pepper cut-outs. Two independent causes:

1. The coverage band ramped linearly from clear air to full coverage, so its
   first texel was already a fifth of the way in; sub-pixel rank noise read as a
   hard cut.
2. The density field's mip level came only from the ray step. At altitude a step
   through a nadir deck is short while one pixel covers kilometres, so the
   finest mips aliased and the edge broke into per-pixel speckle.

## What Changes

- Wrap the coverage band in a smoothstep. The endpoints and the band's mean are
  preserved, so the measured sky share and the clear-air threshold do not move,
  but the lower quarter of the band becomes the soft edge (about
  `coverage * 0.25`).
- Derive a pixel-footprint mip level from the view projection and take the
  coarser of it and the step level. Up close the footprint is smaller than a
  texel and the level is zero, so the near deck is unchanged.
- Add the shader and mirror guards.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `cloud-deck`: adds a resolved-edge requirement and a footprint-filtering
  requirement.

## Impact

- `assets/shaders/cloud_density.wgsl`: coverage smoothstep, footprint LOD, the
  base-noise resolution constant.
- `assets/shaders/clouds.wgsl`: pixel footprint from the view projection.
- `src/domain/services/cloud_noise.rs`: the coverage mirror tests.
- `src/infrastructure/bevy_adapters/terrain/render.rs`: shader guards.

## Constraints

- No extra texture fetch and no extra march step; the footprint is computed once
  per fragment.
- The footprint filter must not blur the near deck: it is zero while a pixel is
  smaller than one volume texel.
- The coverage mean must stay 0.5 so the bright-cloud share is anchored.
