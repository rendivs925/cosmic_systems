# Earth Kourou Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Kourou / French Guiana
Atlantic-coast priority region. It is a regional payload over the resident
ETOPO 2022 authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N05W053`, covering `6..5N`, `53..52W`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n05w053.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `f2709cb4365681f45695153775512033cc394440b1da5545f5c4083c5f7842e0`.

## Region and package

- Region ID: `kourou_french_guiana`.
- Bounds: `52.95..52.45W`, `5.00..5.35N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_kourou`.
- Payload: 812 tiles plus `manifest.ron`, 13,828,428 bytes (13.2 MiB).
- Aggregate runtime SHA-256:
  `dfd264e18e95f0f6fe5e5690cd83a65dac6cd8e725d6e2e0986e67e3b2c63634`.
- Site probe `5.1590,-52.6490`: `-38.213 m` ellipsoidal, approximately `3 m`
  above the EGM2008 sea surface at Kourou.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region kourou_french_guiana

cargo run --release --features dem --bin regional_dem_bake -- \
    --region kourou_french_guiana \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_kourou \
    --verify --bench-load --probe 5.1590 -52.6490
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.169 m`, maximum absolute error `1.144 m`, manifest load
`15.682 ms`, and tile-load p95 `0.057 ms`. These are below the regional
acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the Papua, KSC, Starbase, Vandenberg, and
Wallops roots through `DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Interactive
rocket mode still launches at Papua; a Kourou launch-site/scenario selection is
future work.
