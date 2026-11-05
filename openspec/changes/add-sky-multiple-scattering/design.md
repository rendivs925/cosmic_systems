## Context

`assets/shaders/sky_radiance.wgsl` is the one sky model. It marches the
single-scattering integral along a ray, reusing `transmittance_to_sun` per step.
The dome (`sky.wgsl`), the sea reflection (`water.wgsl`) and the cloud haze
(`clouds.wgsl`) all import it. `SkyParams` is written field by field from
`AtmosphericOptics`, and a test pins the Rust and WGSL field order.

AT-02 measured the current result: at `-4.0,138.0` the +2° horizon is blue but
becomes more saturated with altitude, and the upper limb tail is a teal band
because the single-scattered path is strongly extinguished while nothing fills
the diffuse light from the rest of the atmosphere. The full Hillaire 2020
multiple-scattering treatment needs a 2D LUT computed by a sphere integral; the
project has no compute-shader LUT infrastructure and the task explicitly asks
for bounded work in the existing march.

## Goals / Non-Goals

**Goals:**

- Desaturate the high-altitude horizon and limb toward pale blue-white.
- Keep the limb luminance profile, and therefore the AT-02 24–32 km e-folding,
  unchanged.
- Keep one sky function shared by dome, sea and cloud haze.
- Bound the correction analytically and make it testable without a GPU.

**Non-Goals:**

- A second-order radiative-transfer solve or an absolute multiple-scattering
  radiance.
- A LUT, compute pass, render node, texture, or extra march samples.
- Changing the atmosphere model, aerodynamics, terrain, collision or physics.
- Recalibrating exposure or sky brightness.

## Decisions

### Decision: approximate the diffuse field as a luminance-preserving mix

The multiply-scattered field is approximated as isotropic and spectrally flat
(asymptotic source function for conservative scattering), so its visible effect
is to pull the chromatic single-scattered radiance toward its luminance:

```text
k = k_max * (1 - exp(-altitude / H))
L_out = (1 - k) * L + k * luminance(L)
```

This is a convex mix with `0 <= k < 1`: the output is non-negative, luminance is
exactly conserved, and a floor of `1 - k_max` of the original chromaticity
remains. No extra samples are needed and the luminance profile is preserved, so
the limb e-folding cannot move by construction.

Alternatives considered:

- **Hillaire multiple-scattering LUT**: physically richer, but needs a compute
  pass, a texture and a sphere integral, plus a second march; rejected as
  disproportionate to a P2 visual defect and outside "bounded samples in the
  existing march".
- **Additive gray ambient**: changes absolute luminance, so the AT-02 luminance
  sequence and exposure calibration would need re-baselining; rejected.
- **Additive Rayleigh/Mie second order**: still blue-weighted and does not
  desaturate the teal limb; it would brighten without fixing the hue.

### Decision: strength grows with observer altitude and saturates

`k = k_max * (1 - exp(-h / H))`, with `h` the observer altitude, `H` a
presentation scale height, and `k_max < 1`. At the ground the local
single-scattered source dominates and the fill is small; at altitude and the
limb the local source falls faster than the diffuse field from the dense layer
below, so the relative fill grows. `k_max = 0.6`, `H = 20 km` are declared in
`rendering/sky.rs` next to `SKY_RADIANCE_SCALE`; an airless body sets both to
zero. An observer above the atmosphere still gets the fill on rays that cross
it, which is what whitens the orbital limb.
The constants are presentation calibration, not atmosphere parameters.

### Decision: apply the fill once, after the ground term

The single-scattering march, the Sun transmittance and the ground bounce are
unchanged. The fill is applied to the total radiance at the end of
`sky_radiance_direction`, so the ground-hitting horizon band and the sky rays are
treated consistently. All three importers inherit it, including the sea's mirror
direction and the cloud haze's reflected direction.

## Risks / Trade-offs

- **The approximation is not a radiometric multiple-scattering solve.** The
  spec and the code comment state that it models the diffuse fill's desaturating
  effect, not its absolute radiance. It must not be cited as a validated
  multi-scatter radiance model.
- **A global desaturation could wash out the zenith.** The altitude gate keeps
  the fill small at the ground; the 70 km zenith is intentionally paler, which
  is the observed limb behaviour. If it reads too pale, `k_max` is the single
  dial.
- **The sea reflection and cloud haze also change.** They are intended to match
  the dome; any visible mismatch would be a bug in the shared call, not a
  tuning difference.
- **The AT-02 absolute luminance sequence is preserved** because luminance is
  conserved, but the recorded hue descriptions will change; the evidence table
  is updated rather than silently reused.

## Migration Plan

No data or API migration. If the result over- or under-corrects, set `k_max` to
zero to restore the previous single-scattering result exactly; no other code
depends on the fill.

## Open Questions

None.
