## Why

Celestial and rocket lighting is currently approximated: sun illuminance is a
fixed Earth value at every body, the Sun disc uses a fixed angular size, the
night sides of planets and moons are kept visible with a fake flat emissive,
shadows are disabled, and the sky is a hand-tuned clear colour plus distance
fog. This hides the real terminator, produces physically wrong brightness away
from one AU, and cannot express a real atmosphere. The simulator already has
authoritative ephemeris, flight, and terrain state to derive physically correct
presentation instead.

## What Changes

- Derive direct sunlight radiance from the ephemeris planet–Sun distance with
  inverse-square falloff instead of a fixed Earth illuminance.
- Remove the fake flat night-side emissive from every non-Sun body; keep only
  the Sun emissive and Earth's real night-lights texture.
- Place the day/night terminator geometrically from the solar altitude and use
  explicit twilight bands for sky/ambient instead of a biased daylight curve.
- Size the Sun disc from the actual planet–Sun distance and hide it when it is
  below the local horizon.
- Enable rocket-flight directional shadows with explicit cascade configuration,
  excluding the enclosing far-field globe and clouds so no planet-scale shadow
  artifact appears.
- Replace the flat sky and distance fog in rocket flight with a physically based
  single-scattering atmosphere evaluated against the true planet centre and
  local vertical, including aerial perspective for distant terrain.
- Introduce a Bevy-free atmospheric-optics value object as the single source of
  Rayleigh/Mie/ozone parameters per body.
- Calibrate HDR exposure and bloom to the physical solar illuminance.

## Capabilities

### New Capabilities
- `scientific-lighting`: Physically directed and calibrated sunlight, day/night
  terminator, horizon-correct Sun disc, bounded directional shadows, and a
  single-scattering planetary sky derived from shared ephemeris and flight
  state.

### Modified Capabilities
- `terrain-rendering`: terrain rendering gains physically directed shadow
  participation and aerial perspective driven by the shared atmosphere model,
  without changing terrain, LOD, collision, or streaming authority.

## Impact

- Rocket presentation adapters (`rocket/environment.rs`, `rocket/planet.rs`), a
  new rocket sky material/shader and adapter, solar-map startup materials and
  lights, the terrain surface shader, and Bevy plugin registration.
- One new domain value object for atmospheric optics; no physics, guidance,
  propulsion, terrain-source, coordinate, simulation-time, or collision change.
- Camera post-processing (HDR + exposure + bloom threshold) is calibrated; the
  shared camera is used by all modes, so normal, craft, and rocket startups are
  re-validated.
- No new external dependency, no runtime data fetch, and no second floating
  origin.
