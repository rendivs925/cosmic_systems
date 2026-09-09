# Earth KSC USGS 3DEP v1

This is the selected measured bare-earth elevation source for the Kennedy Space
Center local terrain package. It is a USGS 3DEP 1 m source DEM from the
`FL_Peninsular_FDEM_2018_D19_DRRA` project, tile `USGS_1M_17_x53y317`.

- Source: <https://prd-tnm.s3.amazonaws.com/StagedProducts/Elevation/1m/Projects/FL_Peninsular_FDEM_2018_D19_DRRA/TIFF/USGS_1M_17_x53y317_FL_Peninsular_FDEM_2018_D19_DRRA.tif>
- License: USGS 3DEP products are public and available without use restrictions.
- Acquisition date: 2020-01-25, reported by the USGS 3DEP catalog.
- Horizontal reference: NAD83 / UTM zone 17N, EPSG:26917.
- Vertical reference: NAVD88.
- Pixel spacing: 1 m.
- Raster: 10,012 by 10,012 f32 samples, LZW-compressed GeoTIFF.
- UTM upper-left tie point: easting 529,993.999970 m, northing 3,170,006.000030 m.
- Nodata: -999999 m.
- Downloaded SHA-256: `66ccf1c4473b75cfa2f21cc682b3ccaf5dc391eb3b26df1555c57777c16b95d7`.

The earlier `y316` download is retained as an ignored local file but is not a
KSC source: its north edge is `28.56658` degrees, south of the project's KSC
launch site at `28.57210` degrees. `y317` covers the launch site.

The checked local source is ignored under `assets/large_files/terrain/` and is
never read by the simulator. An offline converter must map its NAD83 UTM samples
to the project's terrain-radial latitude/longitude coordinates and emit a validated
`CSLDEM` package before it can be enabled.

NOAA VDatum Full API was evaluated for a NAVD88-to-local-mean-sea-level (LMSL)
transformation using Contiguous US, NAD83(2011) geographic coordinates, meters,
and GEOID18. It returns `+0.209 m` with `0.069 m` uncertainty at the offshore
reference point `28.573N, 80.604W`, but returns nodata at the project launch
coordinate `28.57210N, 80.64800W`. The offshore result is not valid for KSC and
must not be applied as a constant correction. A launch-site-valid vertical datum
solution or surveyed KSC control points remain required before conversion.
The current global Earth source uses mean-sea-level heights (EPSG:5715).

`local_elevation_convert` accepts only an already normalized terrain-radial TIFF
and a provenance RON document. It verifies the normalized input checksum and
stores the metadata plus a runtime sample checksum inside `CSLDEM`; it does not
infer a horizontal or vertical transformation from GeoTIFF metadata.
