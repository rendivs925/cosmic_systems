# Earth Imagery v1

Earth presentation imagery is an offline-prepared hierarchy with one
public-domain global overview and one bounded high-detail local region. It
refines visible terrain from a globe overview to satellite detail without
becoming a terrain-height, collision, or physics authority, and the simulator
never downloads imagery at runtime.

Manifest: `assets/configs/terrain/earth_imagery_v1.ron`

## Global overview

- Dataset: NASA Blue Marble: Next Generation, August 2004 composite with
  topography and bathymetry.
- Source:
  <https://eoimages.gsfc.nasa.gov/images/imagerecords/74000/74117/world.200408.3x21600x10800.jpg>
- License: public domain (NASA). NASA material is not protected by copyright
  unless noted.
- Attribution: NASA Earth Observatory / Reto Stockli (NASA GSFC).
- Coverage: global, seamless true-colour mosaic.
- Resolution: 21600 by 10800 source samples, 2 km per pixel; the runtime asset is
  downsampled to 8192 by 4096 with Lanczos resampling.
- Horizontal reference: WGS 84 geographic (EPSG:4326), Earth body-fixed.
- Downloaded source SHA-256:
  `926be7253be90a1fd57071a33afbfe7a712a2dfe0d5b5b4602bc5dfbcfdc2c89`.
- Runtime asset SHA-256:
  `9b108ac20def521dfa6035d906e4fa33097cfd205689b0f3211d4b05e416373f`.
- Role: Earth's global fallback albedo. When the package verifies at startup,
  this overview replaces the catalog texture as the texture every terrain patch
  starts from, so uncovered patches are not limited to the small source-derived
  global albedo. A missing or invalid package leaves the catalog albedo in
  place.

## Local high-detail region

- Region id: `papua_coastal_lowland`.
- Dataset: Copernicus Sentinel-2 Level-2A true colour (TCI) surface reflectance.
- Source: the Sentinel-2 L2A COGs on AWS Open Data served through the Earth
  Search STAC API.
- License: Copernicus Sentinel Data is provided on a free, full and open basis
  under the Legal Notice on the use of Copernicus Sentinel Data and Service
  Information (<https://sentinels.copernicus.eu/documents/247904/690755/Sentinel_Data_Legal_Notice>).
  Redistribution and adaptation are permitted with the required attribution.
- Required attribution notice: `Contains modified Copernicus Sentinel data 2026`.
- Acquisition: 2026-08-22, Sentinel-2B, one MGRS scene per tile, true colour TCI
  at 10 m per pixel. Four scenes cover the region:
  `54LTR`, `54LUR`, `54MTS`, `54MUS` (UTM zone 54S, EPSG:32654).
- Coverage: a complete mosaic of the nominal bounding box
  `139.0..140.0` east and `8.6..7.4` south (100.000 % filled by the four scenes).
- Horizontal reference: WGS 84 geographic (EPSG:4326), Earth body-fixed. The UTM
  sources are reprojected with `scripts/imagery_reproject_utm.py`.
- Level range: `min_level = 8`, `max_level = 12`, `resolution = 256`. This yields
  5009 produced tiles (24 at L8, 75 at L9, 263 at L10, 955 at L11, 3692 at L12),
  34 MB on disk after PNG compression. Cube-edge distortion makes this longitude
  fall near a face edge where level-12 tiles are small angularly, so level 12 is
  the largest cap that keeps the package in the tens-of-megabytes range. Level 12
  is about 7 m per texel and level 11 about 14 m per texel, so level 12 slightly
  oversamples the 10 m source, and level 14 over this region would be hundreds of
  megabytes.
- Aggregate source SHA-256 (four scenes):
  `2d3f5173c44895f714d427429eaa929852169318d48c29d3d8cc1b49f674df23`.
- Aggregate runtime tile SHA-256:
  `977f14594b871e0fdb0ead79a51bd0495d0564f58e52a127fcf7adedaf737d31`.
- Role: detailed imagery for visible patches inside the region. A patch at or
  below the level range uses the most detailed produced tile at or coarser than
  its own level; a coarser patch falls back to the global overview.

## Package layout

The runtime package lives under the ignored path
`assets/large_files/terrain/earth_imagery_v1/`:

```text
global_overview.png
tiles/<face>/<level>/<tile_x>_<tile_y>.png
```

Tile identity is the existing cube-sphere `PatchKey`: `face`, `level`,
`tile_x`, `tile_y`, with `face` names `pos_x`, `neg_x`, `pos_y`, `neg_y`,
`pos_z`, `neg_z` in `CubeFace::ALL` order. A patch resolves the most detailed
tile at or above its own level inside the region, otherwise it falls back to the
global overview.

## Verification

`source_sha256` and `runtime_sha256` in the manifest are recorded when the
source data is downloaded and the runtime package is generated. The literal
value `pending` is not a valid SHA-256 and causes package verification to fail so
no unverified imagery is accepted.

For a single-file source (the global overview) the checksum is the SHA-256 of
that file. For a multi-file source or a tile set the checksum is the SHA-256 of
the sorted listing of `<file-sha256>  <relative-name>\n` lines, so it is
deterministic and order-independent.

Expected visual error: the local region is 10 m imagery displayed on terrain
whose geometry is finer than the imagery resolution at the local-detail level, so
imagery, not geometry, limits close-range ground detail. Scene 54LUR contains
thin cirrus streaks and small cloud fragments despite the low reported cloud
cover; these are visible in the imagery and are not corrected.

## Production workflow

1. Query the Earth Search STAC API for Sentinel-2 L2A scenes covering the
   region, choose one low-cloud date, and download the four `TCI.tif` COGs. Also
   download the Blue Marble August 2004 global composite. Record both downloaded
   SHA-256 values (aggregated as above) in the manifest.
2. Reproject and mosaic the UTM scenes into one WGS 84 equirectangular image with
   `scripts/imagery_reproject_utm.py`, for example:

   ```sh
   python3 scripts/imagery_reproject_utm.py --zone 54 --south \
       --bounds 139.0 -8.6 140.0 -7.4 --out /tmp/papua_equirect.png \
       54LTR.tif 54LUR.tif 54MTS.tif 54MUS.tif
   ```

   The script refuses to finish unless the mosaic fully covers the region.
3. Downsample the Blue Marble composite to `global_overview.png` and convert the
   equirectangular image to cube-sphere tiles with the offline converter:

   ```sh
   cargo run --release --features dem --bin imagery_convert -- \
       /tmp/papua_equirect.png assets/large_files/terrain/earth_imagery_v1/tiles \
       139.0 -8.6 140.0 -7.4 8 12 256
   ```
4. Verify the runtime package and fill the `runtime_sha256` fields.
5. Enable the package; a missing or invalid package falls back to the existing
   global albedo and emits a startup availability status.

No simulator path downloads imagery, and a missing package is never a terrain
or collision error.
