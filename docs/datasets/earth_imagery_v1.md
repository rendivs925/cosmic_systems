# Earth Offline Imagery Package v1

## Status

The repository provides the deterministic cube-sphere package format and
converter, but intentionally does not ship a source imagery package. Operators
must select a reviewed, redistributable source and update the accompanying RON
manifest before enabling it.

## Contract

- Body: Earth
- Frame: IAU_EARTH body-fixed directions
- Tile identity: cube face, quadtree level, tile x, tile y
- Encoding: RGBA8 PNG tiles generated from pixel centers
- Runtime: optional presentation data only; no network access, collision, or
  terrain-height authority

## Preparation

```text
cargo run --bin earth_imagery_convert -- \
  source-equirectangular.png \
  assets/large_files/terrain/earth_imagery_v1/tiles \
  0 12 256
```

Record the source URL, version, redistribution license, SHA-256, geographic
datum, coverage, and source resolution in
`assets/configs/terrain/earth_imagery_v1.ron`. Generated package files remain
ignored under `assets/large_files/`.
