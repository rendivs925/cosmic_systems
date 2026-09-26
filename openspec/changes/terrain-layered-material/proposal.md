## Why

Terrain appearance is currently one continuous CPU law (`surface_appearance`) baked
into a single small per-patch albedo/normal map, so close terrain is limited to one
hand-tuned color ramp with no distinct ground materials and no per-material
roughness or normal response. Close views expose flat, uniform ground and repeated
tiling, and layered ground covers such as grass, soil, rock, sand, and snow cannot
be tuned or validated independently.

## What Changes

- Replace the single continuous surface-appearance bake with a multi-layer terrain
  material that blends bounded ground layers (grass, soil, rock, sand, snow) using
  continuous weights derived from authoritative elevation, slope, moisture, and
  latitude inputs.
- Give each layer a PBR texture set (albedo / tangent-space normal / roughness) so
  blended normals and roughness match the blended albedo.
- Add triplanar (world-axis) projection on steep faces, blended continuously with
  the existing patch-local projection to avoid stretched or seamed detail.
- Add a low-frequency macro albedo variation, a mid-frequency micro-detail
  normal/roughness overlay, and a near-camera detail overlay that fades with
  camera distance to hide tiling at close range.
- Keep one layered-material algorithm parameterized by a data-driven layer catalog
  and budget configuration rather than per-biome special cases.
- Keep the existing per-patch generation path, `TerrainMaterial` integration, and
  terrain budgets; any budget change stays evidence-gated.
- Keep the browser (no `dem`) build on the current single-layer path.

## Capabilities

### New Capabilities

- `terrain-material-splat`: Layered ground material that blends grass, soil, rock,
  sand, and snow by slope, height, moisture, and latitude using shared PBR texture
  sets, with triplanar projection and distance-faded detail overlays, strictly as
  a deterministic presentation view over the authoritative terrain source.

### Modified Capabilities

- `terrain-rendering`: Planetary surface materials gain layered splat blending and
  distance-faded macro/micro detail overlays instead of a single baked continuous
  appearance map.

## Impact

- `src/infrastructure/bevy_adapters/terrain/surface/` extends patch surface
  preparation to emit layer weights and resolves shared layer PBR sets; the
  existing albedo/normal map generation path is reused rather than replaced.
- `assets/shaders/terrain_surface.wgsl` and `TerrainSurfaceExtension` gain layered
  blending, triplanar projection, and macro/micro/near-camera detail overlays.
- `src/infrastructure/bevy_adapters/terrain/render.rs` material construction,
  budget accounting, and patch asset release are extended for shared layer sets.
- The authoritative `TerrainSource`, `surface_appearance`, terrain geometry,
  streaming/LOD, and collision remain unchanged and authoritative; material data
  is presentation-only and never feeds collision, altitude, or physics.
- No runtime virtual texture, second terrain source, second floating origin, or
  runtime texture download is introduced. Native builds use the layered path;
  browser/no-`dem` builds retain the single-layer fallback.
