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
- UTM upper-left tie point: easting 529,993.999993 m, northing 3,170,006.000055 m.
- Nodata: -999999 m.
- Downloaded SHA-256: `66ccf1c4473b75cfa2f21cc682b3ccaf5dc391eb3b26df1555c57777c16b95d7`.

The earlier `y316` download is retained as an ignored local file but is not a
KSC source: its north edge is `28.56658` degrees, south of the project's KSC
LC-39A launch site at `28.60840` degrees. `y317` covers the launch site.

The raw source is ignored under `assets/large_files/terrain/` and is never read
by the simulator. `scripts/normalize_ksc_3dep.py` maps its NAD83 UTM samples to
a WGS 84 geographic source raster; `regional_dem_bake` then emits the validated
elevation pyramid.

NOAA VDatum Full API now provides a valid NAVD88-to-WGS 84 ellipsoid
transformation over the package. A 3x3 sample grid across the tile was fitted
with a plane (residual under 1 cm); this is the vertical control used by
`scripts/normalize_ksc_3dep.py`. The resulting package is documented in
`earth_regional_ksc_3dep_v1.md` and is composed after the KSC NASADEM root.

The active ETOPO 2022 authority uses WGS 84 ellipsoid heights plus the runtime
EGM2008 geoid sea-level model. The normalized 3DEP source is already ellipsoidal,
so its regional source declares `vertical_datum: Wgs84Ellipsoid` and the bake
does not add the geoid a second time.

The offline steps are checksum-pinned. Neither step infers a horizontal or
vertical transformation from unreviewed GeoTIFF metadata.
