# Earth Wallops Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Wallops / Virginia
Atlantic-coast priority region. It is a regional payload over the resident
ETOPO 2022 authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N37W076`, covering `38..37N`, `76..75W`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n37w076.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `9c4c90cf4c47bfb8f07e2a28279a501b90f952f5dcaa99891117d3a5a59a0cb5`.

## Region and package

- Region ID: `wallops_virginia`.
- Bounds: `75.70..75.35W`, `37.80..38.05N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_wallops`.
- Payload: 417 tiles plus `manifest.ron`, 7,100,160 bytes (6.8 MiB).
- Aggregate runtime SHA-256:
  `a39fb4363042654719f57b06a00e1d97d55750f7d29035c92118fe3882581f69`.
- Site probe `37.9402,-75.4665`: `-24.482 m` ellipsoidal, approximately `3 m`
  above the EGM2008 sea surface at Wallops.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region wallops_virginia

cargo run --release --features dem --bin regional_dem_bake -- \
    --region wallops_virginia \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_wallops \
    --verify --bench-load --probe 37.9402 -75.4665
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.240 m`, maximum absolute error `2.264 m`, manifest load
`1.731 ms`, and tile-load p95 `0.020 ms`. These are below the regional
acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the Papua, KSC, Starbase, and Vandenberg
roots through `DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Interactive rocket mode
still launches at Papua; a Wallops launch-site/scenario selection is future
work.
