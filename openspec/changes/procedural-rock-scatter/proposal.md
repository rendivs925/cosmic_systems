## Why

Scattered surface rocks are currently lumpy UV-sphere boulders placed near the
terrain surface, with no baked shape variation, no slope-following grounding,
and no base darkening. Close-up they read as floating spheres rather than
natural geology, which breaks the illusion the rest of the premium terrain
surface works to build. The existing per-patch merged scatter path already has
the placement, noise, and mesh-accumulator infrastructure needed to fix this
without adding entities or a second rendering path.

## What Changes

- Replace the UV-sphere boulder primitive with procedurally generated rock
  geometry: an icosphere-like base shape displaced by deterministic noise, with
  optional thermal smoothing to soften talus and weathered forms.
- Give each rock varied size and aspect ratio so clusters do not read as a
  repeated sphere, while keeping a bounded per-patch rock count.
- Scatter rocks with blue-noise placement concentrated on steeper ground,
  replacing uncorrelated per-slot hashing, so scree fields form natural
  clusters rather than salt-and-pepper dots.
- Ground each rock into the slope using the authoritative `TerrainSource`
  height and surface normal, sinking its base into the terrain instead of
  resting it on top.
- Add contact ambient occlusion by darkening rock vertex colour toward the base
  so rocks read as seated in the ground.
- Preserve the existing per-patch merged `MeshAccum` mesh, LOD-count budgeting,
  and scatter-level decimation; no new ECS entities per rock and no change to
  terrain, collision, or streaming authority.

## Capabilities

### New Capabilities
- `terrain-rock-scatter`: Deterministic procedural rock geometry, slope-weighted
  blue-noise placement, slope embedding, and contact ambient occlusion for the
  per-patch scatter mesh, decimated by the existing per-patch LOD budget.

### Modified Capabilities
- `terrain-rendering`: terrain scatter presentation produces richer,
  better-grounded rock geometry within an unchanged merged-mesh and
  presentation-only contract.

## Impact

- Scatter presentation code under
  `src/infrastructure/bevy_adapters/terrain/surface/` (`scatter.rs` and the
  budget constants in `mod.rs`) plus its unit tests.
- Reuses the existing `MeshAccum`, `scatter_count_for_level`, `hash01`,
  `slope_deg_at`, `surface_normal`, and `mesh_height_m` facilities with no new
  abstraction owner.
- No physics, collision, terrain-source, coordinate, LOD-selection, simulation
  time, or streaming change; rocks remain presentation-only and never feed
  rocket collision, altitude, or landing.
