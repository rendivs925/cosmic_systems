# Earth ETOPO 2022 30 Arc-Second v1

The Earth terrain authority is NOAA/NCEI ETOPO 2022 v1 Ice Surface at 30
arc-second resolution, normalized from EGM2008 orthometric heights to WGS 84
ellipsoid heights. ETOPO 2022 is a seamless global relief model assembled from
airborne lidar, satellite-derived topography, and shipborne bathymetry. The
Ice Surface variant preserves the top of the Greenland and Antarctic ice sheets
and matches the previous ETOPO1 Ice Surface authority.

## Provenance

| Field | Value |
| --- | --- |
| Dataset | ETOPO 2022 v1, 30 arc-second, Ice Surface |
| Publisher | NOAA National Centers for Environmental Information (NCEI) |
| DOI | <https://doi.org/10.25921/fd45-gt74> |
| License | Public domain / CC0 (NOAA/NCEI) |
| Horizontal datum | WGS 84 geographic, EPSG:4326 |
| Source vertical datum | EGM2008 geoid, EPSG:3855 |
| Runtime vertical datum | WGS 84 ellipsoid, EPSG:4979 |

Source files:

- Surface:
  <https://www.ngdc.noaa.gov/thredds/fileServer/global/ETOPO2022/30s/30s_surface_elev_netcdf/ETOPO_2022_v1_30s_N90W180_surface.nc>
  (`ETOPO_2022_v1_30s_N90W180_surface.nc`, 1,642,335,281 bytes)
- Geoid:
  <https://www.ngdc.noaa.gov/thredds/fileServer/global/ETOPO2022/30s/30s_geoid_netcdf/ETOPO_2022_v1_30s_N90W180_geoid.nc>
  (`ETOPO_2022_v1_30s_N90W180_geoid.nc`, 1,753,951,916 bytes)

Verified source SHA-256:

- Surface: `3d76cea4fca0a056b6d8c4bcf7f4073465b975489849235309e65d44ed4ed404`
- Geoid: `610a576d1d0f17d5ab967b1b13aa7ea5ad23fa2aeaa2ae9da690734d7fd3e829`

## Vertical normalization

ETOPO tiles are referenced to the EGM2008 geoid. The published conversion is:

```text
h_WGS84 = H_ETOPO_EGM2008 + N_EGM2008
```

`scripts/ingest_etopo2022.py` adds the published geoid tile sample by sample,
rounds to the nearest signed metre (half away from zero), and writes a
north-to-south little-endian signed `i16` raster. The Python step owns the
datum conversion and records both source and target datums in the provenance
sidecar.

Normalized raster: `assets/large_files/terrain/earth_etopo2022_30s_wgs84.raw`

- 43,200 columns x 21,600 rows, 30 arc-second pixel centers
- Longitude runs -180 to 180 degrees with a periodic span (no duplicated seam)
- Rows run north to south
- WGS 84 ellipsoid metres, signed 16-bit little-endian
- SHA-256: `3ee0f6d92349b0d26844062d3198f06aee6cd410896eef5a359f485dfad3b8a9`

Area-weighted acceptance statistics (exact spherical row bands):

- Extrema: -10741 m / +8325 m
- Land fraction: 0.2905 (published ETOPO 2022 global land fraction is ~0.29)
- Mean land elevation: 792.2 m
- Mean surface elevation over the whole globe: -2392.0 m

The unweighted cell counts (land 0.3383) over-represent the polar caps and are
recorded in the provenance sidecar only for diagnostics.

## Ingestion

```sh
python3 scripts/ingest_etopo2022.py \
    --cache-dir assets/large_files/terrain/raw/etopo2022 \
    --output assets/large_files/terrain/earth_etopo2022_30s_wgs84.raw

cargo run --features dem --bin etopo2022_convert -- \
    assets/large_files/terrain/earth_etopo2022_30s_wgs84.raw \
    assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem \
    2048

python3 scripts/ingest_egm2008_geoid.py \
    --output assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw
```

The Python ingest requires `numpy` and `netCDF4`. Downloads are resumable and
range-parallel; the completed files are verified against the pinned SHA-256
values before normalization. The checked-in runtime package is generated
offline and validated at startup; no simulator path downloads source data.

The normalized raster, both source NetCDF files, and the runtime geoid grid are
generated assets under `assets/large_files/` and are not committed. The provenance
sidecar `<raster>_provenance.ron`, this document, the per-dataset config, and
the registry entry are committed.

## Runtime package

The runtime file is:

`assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem`

It stores signed 16-bit metre elevations on six cube-sphere faces in
`CubeFace::ALL` order and its complete precomputed patch-metadata pyramid
(CSDEM v2). Native Earth startup validates and deserializes this measured
package; it does not rebuild terrain metadata.

Generated runtime SHA-256:
`2e30bb5498ce9ec8a12020cefde669208f0dba09d5a6b66861fa6e9ee11d33b6`

## Retained extrema

The 2048-face package samples at roughly 4.9 km at the equator, so it does not
preserve the 30 arc-second raster's single-cell extrema:

| Quantity | 30 arc-second raster | 2048-face package | Delta |
| --- | ---: | ---: | ---: |
| Maximum height | +8,325 m | +6,767.6 m | -1,557.4 m |
| Minimum height | -10,741 m | -10,303.7 m | +437.3 m |

The loss is a deliberate Phase 2 budget decision: the package stays at the
previous 50 MiB and startup cost. It is a global relief authority, not a
summit-scale or trench-scale authority. Phase 3 regional packages and any future
higher-face global package are responsible for local extrema. Do not quote
Everest, Challenger Deep, or similar single-feature elevations from this
package.

## Sea level

Ellipsoidal heights place sea level at the local EGM2008 geoid undulation, not
at zero. `scripts/ingest_egm2008_geoid.py` samples the authoritative 30
arc-second geoid tile every 0.5 degrees through the NOAA THREDDS OPeNDAP
endpoint and writes:

`assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw`

- 720 x 360 signed i16 little-endian centimetres, 0.5 degree sample centers from
  89.5042 N to 89.9958 S
- SHA-256: `f4ea75f9721eee1d0a3fea97a05d9dda6c166bed8300e73f8e7163d1cf59ffea`
- Range: -106.71 m to +85.17 m

`GeoidModel` owns sampling. `TerrainSource::sea_level_height_m` exposes the
offset, and land/ocean classification, water-surface placement, surface
appearance, terrain overview, and the metric harness all subtract it. The geoid
grid is registered as an auxiliary runtime asset, so `preprocess --check`
re-hashes it with the CSDEM. It is small enough to regenerate quickly, ships
beside the runtime package, and never requires a download at runtime.

The previous authority remains available for rollback:

`assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem`

`DEFAULT_EARTH_DEM_PATH` selects the ETOPO 2022 package. The ETOPO1 package and
its config stay in place; switching the constant back restores the previous
authority. The erosion cache namespace is derived from the DEM path, so the
upgrade does not read stale ETOPO1-derived erosion tiles.

## Validation

`terrain_metrics` measures the real relief of the generated package; the
committed ETOPO 2022 baseline is
`tests/baselines/terrain-metrics-earth-etopo2022.ron`. Global invariants such
as extrema, land fraction, and mean elevation are reviewed against the
published ETOPO 2022 statistics. Resolution-dependent metrics are compared only
at the same grid and channel threshold. Reproduce the committed baseline with:

```sh
cargo run --release --features dem --bin terrain_metrics -- --body earth \
    --full --assert --baseline tests/baselines/terrain-metrics-earth-etopo2022.ron
```

Use `--no-geoid` only when measuring a mean-sea-level package such as ETOPO1.

The launch-site height delta report is
`docs/datasets/earth_etopo2022_30s_launch_site_delta.md`. It is generated with:

```sh
cargo run --features dem --bin terrain_site_delta -- \
    assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem \
    assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem
```

Deltas include the EGM2008-to-WGS 84 datum change, so they are not a pure
source-elevation difference. Physics baselines that consume launch-site
altitude are regenerated only after this report is reviewed.

The Rocket Mode overview is regenerated from the new authority:

```sh
cargo run --features dem --bin terrain_overview_bake -- earth \
    assets/large_files/terrain/earth_terrain_overview_v1.png
```
