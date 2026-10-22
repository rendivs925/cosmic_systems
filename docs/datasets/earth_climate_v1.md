# Earth Monthly Climatology v1

Status: Ready. One global presentation-only `.csclim` package built from NASA
POWER monthly climatology. No runtime downloads.

This package supplies measured seasonal context to the existing sky and
volumetric-cloud presentation. It never contributes to terrain height,
collision, altitude, flight dynamics, or the authoritative atmosphere model.
The 1976 U.S. Standard Atmosphere remains the single source for density,
pressure, and speed of sound consumed by rocket physics.

## Source substitution

Phase 6 named ERA5/ERA5-Land, but CDS credentials and `cdsapi` were unavailable
when this layer was implemented. Rather than fabricate climatology or add an
unauthenticated download path, the layer uses NASA POWER v10 Analysis Ready
Data, which is public, requires no authentication, and publishes global monthly
climatology under CC BY 4.0:

| Field | Source | Native grid |
| --- | --- | --- |
| 2 m temperature | MERRA-2 via POWER | 0.5 x 0.625 degree |
| surface pressure | MERRA-2 via POWER | 0.5 x 0.625 degree |
| 2 m relative humidity | MERRA-2 via POWER | 0.5 x 0.625 degree |
| 2 m eastward/northward wind | MERRA-2 via POWER | 0.5 x 0.625 degree |
| cloud amount | CERES SYN1deg via POWER | 1 degree |

The published climatology period is 2001-01-01 through 2020-12-31. ERA5 remains
a future option if CDS access is provisioned; the package format and sampler do
not depend on the source.

- MERRA-2 base:
  `https://nasa-power.s3.us-west-2.amazonaws.com/merra2/spatial/power_merra2_climatology_spatial_utc.zarr`
- CERES SYN1deg base:
  `https://nasa-power.s3.us-west-2.amazonaws.com/syn1deg/spatial/power_syn1deg_climatology_spatial_utc.zarr`
- License: CC BY 4.0.
- Required attribution: `NASA Prediction of Worldwide Energy Resources
  (POWER); MERRA-2; CERES SYN1deg`.

The ingest script records a deterministic aggregate SHA-256 over every fetched
Zarr chunk (`sha256(chunk)  <chunk-url>` lines, sorted by URL). The registry
records that aggregate as `source_sha256`.

## Transformation

`scripts/ingest_climate.py` reads the POWER Zarr chunks directly. It decodes
the Blosc container through the system `libblosc` with `ctypes`, applies the
stored `fixedscaleoffset` filter, and avoids adding `xarray`, `zarr`, or
`numcodecs` as Python dependencies.

For each of the twelve months it:

1. Reads MERRA-2 temperature, pressure, humidity, and wind on the source grid.
2. Reads CERES SYN1deg cloud amount and bilinearly resamples it to the MERRA-2
   horizontal grid with periodic longitude.
3. Converts the output terrain-radial latitudes to WGS 84 geodetic latitudes
   before sampling the geodetic source grid.
4. Writes one interleaved little-endian float32 raster in
   `[month][row][column][channel]` order, north-first, with channels
   `temperature degC`, `pressure hPa`, `relative humidity %`,
   `wind east m/s`, `wind north m/s`, and `cloud amount %`.

`src/bin/climate_bake.rs` quantizes each channel and writes the versioned
package. All twelve months are retained; the sampler interpolates cyclically
between adjacent months, so January follows December without a discontinuity.

## Runtime package

File: `assets/large_files/terrain/climate_v1/earth_merra2_climatology_v1.csclim`

| Property | Value |
| --- | --- |
| Grid | 576 x 361 cells, north-first, terrain-radial |
| Bounds | 180W..180E, 90S..90N |
| Months | 12 |
| Quantization | 0.01 degC, 0.05 hPa, 0.01 %, 0.01 m/s |
| Package bytes | 29,943,778 |
| Runtime SHA-256 | `07e7808345dc980925d6c0214af0059c163a11c3bead75f49356d9d4a7ed8db1` |

A global circle is treated cyclically by the sampler, so the antimeridian has no
seam. Longitude wrapping and month interpolation are covered by
`src/domain/services/climate.rs` tests.

## Presentation consumers

- **Sky:** measured surface pressure scales the sea-level Rayleigh and Mie
  coefficients inside a `0.5..1.1` ratio clamp. This is an optical-depth
  adjustment, not a physics change.
- **Clouds:** measured cloud amount drives the deck coverage multiplier;
  measured wind speed and direction drive advection. Wind is projected into the
  body-fixed x/z plane used by the cloud uniform, with a `2.5x` scaling from
  2 m wind toward mid-tropospheric advection and a `2..60 m/s` clamp.
- **Seasonality:** month fraction comes from the authoritative simulation UTC
  Julian date, so a launch that crosses a month boundary blends continuously.

The procedural Perlin/Worley fields remain the spatial texture. Measured data
modulates their aggregate coverage and motion; it does not paint individual
clouds from the coarse reanalysis grid.

## Validation

```sh
python3 scripts/ingest_climate.py
python3 scripts/ingest_climate.py --verify
cargo run --release --features native --bin climate_bake -- ...
cargo run --features dem --bin preprocess -- --layer climate --check
```

`--verify` compares the prepared raster with the POWER point climatology API at
Starbase, KSC, Kourou, and Mahia for all twelve months. Tolerances are 1.5 degC,
5 hPa, 10 % relative humidity, 1.5 m/s wind, and 10 % cloud amount; the current
package passes all four sites.

No physics baseline was regenerated. The flight atmosphere and all terrain
height/collision authorities are unchanged.

## Limitations

- The horizontal resolution is the MERRA-2 native grid (about 55 km at the
  equator), so the package cannot represent local microclimate, urban heat
  islands, or individual cloud systems.
- Cloud amount is total column cloud fraction from CERES, not a vertical cloud
  profile. Cloud base/top remain the presentation shell constants.
- The package stores surface fields only. A future measured atmospheric profile
  would need a separate vertical-level dataset and a physics-baseline review
  before it could replace the standard atmosphere.
- ERA5/ERA5-Land at 0.25 degree remains the preferred future source when CDS
  access is available; the current source is a documented open substitute, not
  a claim of ERA5 equivalence.
