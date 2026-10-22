# Earth Land Cover - Papua v1

Status: Ready, presentation-only vegetation input.

This package supplies measured broad surface classes for the Papua regional
terrain. It refines vegetation species and density only. It never contributes to
height, altitude, collision, or any physics value.

## Source

- Dataset: ESA WorldCover 2021 v200.
- Tile: `ESA_WorldCover_10m_2021_v200_S09E138_Map.tif`, 3-degree tile covering
  `138..141E`, `9..6S`.
- URL:
  `https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/ESA_WorldCover_10m_2021_v200_S09E138_Map.tif`
- Source SHA-256:
  `884c7ddcd6175d4711305f6a7e706ac2fa381867b51400e883bbacc8a001abb0`.
- Resolution and projection: 10 m, WGS 84 geographic (EPSG:4326).
- License: CC BY 4.0.
- Required attribution: `ESA WorldCover project 2021 / Contains modified
  Copernicus Sentinel data (2021) processed by ESA`.

The source is cached only in the ignored raw-asset tree. The simulator never
downloads it at runtime.

## Transformation

`scripts/ingest_landcover.py` performs the deterministic preparation:

1. Verify the pinned source SHA-256.
2. Crop to the Papua region.
3. Convert terrain-radial (planetocentric) output latitudes to WGS 84 geodetic
   latitudes before sampling, matching `regional_dem.rs` and the cube-sphere
   terrain convention.
4. Nearest-neighbour resample to a 1 arc-second terrain-radial grid. Classes are
   categorical, so interpolation is deliberately not used.
5. Map WorldCover codes to the canonical class codes in `LandCoverClass`:
   `10->0` tree, `20->1` shrub, `30->2` grass, `40->3` crop, `50->4` built,
   `60->5` bare/sparse, `70->6` snow/ice, `80->7` permanent water, `90->8`
   herbaceous wetland, `95->9` mangroves, `100->10` moss/lichen.
6. Treat nodata and unknown codes as bare/sparse. No ocean pixels are turned
   into vegetation.

Normalized equirectangular class raster:
`assets/large_files/terrain/raw/landcover/papua_coastal_lowland_1arcsec.png`,
SHA-256 `cd392794a117fc66802e7b723a0d6da763313c3c0997076282562d0529b25300`.

Reproduction:

```sh
python3 scripts/ingest_landcover.py \
  --region papua_coastal_lowland \
  --tile-url https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/ESA_WorldCover_10m_2021_v200_S09E138_Map.tif \
  --tile-sha256 884c7ddcd6175d4711305f6a7e706ac2fa381867b51400e883bbacc8a001abb0 \
  --tile-bounds 138,-9,141,-6 \
  --bounds 139.0,-8.543280680,140.0,-7.351005219 \
  --tile-path assets/large_files/terrain/raw/worldcover/S09E138.tif \
  --output-png assets/large_files/terrain/raw/landcover/papua_coastal_lowland_1arcsec.png

cargo run --release --features native --bin land_cover_bake -- \
  assets/large_files/terrain/raw/landcover/papua_coastal_lowland_1arcsec.png \
  assets/large_files/terrain/landcover_v1/papua_coastal_lowland.clcvr \
  139.000000000 -8.543280680 140.000000000 -7.351005219 \
  884c7ddcd6175d4711305f6a7e706ac2fa381867b51400e883bbacc8a001abb0 \
  'ESA WorldCover 2021 v200, tile S09E138' 'CC BY 4.0' 10.0
```

## Runtime package

- Path:
  `assets/large_files/terrain/landcover_v1/papua_coastal_lowland.clcvr`.
- Grid: `3601x4293`, north-first, 1 arc-second terrain-radial degrees.
- Bounds: `139.0..140.0E`, `8.543280680..7.351005219S` in terrain-radial
  latitude.
- Size: 15,459,573 bytes (14.7 MiB).
- Runtime SHA-256:
  `c0552a1d314e07fcac8f39305e5c3eb25f4ae875663a2283b8f7e3ad390f99ab`.
- Format: `.clcvr` v1, one class byte per sample, metadata and payload checksum
  validated at startup.
- Class fractions: tree 46.93%, shrub 0.74%, grass 3.66%, crop 0.01%, built
  0.01%, bare/sparse 8.27%, permanent water 31.42%, herbaceous wetland 7.44%,
  mangroves 1.53%.

The runtime loads every `.clcvr` file in `landcover_v1` in file-name order.
Later packages override overlapping coverage, so a regional package can refine a
future global package without changing the binary format. Outside all packages,
vegetation placement falls back to the source's climate-derived density.

## Validation

- The normalized raster is compared with the pinned source tile by construction:
  the converter only crops and nearest-neighbour remaps class codes.
- Package round-trip, checksum rejection, coverage, class-density ordering, and
  overlapping-package precedence are covered by `land_cover.rs` tests.
- `preprocess --check` verifies the runtime SHA-256.
- Scatter determinism with measured cover is covered by `surface` tests.
- The package has no vertical datum and is never read by terrain height,
  collision, or flight physics.
