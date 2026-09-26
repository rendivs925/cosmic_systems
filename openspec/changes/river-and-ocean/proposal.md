## Why

Ocean water is currently a flat, sea-level sphere cap and rivers are billboard-like
ribbons that only approximate the drainage network, so water reads as a painted
decal rather than a body of water from orbit or the cockpit. This change makes
ocean and river presentation physically convincing while keeping water strictly
presentation-only and deterministic.

## What Changes

- Replace the flat sea-level sphere-cap ocean with a vertex-displaced ocean surface:
  a bounded sum of directional waves with analytic normals evaluated per vertex.
- Add Fresnel sky reflection, Beer-Lambert depth absorption driven by the existing
  normalized depth vertex channel, and subsurface scattering to the water shading.
- Add crest foam (breaking wave tops) and shoreline shoaling foam, in addition to
  the existing waterline foam band.
- Allow ocean water to receive shadow maps (at least terrain shadows).
- Offer optional screen-space refraction as a configurable, default-safe effect with
  a fallback on targets that cannot support it.
- Replace the flat river ribbon with flow-directed channels that follow the
  authoritative drainage network, with width scaled by discharge/flow accumulation,
  defined banks, and a flowing surface.
- Keep all water presentation-only and deterministic: it never feeds collision,
  radar altitude, or physics.

## Capabilities

### New Capabilities
- `planetary-water`: displaced, physically shaded ocean and flow-directed river water presentation for planetary surfaces.

### Modified Capabilities
- `terrain-rendering`: terrain water presentation gains a displaced ocean surface and hydrology-aligned river channels instead of flat caps and flat ribbons.

## Impact

- Shaders/materials: `assets/shaders/water.wgsl`, `WaterExtension`/`WaterParams` in
  `src/infrastructure/bevy_adapters/terrain/water.rs`.
- Render spawn paths: ocean cap meshes and water child shadow flags in
  `src/infrastructure/bevy_adapters/terrain/render.rs`.
- River geometry: `build_river_mesh` and the surface spawn path in
  `src/infrastructure/bevy_adapters/terrain/surface/scatter.rs` and `surface/mod.rs`.
- Read-only consumption of the authoritative hydrology signal in
  `src/domain/services/terrain_source` (no new hydrology computation in water).
- Tests for deterministic displacement/geometry and presentation-only boundaries.
