## Why

The bounded procedural detail octave is ~250 m wide and only appears from LOD 11
inward. At 5–50 km the patches are coarser, the octave would alias, and the
rendered ground therefore loses its relief: a lowland reads as a flat sheet.

TM-02 adds a second, coarse relief band. It is deliberately **mesh-only**: the
fine detail is part of the physical surface, but a 4 km wobble must not move
collision or the launch-site anchor, and it must not move the coastline.

## What Changes

- Add `ProceduralFarDetailSource`: a deterministic ~4 km, ±30 m relief field with
  the same launch-site attenuation as the fine detail.
- Add `FarDetailLayer`, a band-pass LOD fade (absent at planet scale, full where
  4 km resolves, absent again where the fine octave takes over) applied only by
  `LayeredTerrainSource::mesh_height_m`.
- Apply it only on land above the existing sea-datum guard, so it cannot raise a
  seabed above the datum or lower a coast.
- Wire the band into Earth's composed terrain.
- Add band-weight, sea-guard and composition tests.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `terrain-lod`: adds a mesh-only coarse relief band with a band-pass LOD fade.

## Impact

- `src/domain/services/terrain_source/procedural.rs`: far source.
- `src/domain/services/terrain_source/layered.rs`: layer, weight, mesh path.
- `src/domain/services/terrain_source/catalog.rs`: Earth composition.
- No collision, launch-site anchor, physics, or flight-baseline change.
