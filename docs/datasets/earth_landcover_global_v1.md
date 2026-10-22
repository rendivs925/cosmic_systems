# Earth Land Cover - Global Fallback v1

Status: Ready, presentation-only global vegetation fallback.

This compact package gives the whole planet a measured broad land-cover class at
5 arc-minutes. The eight regional 1 arc-second packages override it wherever
they cover. It never contributes to height, altitude, collision, or physics.

## Source

- Dataset: ESA WorldCover 2021 v200, 3-degree COGs, 10 m.
- Coverage used: 2,651 land tiles, 60S to 84N.
- License: CC BY 4.0.
- Attribution: `ESA WorldCover project 2021 / Contains modified Copernicus
  Sentinel data (2021) processed by ESA`.
- Base URL:
  `https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/`.

Downloading all 10 m tiles would require roughly 120 GB. Instead
`scripts/ingest_global_landcover.py` reads only the first 128 KiB of each COG,
which contains the TIFF IFD chain and the smallest internal overview tile. The
script parses that prefix directly, so it needs neither GDAL nor tifffile.

## Transformation

1. List the 2,651 `*_Map.tif` keys from the public S3 bucket.
2. Fetch each COG's 128 KiB prefix and decompress the smallest overview.
3. Cache the overview as an ignored PNG under
   `assets/large_files/terrain/raw/worldcover_overview/`.
4. Convert 5 arc-minute terrain-radial output latitudes to WGS 84 geodetic
   latitudes before sampling.
5. Nearest-neighbour sample WorldCover classes onto the global grid.
6. Initialize ocean to permanent water; land tiles overwrite it.
7. Map WorldCover codes to `LandCoverClass` codes exactly as the regional
   packages do.

The output grid spans `180W..180E`, `60S..84N` at 5 arc-minutes. Antarctica and
the far north remain outside the package and use the climate-density fallback.

## Runtime package

- Path:
  `assets/large_files/terrain/landcover_v1/00_global_landcover_5arcmin.clcvr`.
- The `00_` prefix makes the global fallback load first; regional package
  filenames sort after it and override overlapping coverage.
- Size: 7,471,496 bytes (7.1 MiB).
- Runtime SHA-256:
  `719e0e6c67ce898489858304bab969535b484eaadf01a118578d7d7aa861ec5c`.
- Overview aggregate SHA-256:
  `253a8f13a25a9efbdb95aea3271030450010000f3610bfec9bef09b53285b27f`.
- Class fractions: tree 9.9%, shrub 1.6%, grass 6.8%, crop 2.6%, built 0.1%,
  bare/sparse 16.5%, snow/ice 1.4%, permanent water 59.0%, wetland 0.7%,
  mangroves 0.03%, moss/lichen 1.3%.

## Validation

- Package round-trip, checksum rejection, and overlapping-package precedence are
  covered by `land_cover.rs` tests.
- `preprocess --layer landcover --check` verifies the runtime checksum.
- The global package has no vertical datum and is never read by terrain height,
  collision, or flight physics.
