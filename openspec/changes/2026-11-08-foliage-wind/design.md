## Context

Vegetation is one merged, alpha-masked mesh per terrain patch. The second UV
channel carries per-vertex foliage weights: `x` is the wind sway weight and `y`
the translucency weight. A vertex shader displaces the mesh; a fragment shader
scales the material's diffuse-transmission lobe. The cloud deck is advected by a
measured monthly climatology sample.

## Goals

- Foliage and clouds cannot advertise different wind directions.
- A plant bends at its attachment instead of sliding as a rigid card.
- Wind and translucency remain presentation only and add no per-frame mesh work.

## Non-Goals

- Gustiness, turbulence or a weather simulation. The package is a monthly mean.
- A new wind source. The cloud deck's measured sample is the single source.
- Any change to placement, density, collision, radar altitude or determinism.

## Decisions

### The deck's applied direction is the shared value

`apply_measured_cloud_climate` now returns the `(x, z)` vector it writes into the
cloud uniform, and the cloud system publishes that exact vector to
`PresentationWind`. Recomputing the direction in the vegetation system would
have allowed two samples to diverge; returning the applied value makes the two
consumers one computation.

### Surface wind drives the amplitude

The deck uses `2.5 x` the measured 2 m speed as mid-tropospheric advection.
Foliage feels the surface wind, so the sway mapping uses the unscaled measured
speed. Direction is shared; a vertical speed difference is physical, not a
disagreement. This is the one place the literal "speed matches the advection"
wording is interpreted as "one measured sample, one direction" rather than an
identical scalar, and it is recorded instead of hidden.

### Bounded, monotone mappings

`sway_strength = clamp(0.035 * speed, 0.10, 1.0)` metres and
`sway_frequency = clamp(0.9 + 0.07 * speed, 0.8, 3.2)` radians per second. At the
10 m/s fallback they reproduce the previously tuned 0.35 m and 1.6 rad/s, so a
missing climate package changes nothing. The floor keeps a light breeze moving
leaves, which a monthly mean otherwise smooths away.

### Sway weight follows the attachment

A card's bottom edge carries `base_sway` and its top edge 1.0. Grounded tufts and
far-field impostors use 0.0; a crown lobe uses the branch-tip weight, 0.60, which
the branch curve now also derives from the same constant. A canopy volume grades
from 0.50 at its underside to 1.0 at its top. The previous constant 1.0 slid
grass and impostor trunks across the ground by the full amplitude.

## Risks

- The monthly-mean wind is a few metres per second, so the canopy displacement
  is about 0.10-0.17 m and moves by roughly one pixel in a 130 m nadir frame.
  That is the correct physical amplitude for a monthly mean, and the capture
  reports the measured shift rather than exaggerating it.
- The deck and foliage can still move at different *rates*; only the direction
  is identical. Recorded above.

## Migration

None. Presentation only; no saved state and no baseline.
