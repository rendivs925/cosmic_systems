## Why

The current KSC presentation combines a coarse global ETOPO1 elevation source
with a temporary visual-only local mesh. That cannot represent launch-site
relief accurately and risks disagreement with collision, altitude, and landing
state. KSC needs a measured local elevation package, imagery aligned to that
terrain, and geodetically placed infrastructure.

## What Changes

- Add a versioned, offline-prepared local Earth elevation package contract for
  high-resolution KSC coverage derived from a reviewed public bare-earth DEM.
- Compose measured local elevation into the existing Earth terrain authority so
  terrain rendering, collision, radar altitude, and landing consume one surface.
- Add an offline conversion workflow with provenance, coordinate, vertical-datum,
  nodata, and checksum validation.
- Replace temporary visual-only KSC terrain meshes with the authoritative
  terrain-presentation path.
- Permit bounded, data-backed local terrain and imagery refinement around KSC
  while retaining ETOPO1 outside local coverage.
- Add geodetically placed KSC infrastructure presentation with explicit asset
  provenance and LOD ownership; structures remain separate from bare-earth DEM
  elevation and rocket collision.

## Capabilities

### New Capabilities
- `local-elevation-package`: Versioned, offline local DEM packages with explicit
  provenance, datum, coverage, validation, and deterministic sampling.
- `ksc-infrastructure-presentation`: Geodetically registered KSC buildings,
  launch pads, roads, and other scene assets with bounded presentation LOD.

### Modified Capabilities
- `terrain-source`: Valid measured local DEM coverage replaces global/procedural
  physical elevation for that coverage while preserving deterministic fallback.
- `terrain-rendering`: KSC render geometry and imagery must consume the same
  local elevation package as collision, without a second terrain mesh authority.
- `terrain-collision`: KSC collision, altitude, and normals must use the
  measured local elevation composed by the active Earth terrain source.
- `terrain-lod`: KSC local coverage receives bounded, data-backed refinement
  without increasing global terrain residency.

## Impact

- `src/domain/services/terrain_source.rs`, `dem_terrain_source.rs`,
  `terrain_collision.rs`, and `cube_sphere.rs` retain/extend terrain authority.
- `src/infrastructure/bevy_adapters/terrain/streaming.rs`, `render.rs`, and
  `surface.rs` consume packaged terrain data through the existing lifecycle.
- `src/infrastructure/bevy_adapters/rocket/planet.rs` removes the temporary
  local mesh presentation path.
- New offline conversion tooling, manifests, dataset provenance, local ignored
  assets, focused tests, and KSC presentation assets are required.
