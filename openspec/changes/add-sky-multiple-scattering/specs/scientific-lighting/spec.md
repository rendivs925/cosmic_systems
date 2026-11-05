## ADDED Requirements

### Requirement: Sky includes a bounded diffuse multiple-scattering fill

Rocket-flight sky presentation SHALL augment the single-scattered radiance with a
bounded diffuse multiple-scattering approximation. The approximation SHALL be a
convex, luminance-preserving mix of the computed radiance toward its luminance,
with a strength `k` in `[0, k_max]` where `k_max < 1`. It MUST NOT add net
luminance, MUST NOT produce negative radiance, and MUST be zero for an airless
body or a ray that misses the atmosphere. An observer above the atmosphere
whose ray crosses it still receives the fill on that ray.

The fill strength SHALL grow with observer altitude, because the local
single-scattered source falls faster than the diffuse field below it, and SHALL
saturate at `k_max`. The same shared radiance function SHALL serve the sky dome,
the sea's mirror reflection and the cloud haze.

#### Scenario: Horizon desaturates toward pale blue-white

- **WHEN** the observer looks at the +2° horizon from 10, 30 or 70 km in daylight
- **THEN** the rendered hue is less saturated than the single-scattering result
  and trends toward pale blue-white rather than tan, pink or teal

#### Scenario: Limb luminance profile is unchanged

- **WHEN** the 24–32 km limb luminance profile is measured before and after the
  fill
- **THEN** its e-folding stays within 8 ± 1 km, because the fill is
  luminance-preserving and adds no samples to the march

#### Scenario: Vacuum and airless bodies are untouched

- **WHEN** the body has no atmosphere, or a ray misses the atmosphere entirely
- **THEN** the fill is zero and the sky is the single-scattering/vacuum result

#### Scenario: Energy bound holds

- **WHEN** the fill is applied to any non-negative radiance with a strength in
  `[0, k_max]`
- **THEN** the output is non-negative, its luminance equals the input's, and it
  lies on the convex segment between the input and its luminance-equivalent
  gray, so no channel overshoots

#### Scenario: Reflected sky and haze match the dome

- **WHEN** the sea reflects the sky or cloud haze replaces a limb cloud's light
- **THEN** they evaluate the same filled radiance function as the dome, so the
  reflected and hazed colours cannot diverge
