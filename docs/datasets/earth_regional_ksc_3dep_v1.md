# Earth KSC Regional 3DEP v1

This is the local 1 m USGS 3DEP package for the Kennedy Space Center launch area.
It refines the broader KSC NASADEM package and the global ETOPO 2022 authority.

## Source and normalization

- Source: USGS 3DEP `FL_Peninsular_FDEM_2018_D19_DRRA`, tile
  `USGS_1M_17_x53y317`, 1 m bare earth, acquired 2020-01-25.
- Source SHA-256:
  `66ccf1c4473b75cfa2f21cc682b3ccaf5dc391eb3b26df1555c57777c16b95d7`.
- Horizontal source frame: NAD83 / UTM zone 17N (`EPSG:26917`).
- Vertical source frame: NAVD88.
- Offline normalization:
  `python3 scripts/normalize_ksc_3dep.py`.
- The script reprojects the tile to 1 arc-second WGS 84 geographic samples and
  applies a NAVD88-to-WGS 84 ellipsoid correction. The correction is a plane
  fitted to a 3x3 NOAA VDatum grid over the tile; residual is under 1 cm and
  VDatum reports 0.06 m uncertainty.
- Normalized source:
  `assets/large_files/terrain/raw/ksc_3dep/ksc_3dep_ellipsoid_wgs84_1arcsec.tif`,
  SHA-256 `0eb7cf2205b13e54727ef8fa8508f6e63ded13416704d8858a4ac11aae327d50`.
- The source tile contains 3DEP nodata over water. The normalized raster emits
  only the reviewed land rectangle covering the launch site and LC-39A.

## Region and package

- Region ID: `ksc_3dep_launch`.
- Geographic intent: `80.68..80.59W`, `28.567..28.620N` geodetic. The
  configured region stores the equivalent planetocentric terrain bounds.
- Levels: `9..13`; tile resolution: `64 x 64` shared-edge samples.
- Runtime root:
  `assets/large_files/terrain/earth_elevation_pyramid_ksc_3dep`.
- Payload: 162 tiles plus `manifest.ron`, 2,758,251 bytes (2.6 MiB).
- Aggregate runtime SHA-256:
  `54d71e45d8f957f575b5959f08a945d77e25030e94985c96363bab2e7530b095`.
- Coastal probe `28.5721,-80.6480`: `-28.779 m` ellipsoidal, about `0.92 m` NAVD88.
- Probe at LC-39A `28.6084,-80.6043`: `-15.964 m` ellipsoidal.

## Bake and acceptance

```sh
python3 scripts/normalize_ksc_3dep.py

cargo run --release --features dem --bin regional_dem_bake -- \
    --region ksc_3dep_launch \
    --config assets/configs/terrain/regions_v1.ron \
    --root . \
    --output assets/large_files/terrain/earth_elevation_pyramid_ksc_3dep \
    --verify --bench-load --probe 28.5721 -80.6480
```

The bake verified full coverage, source RMSE `0.025 m`, maximum absolute error
`0.099 m`, manifest load `0.456 ms`, and tile-load p95 `0.013 ms`. These are
below the regional acceptance gates of `1.0 m` RMSE, `5.0 m` max error, and
`50 ms` p95 load.

The runtime composes this root immediately after the KSC NASADEM root, so the
finer 3DEP tiles refine the same area. Rocket mode selects the site with
`cargo run -- rocket --site ksc`.
