# Earth Vandenberg Regional NASADEM v1

This is the 30 m bare-earth elevation package for the Vandenberg / California
central-coast priority region. It is a regional payload over the resident
ETOPO 2022 authority and does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N34W121`, covering `35..34N`, `121..120W`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n34w121.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the EGM2008
  0.5 degree geoid grid.
- Source SHA-256:
  `be9c3388c9ed196c23f6fcf473cf5ec7c978da569f79b60046590ee640e79e7e`.

## Region and package

- Region ID: `vandenberg_california`.
- Bounds: `120.75..120.45W`, `34.60..34.85N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_vandenberg`.
- Payload: 455 tiles plus `manifest.ron`, 7,748,198 bytes (7.4 MiB).
- Aggregate runtime SHA-256:
  `ce6f44f28d53f9286f7767668e2e70ed40842ffaf02de5f623c512f5bc4ee6ff`.
- Site probe `34.7420,-120.5720`: `68.138 m` ellipsoidal.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region vandenberg_california

cargo run --release --features dem --bin regional_dem_bake -- \
    --region vandenberg_california \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_vandenberg \
    --verify --bench-load --probe 34.7420 -120.5720
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.149 m`, maximum absolute error `1.361 m`, manifest load
`11.898 ms`, and tile-load p95 `0.122 ms`. These are below the regional
acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the Papua, KSC, and Starbase roots through
`DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. Interactive rocket mode still launches
at Papua; a Vandenberg launch-site/scenario selection is future work.
