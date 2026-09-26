## Why

Close-range terrain currently scatters low-poly plants with a uniform per-patch random hash, so trees appear evenly dotted regardless of terrain, species differ only by a broadleaf/conifer billboard swap, plant bases float off slopes, and no measured ecological signal is consumed. This reads as synthetic and blocks the world-class simulator goal. Deterministic, ecologically grounded, measured vegetation is the prerequisite before wind, animation shading, or instancing work can be credible.

## What Changes

- Replace uniform random scatter with deterministic per-patch blue-noise / Poisson-disk placement seeded from patch identity and the simulation seed.
- Add a low-frequency clumping/clearing mask plus ecological rules: altitude band, slope limit, water/moisture, and per-species spacing.
- Add an offline, versioned measured land-cover package (for example ESA WorldCover) consumed as a presentation-only resource that drives species mix and density.
- Add distinct deterministic species geometry (tropical broadleaf, temperate broadleaf, conifer/boreal, palm, shrub/understory, grass) generated from bounded baked skeletons.
- Keep the current merged per-patch scatter mesh, worker-task generation path, and scatter budgets.
- Fix grounding: embed plant bases into the local slope and align plant up to the terrain surface normal.
- **Deferred:** wind/animation shading and GPU instancing/impostors remain out of scope for this change.

## Capabilities

### New Capabilities
- `vegetation-placement`: deterministic per-patch blue-noise placement, clumping/clearing mask, and ecological placement rules.
- `vegetation-species`: deterministic species selection and distinct bounded baked geometry per species, merged into the existing per-patch mesh.
- `vegetation-land-cover`: offline measured land-cover package consumed as a presentation-only signal for species mix and density.

### Modified Capabilities
- `terrain-rendering`: close patches render a merged vegetation mesh produced from deterministic placement, species geometry, and measured land cover, while remaining presentation-only.
- `terrain-source`: documents the climate-derived `vegetation_density` as the procedural fallback signal and states that measured land cover is a presentation-only overlay.

## Impact

- Presentation/domain code: `src/infrastructure/bevy_adapters/terrain/surface/` (`scatter.rs`, `mod.rs`, `surface_maps.rs`) and the terrain worker task in `src/infrastructure/bevy_adapters/terrain/streaming.rs`.
- Domain: `src/domain/services/terrain_source/` (documented fallback density) plus a new pure placement/species domain module and a land-cover package reader alongside `imagery_package.rs` / `local_elevation.rs`.
- Configuration: new vegetation/placement/species and land-cover paths and budgets.
- Assets: an optional offline land-cover package under `assets/large_files/terrain/`; no binary package is committed by default and a deterministic fallback is used when it is absent.
- No changes to terrain height authority, collision, rocket physics, or reference frames; no new ECS per-plant entities.
