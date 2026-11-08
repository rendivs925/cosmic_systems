## Context

`weather_coverage` turns the rank-equalised coverage channel into a local
fraction. `density_lod` picks a volume mip from the march step. The deck is
march-integrated, so a boundary is a place where density falls through zero over
a few steps.

## Goals

- A boundary that reads as a resolved edge at 400–1,200 km instead of per-pixel
  notches.
- The measured sky share and the clear-air threshold unchanged.
- No new fetch and no extra march step.

## Non-Goals

- A top-height field or a second lighting model. Sharpening with those was
  rejected before.
- A temporal accumulation or TAA dependency.
- Changing coverage, type, cell size or shadows.

## Decisions

### Smoothstep the coverage band, do not narrow it

`band = saturate((rank - (1-f)) / f)` is the old linear ramp. Returning
`smoothstep(0, 1, band)` keeps the endpoints and, because a uniform rank maps to
a uniform band, keeps the mean at 0.5. The first quarter of the band now rises
quadratically, so the transition from clear air to cloud is soft. Narrowing the
band would have raised the mean coverage and pushed the bright share up; this
does not.

### Filter to the pixel footprint

The step only knows how finely the ray is sampled. A nadir pixel at 1,200 km
covers about 2 km while the base texel is 422 m, so the fine mips carry
sub-pixel structure that the silhouette turns into noise. `footprint_lod` is
`log2(footprint / texel)` and the march uses
`max(step_level, footprint_level)`. At 1,200 km that is about 2.2 mips of
averaging; at ground level the footprint is below one texel and the level is
zero, so the near deck is untouched.

### A shared constant, guarded

`CLOUD_BASE_NOISE_RES` is 64 in the shader and `BASE_NOISE_RES` is 64 in the
generator. A mismatch would blur the wrong scale by a power of two, so a test
parses the shader literal and compares it to the generator constant.

## Risks

- Mip-averaging flattens density contrast, so a distant deck may read slightly
  thinner. That is the correct behaviour for a sub-pixel field and is preferred
  to aliasing.
- The coverage smoothstep shifts density within the band; the mean is anchored
  but the bright share is measured, not asserted here.

## Migration

None. Presentation only; no saved state and no baseline.
