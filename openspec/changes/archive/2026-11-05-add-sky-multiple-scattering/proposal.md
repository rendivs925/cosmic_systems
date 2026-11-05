## Why

The rocket-flight sky integrates single scattering only. Measured frames show the
consequence at altitude: the 10–70 km horizon reddens toward tan/pink and the
faint upper limb tail reads teal (a blue-green band), because light that scatters
more than once is missing exactly where the ray path is longest. The current
requirement mandates a single-scattering atmosphere, so this fix needs a
requirement change rather than an implementation-only edit.

## What Changes

- The sky radiance model keeps single scattering as the chromatic and directional
  source, and adds a bounded **diffuse multiple-scattering fill** that grows with
  observer altitude and is zero in vacuum.
- The fill is a convex, luminance-preserving mix of the single-scattered
  radiance toward its luminance: it desaturates the horizon and limb toward pale
  blue-white without adding net radiance. That keeps the AT-02 absolute
  luminance sequence and the 8 km limb e-folding valid as regressions.
- The strength is bounded (`k ∈ [0, k_max]`, `k_max < 1`), so the result is
  non-negative, luminance is exactly conserved, and a floor of the original
  chromaticity always remains.
- The sky, the sea's mirror reflection and the cloud haze continue to share one
  implementation, so the reflected sky and the limb haze whiten together.
- No new LUT, render node, texture, or per-fragment sample is added. The fill
  costs one `exp` and one mix per evaluated sky direction.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `scientific-lighting`: the sky requirement changes from single-scattering-only
  to single scattering plus a bounded, documented diffuse multiple-scattering
  approximation, with an explicit energy bound and an unchanged limb
  luminance profile.

## Impact

- `assets/shaders/sky_radiance.wgsl`: the shared radiance function and the
  `SkyParams` block.
- `src/infrastructure/bevy_adapters/rendering/sky.rs`: the two bounded
  presentation constants and the energy-bound tests.
- `assets/shaders/sky.wgsl`, `water.wgsl`, `clouds.wgsl`: no source change; they
  import the shared function and inherit the fill.
- Visual measurement only: the AT-02 `sky2` and `fov6` captures are repeated.
  Terrain, collision, altitude, physics and the atmosphere model do not change.
