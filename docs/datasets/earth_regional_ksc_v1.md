# Earth KSC Regional NASADEM v1

This is the interim 30 m bare-earth elevation package for the Kennedy Space
Center / Florida priority region. It is a regional payload over the resident
ETOPO 2022 authority; it does not replace the global source.

## Source

- Dataset: NASADEM_HGT v001, 1 arc-second bare-earth.
- Tile: `N28W081`, covering `29..28N`, `81..80W`.
- Source: <https://opentopography.s3.sdsc.edu/raster/NASADEM/NASADEM_be/NASADEM_HGT_n28w081.tif>
- License: public domain (NASA/NASADEM).
- Original vertical reference: EGM96 orthometric heights.
- Runtime vertical reference: WGS 84 ellipsoid / `EPSG:4979`, using the same
  EGM2008 0.5 degree geoid grid as the global authority.
- Source SHA-256:
  `62c3819f9ace4c785add8bbea485e3b031644087b62eaf5e951e3147c4573c09`.

The EGM96-to-EGM2008 geoid difference is under 1 m and is not separately
modelled; no NAVD88 correction is applied. The KSC 3DEP 1 m source remains the
planned high-resolution authority once a launch-site-valid NAVD88-to-WGS 84
control exists.

## Region and package

- Region ID: `ksc_florida`.
- Bounds: `80.85..80.45W`, `28.40..28.75N`.
- Levels: `9..12`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_ksc`.
- Payload: 455 tiles plus `manifest.ron`, 7,745,233 bytes (7.4 MiB).
- Aggregate runtime SHA-256:
  `7c0aba71a6e844535cfa557d6a3132e4f7f3f8314e7ff4d292e40954a62ff157`.
- Coastal probe `28.5721,-80.6480`: `-26.672 m` ellipsoidal, approximately
  `3 m` above the EGM2008 sea surface.
- LC-39A probe `28.6084,-80.6043`: `-22.565 m` ellipsoidal.

## Ingest and bake

```sh
python3 scripts/ingest_regional_dem.py --region ksc_florida

cargo run --release --features dem --bin regional_dem_bake -- \
    --region ksc_florida \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_ksc \
    --verify --bench-load --probe 28.5721 -80.6480
```

The package is deterministic. On 2026-10-21 the bake verified full coverage,
source RMSE `0.194 m`, maximum absolute error `1.529 m`, manifest load `2.295 ms`,
and tile-load p95 `0.022 ms`. These are below the regional acceptance gates of
`1.0 m` RMSE, `5.0 m` max error, and `50 ms` p95 load.

The runtime composes this root with the other regional roots through
`DEFAULT_EARTH_ELEVATION_MANIFEST_ROOTS`. The KSC 3DEP root is composed
immediately after it and replaces this package inside the 3DEP land rectangle.
Rocket mode selects this site with `cargo run -- rocket --site ksc`;
site-specific mission profiles are future work.
