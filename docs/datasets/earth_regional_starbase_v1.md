# Earth Starbase/Boca Chica Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Starbase / Boca Chica
priority region. It is a regional payload over the resident ETOPO 2022
authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N25W098`, covering `26..25N`, `98..97W`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n25w098.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `2b0d248e76634623e77add69b577898f3ed1ff6d3aa922b4a60b2179199de3c4`.

## Region and package

- Region ID: `starbase_boca_chica`.
- Bounds: `97.30..97.05W`, `25.85..26.10N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_starbase`.
- Payload: 232 tiles plus `manifest.ron`, 3,951,244 bytes (3.8 MiB).
- Aggregate runtime SHA-256:
  `1d3d022088d11441ac882a2060d55aa32a53d9f099385af5742f4dfeb52ae7e3`.
- Site probe `25.9971,-97.1553`: `-22.475 m` ellipsoidal, approximately `3 m`
  above the EGM2008 sea surface at Boca Chica.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region starbase_boca_chica

cargo run --release --features dem --bin regional_dem_bake -- \
    --region starbase_boca_chica \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_starbase \
    --verify --bench-load --probe 25.9971 -97.1553
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.166 m`, maximum absolute error `1.333 m`, manifest load `0.677 ms`,
and tile-load p95 `0.014 ms`. These are below the regional acceptance gates of
`1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the Papua and KSC roots through
`DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Interactive rocket mode still launches
at Papua; a Starbase launch-site/scenario selection is future work.
