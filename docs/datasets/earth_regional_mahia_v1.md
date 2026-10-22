# Earth Mahia Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Mahia / New Zealand
priority region. It is a regional payload over the resident ETOPO 2022
authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `S40E177`, covering `40..39S`, `177..178E`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_s40e177.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `318ba7635087d7ff585c0b53ee7f183d08d63e147697fa5284522afb0ee37772`.

## Region and package

- Region ID: `mahia_new_zealand`.
- Bounds: `177.70..178.00E`, `39.40..39.15S`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_mahia`.
- Payload: 326 tiles plus `manifest.ron`, 5,550,284 bytes (5.3 MiB).
- Aggregate runtime SHA-256:
  `6199e03b1cae1a5c58a97d6f10329f32c3cfad36aa495c2e4aa54db2cc9b2acf`.
- Site probe `39.2600S,177.8640E`: `58.317 m` ellipsoidal.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region mahia_new_zealand

cargo run --release --features dem --bin regional_dem_bake -- \
    --region mahia_new_zealand \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_mahia \
    --verify --bench-load --probe -39.2600 177.8640
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.185 m`, maximum absolute error `1.834 m`, manifest load
`0.821 ms`, and tile-load p95 `0.011 ms`. These are below the regional
acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the other regional roots through
`DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Rocket mode selects this site with
`cargo run -- rocket --site mahia`; site-specific mission profiles are future
work.
