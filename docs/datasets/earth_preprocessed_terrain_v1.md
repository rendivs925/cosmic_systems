# Earth Preprocessed Terrain v1

## Purpose

`earth_preprocessed_cs2048_v1.csdem` and
`earth_preprocessed_cs2048_v1.cssurf` are the immutable Earth terrain
authority used at runtime when the `dem` feature is enabled. They replace
per-sample evaluation of the deterministic synthetic landscape and surface
fields with resident cube-sphere arrays.

The terrain streamer remains responsible for LOD selection, worker scheduling,
mesh construction, collision queries, and GPU uploads. These packages only
change the data source it samples.

## Contents

- `earth_preprocessed_cs2048_v1.csdem`: signed-meter elevation samples on six
  cube faces at 2048 samples per face.
- `earth_preprocessed_cs2048_v1.cssurf`: normalized `u8` moisture and river
  strength samples using the same face order, resolution, and sample locations.

The height package includes ETOPO1 elevation plus the deterministic landscape
model defined by `EarthTerrainSource`. Meshes and collision therefore query the
same baked heights. Surface material generation consumes the paired metadata,
so it does not evaluate hydrology or biome fields while streaming a patch. The
three surveyed launch/recovery pad overrides remain a static runtime overlay:
their footprints are much smaller than a 2048-face sample and must retain exact
configured elevations for fixed rocket simulation baselines.

## Regeneration

The files are generated assets and are intentionally ignored by Git. Regenerate
them from the resident ETOPO1 CSDEM after changing authoritative terrain logic:

```text
cargo run --release --features dem --bin earth_terrain_bake -- \
  assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v1.csdem \
  assets/large_files/terrain/earth_preprocessed_cs2048_v1.csdem \
  assets/large_files/terrain/earth_preprocessed_cs2048_v1.cssurf \
  2048
```

The bake is deterministic for a fixed source package and
`EARTH_SYNTHETIC_LANDSCAPE_SEED`. Change the package version and regenerate the
files whenever the source elevation model, synthetic model, face resolution, or
surface-channel encoding changes.

## Limitations

The synthetic relief remains an authored deterministic supplement to ETOPO1,
not measured topography. KSC local 3DEP data remains inactive until its vertical
datum is converted and validated against surveyed control points.
