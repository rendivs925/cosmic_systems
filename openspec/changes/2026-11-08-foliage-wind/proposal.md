## Why

VG-03 asks for foliage translucency and a wind response driven by the same wind
field as the cloud deck. The wind/translucency material already existed, but
reading it against the deck found two real defects.

1. The vegetation wind was a hard-coded constant `(0.82, 0, 0.57)` with a fixed
   amplitude, while the cloud deck advected on a measured climatology sample.
   Foliage and clouds could therefore drift in different directions at the same
   place and time.
2. Every vertex of a crossed card carried sway weight `1.0`, including the
   bottom edge. A grass tuft and a far-field impostor tree slid across the
   ground by the full canopy amplitude instead of bending at the root, and the
   new volumetric crowns translated rigidly and sheared off the trunk top.

## What Changes

- Publish the deck's measured advection direction into one `PresentationWind`
  resource from the same call that applies it to the cloud uniform, and read it
  in the vegetation material system. Direction is shared exactly; amplitude and
  oscillation rate derive from the measured surface wind, because the deck is
  advected by the mid-tropospheric wind and foliage feels the surface wind.
- Add `sway_strength_for_surface_wind` and
  `sway_frequency_for_surface_wind`, bounded mappings from the measured surface
  speed to canopy displacement and oscillation rate.
- Grade the sway weight across card height and across the canopy-volume height,
  with a base weight shared by the attachment (0 for a grounded tuft or
  impostor, the branch-tip weight for a crown lobe, 0.5 for the crown underside).
- Keep translucency per vertex: thin leaf and grass cards transmit backlight,
  woody vertices do not.

## Capabilities

### New Capabilities

- `vegetation-presentation`: wind sway and translucency are presentation-only
  and share one wind vector with the cloud deck.

### Modified Capabilities

None.

## Impact

- `src/infrastructure/bevy_adapters/climate.rs`: `PresentationWind` resource.
- `src/infrastructure/bevy_adapters/rocket/planet.rs`: publishes the applied
  deck direction.
- `src/infrastructure/bevy_adapters/terrain/render.rs`: vegetation system reads
  the shared wind and orders after the cloud system.
- `src/infrastructure/bevy_adapters/terrain/vegetation_material.rs`: bounded
  wind mappings.
- `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs`: height-graded
  sway weights.
- `src/application/plugins.rs`: resource registration.

## Determinism

The wind sample is a pure function of the capture pose and simulation month, so
a fixed-clock capture is reproducible. No placement, acceptance, species,
embed depth, collision or terrain sample reads the wind.
