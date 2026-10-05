## Purpose

Defines the physically shaded planetary water presentation (ocean and rivers)
with analytic wave normals, so surface water reads as a lit, depth-aware body of
water rather than a flat decal, while remaining deterministic and strictly
presentation-only.

## ADDED Requirements

### Requirement: Ocean surface presents bounded directional wave detail

The system SHALL evaluate a bounded sum of directional waves for the ocean
surface in the fragment stage, so wave detail varies over space and time instead
of rendering as a flat sea-level cap.

#### Scenario: Bounded wave sum shapes the surface

- **WHEN** an ocean surface is rendered
- **THEN** the shading normal is derived from a bounded sum of directional wave
  contributions that varies smoothly across the surface

#### Scenario: Wave detail stays bounded

- **WHEN** the ocean surface is shaded
- **THEN** the wave contribution stays within the configured maximum and the
  sea-level mesh never introduces gaps against the coastline

#### Scenario: Sub-pixel waves become roughness

- **WHEN** a wave component is smaller than the pixel footprint
- **THEN** it contributes to surface roughness instead of point-sampled facets

#### Scenario: Presentation clock drives motion

- **WHEN** the presentation clock advances
- **THEN** wave phase advances deterministically and no simulation or collision
  state changes

### Requirement: Ocean surface normals are analytic

The system SHALL derive the ocean surface normal analytically from the same
bounded wave sum used for shading, so lighting matches the wave shape.

#### Scenario: Analytic normal matches the wave sum

- **WHEN** the ocean is shaded by the wave sum
- **THEN** the shading normal is the analytic normal of that wave field and
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
- **THEN** the water wave normals and geometry are identical
