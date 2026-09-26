## Why

Streamed terrain is lit only by the shared directional Sun and a uniform sky
fill, so ridges do not shade their own valleys, hills do not darken the ground
behind them, and crevices and rock contacts read flat. The scientific-lighting
change supplies the physical Sun, sky, and near-field directional shadows, but
landscape-scale self-shadowing and ambient occlusion are still missing at the
range terrain is actually viewed.

## What Changes

- Add a baked terrain self-shadow term from a heightfield ray-march toward the
  shared ephemeris Sun direction (the fixed-sun convention already used for
  shadows), covering hills and valleys beyond the directional-shadow cascade
  range.
- Apply the self-shadow term only to the direct-sun contribution so shadowed
  terrain keeps sky/ambient fill instead of going black.
- Add a baked ambient-occlusion/sky-occlusion term for crevices, concavities,
  and ground contact, applied to the indirect sky/ambient contribution.
- Make ocean and river water receive terrain shadow instead of being excluded
  from shadow receiving.
- Reuse the existing terrain patch identity, streaming lifecycle, render-origin
  rebasing, and the authoritative `TerrainSource` height field; no second
  terrain representation, no terrain, LOD, collision, or streaming authority
  change.
- Keep bake and sampling bounded and evidence-gated through existing terrain
  performance telemetry.

## Capabilities

### New Capabilities

- `terrain-lighting-occlusion`: Baked terrain self-shadow and ambient/sky
  occlusion presentation derived from the authoritative height field and the
  shared ephemeris Sun, applied to direct and indirect lighting terms
  respectively.

### Modified Capabilities

- `terrain-rendering`: Terrain lighting gains sampled self-shadow and occlusion
  terms, and water participates in shadow receiving, without changing terrain,
  LOD, collision, streaming, or render-origin authority.

## Impact

- Terrain presentation adapters (`terrain/render.rs`, `terrain/water.rs`,
  `terrain/surface*`), the terrain surface and water shaders, the terrain
  material extension bindings, and terrain plugin registration.
- Openspec-only planning artifacts; no application code is modified by this
  change document. Implementation reuses `TerrainSource`, the ephemeris Sun
  direction, terrain patch identity, and the render origin.
- No new external dependency, no runtime data fetch, no second floating origin,
  no simulation, guidance, propulsion, coordinate, or collision change.
