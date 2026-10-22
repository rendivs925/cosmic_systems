# Earth Baikonur Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Baikonur / Kazakhstan
priority region. It is a regional payload over the resident ETOPO 2022
authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N45E063`, covering `46..45N`, `63..64E`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n45e063.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `a7d7e46b325b579c4e764026173774b9ac939f90bf31c77cb612f6c201ba0274`.

## Region and package

- Region ID: `baikonur_kazakhstan`.
- Bounds: `63.05..63.55E`, `45.75..46.10N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_baikonur`.
- Payload: 796 tiles plus `manifest.ron`, 13,553,299 bytes (12.9 MiB).
- Aggregate runtime SHA-256:
  `66cbc54c90371cf4f29af2b5479e70a164c8c438dc79ff904d3ae7284864b933`.
- Site probe `45.9650,63.3050`: `80.008 m` ellipsoidal.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region baikonur_kazakhstan

cargo run --release --features dem --bin regional_dem_bake -- \
    --region baikonur_kazakhstan \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_baikonur \
    --verify --bench-load --probe 45.9650 63.3050
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.129 m`, maximum absolute error `0.706 m`, manifest load
`2.076 ms`, and tile-load p95 `0.014 ms`. These are below the regional
acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the other regional roots through
`DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Rocket mode selects this site with
`cargo run -- rocket --site baikonur`; site-specific mission profiles are future
work.
