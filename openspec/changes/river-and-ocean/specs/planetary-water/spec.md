## Purpose

Defines the displaced, physically shaded planetary water presentation (ocean and
rivers) so surface water reads as a lit, depth-aware body of water rather than a
flat decal, while remaining deterministic and strictly presentation-only.

## ADDED Requirements

### Requirement: Ocean surface is vertex-displaced by directional waves

The system SHALL displace the ocean surface in the vertex stage using a bounded
sum of directional waves, so wave height varies over space and time instead of
rendering as a flat sea-level cap.

#### Scenario: Bounded wave sum displaces vertices

- **WHEN** an ocean surface is rendered
- **THEN** each vertex is displaced vertically by a bounded sum of directional
  wave contributions and the displaced height varies smoothly across the surface

#### Scenario: Displacement amplitude is bounded

- **WHEN** the ocean surface is displaced
- **THEN** the displacement magnitude stays within the configured maximum and the
  surface never introduces gaps against the coastline

#### Scenario: Presentation clock drives motion

- **WHEN** the presentation clock advances
- **THEN** wave phase advances deterministically and no simulation or collision
  state changes

### Requirement: Ocean surface normals are analytic

The system SHALL derive the displaced ocean surface normal analytically from the
same bounded wave sum used for displacement, so lighting matches the surface shape.

#### Scenario: Analytic normal matches displacement

- **WHEN** the ocean is displaced by the wave sum
- **THEN** the shading normal is the analytic normal of that displaced surface and
  produces a moving specular response

#### Scenario: Normal remains stable at close range

- **WHEN** the camera is near the water surface
- **THEN** the analytic normal varies smoothly without abrupt flips or faceting

### Requirement: Ocean shading is physically based

The system SHALL shade the ocean with a Fresnel sky reflection, Beer-Lambert
depth absorption driven by the normalized depth vertex channel, and subsurface
scattering for the upwelling body colour.

#### Scenario: Fresnel sky reflection at grazing angles

- **WHEN** the view direction grazes the ocean surface
- **THEN** the reflected sky contribution increases and the surface becomes more
  opaque than when viewed from above

#### Scenario: Depth absorption follows the depth channel

- **WHEN** the normalized depth vertex channel increases from shoreline to deep water
- **THEN** the water colour transitions from shallow water toward deep water
  following Beer-Lambert absorption of that depth

#### Scenario: Subsurface scattering lifts lit troughs

- **WHEN** light passes through wave crests and troughs
- **THEN** the water body colour includes a subsurface scattering contribution
  rather than only a surface reflection

### Requirement: Ocean presents crest and shoaling foam

The system SHALL render foam both on breaking wave crests and along the shoreline
shoaling band, including the existing waterline foam behaviour.

#### Scenario: Crest foam appears on steep wave tops

- **WHEN** a wave crest exceeds the breaking threshold
- **THEN** foam is blended onto the crest in proportion to the crest steepness

#### Scenario: Shoaling foam appears at the shoreline

- **WHEN** the normalized depth channel is near the waterline
- **THEN** foam is blended across the shoaling band and fades into open water

### Requirement: Ocean receives terrain shadows

The ocean surface SHALL receive shadow maps, at minimum shadows cast by terrain,
while remaining non-shadow-casting.

#### Scenario: Terrain shadow darkens the sea

- **WHEN** terrain casts a shadow that overlaps the ocean surface
- **THEN** the shadowed ocean is darkened by the shadow term

#### Scenario: Water does not cast shadows

- **WHEN** water is rendered
- **THEN** it does not contribute a shadow caster

### Requirement: Screen-space refraction is optional

The system SHALL support optional screen-space refraction that is disabled by
default and falls back to non-refractive depth-based blending where the target is
unavailable.

#### Scenario: Refraction enabled

- **WHEN** screen-space refraction is enabled and supported
- **THEN** submerged terrain is refracted through the water surface

#### Scenario: Refraction unsupported

- **WHEN** screen-space refraction is enabled but the target does not support it
- **THEN** the surface falls back to depth-based blending without error

### Requirement: River channels follow the authoritative drainage network

The system SHALL build river channels that follow the authoritative drainage
network exposed by the terrain source, with width scaled by discharge/flow
accumulation, defined banks, and a flowing surface.

#### Scenario: Channel follows drainage

- **WHEN** a terrain patch is crossed by the authoritative drainage network
- **THEN** the river channel geometry follows that network rather than an
  independent render-time field

#### Scenario: Width scales with flow

- **WHEN** discharge/flow accumulation increases along a channel
- **THEN** the channel width increases accordingly

#### Scenario: Banks bound the channel

- **WHEN** a river channel is rendered
- **THEN** the channel is bounded by banks that blend into the surrounding terrain

#### Scenario: River surface flows

- **WHEN** the presentation clock advances
- **THEN** the river surface shows flow-directed motion along the channel

### Requirement: Water is presentation-only and deterministic

The system SHALL keep all water geometry and shading presentation-only, so water
never feeds collision, radar altitude, guidance, or physics, and identical inputs
produce identical output.

#### Scenario: Physics unaffected by water

- **WHEN** water is rendered with any configuration
- **THEN** collision, radar altitude, guidance, and physics state are unchanged

#### Scenario: Deterministic regeneration

- **WHEN** the same configuration and presentation clock value are applied twice
- **THEN** the water displacement, normals, and geometry are identical
