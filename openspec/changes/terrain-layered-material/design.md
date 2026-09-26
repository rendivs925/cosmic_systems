## Context

See `proposal.md` for motivation. The current presentation path is:

```text
TerrainSource (authoritative: height, slope, moisture, zone_lat)
  -> surface_appearance() one continuous law (albedo/roughness)
  -> build_patch_surfaces() per-patch 128x128 albedo + residual normal map
  -> TerrainSurfaceExtension (ExtendedMaterial<StandardMaterial, _>)
  -> assets/shaders/terrain_surface.wgsl (samples local maps, global albedo,
     imagery, and a shared triplanar micro-detail texture)
```

`TerrainMaterial` is already an `ExtendedMaterial` registered through
`MaterialPlugin`, `build_terrain_material` is the single material-construction
owner, `prepared_patch_surface`/`build_patch_surfaces` is the per-patch generation
owner, and `render.rs` already owns patch upload, budget telemetry, and asset
release. The streaming pipeline does not sample `Transform` or rendered meshes as
authority. This change extends those exact owners; it does not add a parallel
material, terrain source, coordinate system, or floating origin.

## Goals / Non-Goals

**Goals:**

- Blend a bounded, configurable set of ground layers from authoritative elevation,
  slope, moisture, and latitude inputs with continuous weights.
- Give each layer an albedo/normal/roughness set and combine normal/roughness with
  the same weight mix as albedo.
- Project layer detail triplanar on steep faces, blended continuously with the
  patch-local projection.
- Add macro variation, micro detail, and a near-camera detail overlay faded by
  view distance, evaluated per pixel for seam continuity.
- Keep one algorithm parameterized by a data-driven layer catalog and budget
  configuration; keep material strictly presentation-only.
- Preserve existing texture/upload budgets and the single-layer browser path.

**Non-Goals:**

- No runtime virtual texture, texture array atlas manager, or second material
  system.
- No change to `TerrainSource`, terrain geometry, streaming/LOD selection,
  collision, altitude, or any physical authority.
- No second floating origin or new coordinate conversion; triplanar uses existing
  render-origin-local positions.
- No runtime texture downloads and no unprovenanced media.
- No unmeasured budget increase or resolution bump.

## Decisions

### 1. Per-patch splat weights plus shared layer PBR sets, not a runtime virtual texture

Generate a per-patch layer-weight map (one RGBA8 map, extended to a second map only
if the bounded layer count requires it) from the authoritative source, and sample
shared tiling layer PBR sets with it. This reuses the existing per-patch generation,
LOD fade, upload, and eviction lifecycle. A runtime virtual texture was rejected for
the first implementation: it adds a new GPU feedback/atlas/streaming subsystem,
significant residency, and native-only behavior, with no measured evidence that the
existing per-patch budget is insufficient. A single global splat map was rejected
because it cannot follow patch LOD and would blur close detail.

### 2. Reuse `TerrainMaterial`, `terrain_surface.wgsl`, and the per-patch generation path

Extend `TerrainSurfaceExtension` bindings and the fragment shader; extend
`build_patch_surfaces`/`PreparedPatchSurface` to emit the layer-weight data. Do not
introduce a new material type, shader, or patch-preparation system. `build_terrain_material`
remains the one place a patch material is constructed so the imagery upgrade path and
layer path stay a single code path. The existing global albedo and imagery
enrichment remain the geographic base under the layered material.

### 3. Weights derive from authoritative inputs and stay presentation-only

Layer weights are a pure function of already-sampled authoritative values:
`height_m`, `slope_deg_at` (or `overview_slope_deg` at coarse LOD), `moisture`,
and `zone_lat`. The weight model, not a biome lookup, produces ecotones, mirroring
the existing continuous `surface_appearance` philosophy. Material data is emitted
only into `PreparedPatchSurface` and GPI asset state; nothing in the collision,
altitude, or fixed-step physics path may query it. The existing
`surface_appearance` law is retained as the fallback base/albedo contribution and
as the single-layer path, not deleted.

### 4. One layer catalog resolves PBR sets, offline and deterministic

A single data-driven layer catalog defines layer identity, weight parameters,
tiling scale, and its albedo/normal/roughness source. Native builds resolve
offline-prepared sets with recorded provenance, or a deterministic procedural set
generated at startup when no external asset is present; browser/no-`dem` builds
skip the layered path entirely. A hardcoded texture path per biome was rejected
because it scatters asset ownership and provenance; external runtime fetching was
rejected because it breaks offline and deterministic requirements.

### 5. Triplanar projection blended by orientation, one algorithm

Sample layer detail by world-axis projection and blend it with the patch-local UV
projection using the surface's orientation relative to the patch normal. This reuses
the projection approach already used by `sample_detail_triplanar` and the existing
`dpdx`/`dpdy` tangent reconstruction. A separate steep-only material variant was
rejected because it would duplicate the blend and risk a hard material boundary.

### 6. Multi-scale detail overlays with per-pixel distance fade

Reuse the existing `value_noise` macro variation and shared micro-detail texture,
and add a nearer, higher-frequency detail overlay. Every overlay contribution is
scaled by a per-pixel `smoothstep` over view distance, matching the existing
`DETAIL_FADE_START_M`/`DETAIL_FADE_END_M` and micro fade so neighbouring LODs agree
at shared edges. The overlay provides grain only; it never changes geometry.

### 7. Data-driven, evidence-gated budgets

Texture resolution and per-patch map count stay at their current values; per-patch
cost is the layer-weight map plus the shared layer sets, which are sampled by all
patches and uploaded once. Budget configuration is centralized with the existing
terrain budget owners, and any increase requires before/after measurements. The
existing per-frame upload cap, cache eviction, and patch asset release are extended
rather than bypassed.

### 8. Native-first with a single-layer browser fallback

The layered path is selected at material-construction time from build capability
and catalog availability. When unavailable, patches keep the current neutral
maps and `surface_appearance` albedo path so WASM rendering, streaming, and
collision are unchanged.

## Risks / Trade-offs

- [Layered material raises fragment cost and texture bandwidth] -> Keep a bounded
  layer count, share texture sets across patches, cap per-patch maps, and gate any
  budget increase on measured native frame-time evidence.
- [Blended normals/roughness can look wrong or over-strong] -> Combine normal and
  roughness with the same weights as albedo, keep detail gains bounded, and add
  regression tests on the pure weight and blend functions.
- [Per-patch map generation cost blocks workers] -> Generate weights only at the
  existing `LOCAL_SURFACE_MIN_PATCH_LEVEL` detail ring and reuse the existing
  worker budget and upload cap.
- [Triplanar can seam or pop at projection transitions] -> Blend projection
  continuously by orientation and reuse per-pixel fade; test edge continuity across
  adjacent LODs.
- [Procedural vs external PBR sets could diverge visually] -> Resolve all layers
  through one catalog, record asset provenance, and require deterministic
  procedural fallback output.
- [Material data could leak into physics] -> Keep emission confined to
  `PreparedPatchSurface`/GPU asset state, document the boundary, and add a test
  asserting collision/altitude paths never read material data.
- [Browser fallback could silently break] -> Select the fallback explicitly at
  material construction and validate both build configurations.

## Migration Plan

1. Add the pure layer-weight model and its deterministic regression tests without
   changing rendering.
2. Extend per-patch preparation to emit layer weights while keeping the existing
   albedo/normal output as fallback.
3. Extend `TerrainSurfaceExtension` and the shader with layered blending, triplanar,
   and detail overlays behind the layered path.
4. Wire the layer catalog, shared asset resolution, budget accounting, and patch
   asset release in `render.rs`.
5. Validate native `dem` layered behavior, browser/no-`dem` single-layer fallback,
   terrain streaming, collision, and mode startups; keep budget changes gated on
   measurements.

Rollback is the catalog/config switch back to the single-layer path; no terrain or
collision data is migrated.

## Open Questions

- Final ground-layer count and exact layer parameters are configuration values to
  be tuned against the bounded budget; they do not change the algorithm, specs, or
  task breakdown.
