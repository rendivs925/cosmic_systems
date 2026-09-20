## ADDED Requirements

### Requirement: Sunlight direction is the shared ephemeris direction

Celestial and rocket direct lighting SHALL derive the Sun direction from the
same evaluated ephemeris state used for celestial placement in that mode, in
planet-centered inertial axes, and MUST NOT use an independent or animated
light orbit.

#### Scenario: Rocket light matches ephemeris Sun

- **WHEN** rocket-mode sunlight is initialized or updated
- **THEN** the directional light's direction-to-light equals the normalized
  bound-planet-to-Sun vector from the current ephemeris snapshot

#### Scenario: Day/night cycle comes from body rotation

- **WHEN** simulation time advances on the rotating bound planet
- **THEN** the inertial Sun direction is unchanged by the planet's spin and the
  local day/night cycle is produced by body-fixed rotation passing beneath it

### Requirement: Direct sunlight illuminance follows the inverse-square law

Direct solar illuminance SHALL scale with the true planet–Sun distance so that
illuminance equals the reference value only at the calibrated reference
distance and decreases with the inverse square of the actual distance.

#### Scenario: Earth receives the reference illuminance

- **WHEN** the bound planet is at the calibrated reference distance from the Sun
- **THEN** the directional light illuminance equals the reference illuminance

#### Scenario: Outer body is dimmer

- **WHEN** the bound planet is more distant from the Sun than the reference
  distance
- **THEN** the directional light illuminance is reduced by the squared distance
  ratio and is never constant across bodies

### Requirement: Sun disc presentation is angularly correct and horizon-aware

The rendered Sun disc SHALL present the Sun's true angular diameter at the
current planet–Sun distance and SHALL NOT be visible when the Sun is below the
observer's local horizon, other than a bounded refraction allowance.

#### Scenario: Disc size tracks heliocentric distance

- **WHEN** the observer's planet is nearer to or farther from the Sun than the
  reference distance
- **THEN** the rendered solar angular radius is correspondingly larger or
  smaller than the reference angular radius

#### Scenario: Night-side Sun is hidden

- **WHEN** the Sun direction is below the local horizontal for the observer
- **THEN** the Sun disc is not rendered

### Requirement: Night sides are dark except where physically emissive

Only the Sun and bodies with a real night-emission source SHALL emit light; all
other celestial bodies MUST be unlit on their night sides and MUST NOT use
flat self-illumination to remain visible.

#### Scenario: Moon night side is dark

- **WHEN** a moon without a real emission source is viewed on its night side
- **THEN** its material contributes no self-emission and the surface is lit only
  by direct and environment light

#### Scenario: Earth night lights remain

- **WHEN** Earth's night side is viewed
- **THEN** the real night-emission texture is used and no flat albedo-wide
  constant emission is applied

### Requirement: Day/night terminator is geometric

The direct-lighting day/night boundary SHALL occur at the geometric solar
terminator, and any twilight smoothing for non-direct sky or ambient terms
SHALL use explicit solar-altitude bands that do not shift the direct terminator.

#### Scenario: Terminator at zero solar altitude

- **WHEN** a surface point has the Sun exactly on its local horizon
- **THEN** direct illumination is at its geometric transition and not offset by
  a fixed bias

#### Scenario: Twilight is bounded

- **WHEN** the observer is on the dark side within the astronomical twilight
  altitude band
- **THEN** non-direct sky and ambient terms fade according to the defined bands
  and reach their night value beyond the band

### Requirement: Rocket-flight shadows are physically directed and bounded

Rocket flight SHALL render directional shadows cast from the same ephemeris Sun,
with an explicit cascade configuration, and SHALL exclude presentation geometry
that encloses the camera so that no planet-scale shadow artifact is produced.

#### Scenario: Terrain and vehicle cast shadows

- **WHEN** rocket-flight shadows are enabled near the launch surface
- **THEN** terrain and the vehicle cast and receive shadows whose direction is
  consistent with the ephemeris Sun

#### Scenario: Enclosing globe does not self-shadow

- **WHEN** the far-field planet shell and cloud shell surround the camera
- **THEN** they are excluded as shadow casters so they do not darken the scene

### Requirement: Sky is a physically based single-scattering atmosphere

Rocket-flight sky presentation SHALL compute atmospheric single scattering from
the true planet centre, the observer's local vertical, the shared Sun
direction, and per-body atmospheric optics, and MUST NOT rely on a fixed clear
colour as the sky. Aerial perspective for distant terrain SHALL use the same
optics.

#### Scenario: Sky responds to Sun altitude

- **WHEN** the observer's local solar altitude changes from day to night
- **THEN** the rendered sky radiance and colour change consistent with
  Rayleigh, Mie, and ozone scattering rather than a fixed colour

#### Scenario: Vacuum shows stars

- **WHEN** the observer is above the modelled atmosphere
- **THEN** scattering contribution approaches zero and the background stars are
  visible

#### Scenario: Distant terrain recedes through the same atmosphere

- **WHEN** terrain is rendered through a significant air path
- **THEN** its aerial perspective is derived from the same atmospheric optics as
  the sky

### Requirement: Atmospheric optics have one authoritative deterministic source

Per-body scattering parameters SHALL have a single authoritative definition that
is independent of the Bevy renderer and produces identical values for identical
inputs.

#### Scenario: Optics are renderer-independent

- **WHEN** atmospheric optics are evaluated for a body
- **THEN** the parameters are produced without requiring a Bevy asset, window,
  or GPU

### Requirement: HDR exposure and bloom are calibrated to physical illuminance

The camera SHALL use high dynamic range output with an explicit exposure, and
bloom SHALL be thresholded from the calibrated solar luminance rather than an
unrelated constant, so that daylight, night lights, and the solar disc map to
stable display values across modes.

#### Scenario: Solar disc clips, surfaces do not

- **WHEN** a sunlit surface and the solar disc are both in view under the
  calibrated exposure
- **THEN** the disc reaches highlight range while ordinary lit surfaces remain
  within the display range

#### Scenario: Night remains readable

- **WHEN** the observer is on the night side
- **THEN** night lights remain visible and the scene is not raised by an
  unrelated ambient floor
