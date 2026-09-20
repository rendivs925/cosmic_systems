# Earth Imagery v1

Earth presentation imagery is an offline-prepared hierarchy with one
public-domain global overview and one bounded high-detail local region. It
refines visible terrain from a globe overview to satellite detail without
becoming a terrain-height, collision, or physics authority, and the simulator
never downloads imagery at runtime.

Manifest: `assets/configs/terrain/earth_imagery_v1.ron`

## Global overview

- Dataset: NASA Blue Marble: Next Generation, August 2004 base map.
- Source: <https://science.nasa.gov/earth/earth-observatory/blue-marble/>
- License: public domain (NASA). NASA material is not protected by copyright
  unless noted.
- Attribution: NASA Earth Observatory / Reto Stockli (NASA GSFC).
- Coverage: global, seamless true-colour mosaic.
- Resolution: 500 m per pixel at the equator.
- Horizontal reference: WGS 84 geographic (EPSG:4326), Earth body-fixed.
- Role: immediate fallback albedo for every visible Earth patch until a more
  detailed local tile is ready.

## Local high-detail region

- Region id: `papua_coastal_lowland`.
- Dataset: Copernicus Sentinel-2 Level-2A surface reflectance.
- Source: <https://dataspace.copernicus.eu/>
- License: Copernicus Sentinel Data is provided on a free, full and open basis
  under the Legal Notice on the use of Copernicus Sentinel Data and Service
  Information (<https://sentinels.copernicus.eu/documents/247904/690755/Sentinel_Data_Legal_Notice>).
  Redistribution and adaptation are permitted with the required attribution.
- Required attribution notice: `Contains modified Copernicus Sentinel data 2026`.
- Bands: red (B04), green (B03), blue (B02) at 10 m per pixel.
- Coverage: the Sentinel-2 L2A granule containing the presentation launch site
  at 8.0 degrees south, 139.5 degrees east. The nominal bounding box is
  `139.0..140.0` east and `8.6..7.4` south; the exact granule footprint,
  acquisition date, and processing baseline are recorded here at production.
- Horizontal reference: WGS 84 geographic (EPSG:4326), Earth body-fixed.
- Level range: `min_level = 8`, `max_level = 12`. Level 12 is the recommended
  production cap for this region size: 10 m pixels at roughly 9.5 m per texel
  while keeping the package near tens of megabytes. Producing every level to
  level 14 over a one-degree region would be roughly 800 MB of tiles, so either
  the cap stays at 12 or the region shrinks.
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
source data is downloaded and the runtime package is generated. Until then they
hold the literal value `pending`, which is not a valid SHA-256 and must cause
package verification to fail so no unverified imagery is accepted.

Expected visual error is also recorded at production: the local region is
10 m imagery displayed on terrain whose geometry is finer than the imagery
resolution at the local-detail level, so imagery, not geometry, limits the
close-range ground detail.

## Production workflow

1. Acquire the Blue Marble overview and the Sentinel-2 L2A granule, and record
   their downloaded SHA-256 values in the manifest.
2. Convert both sources to the cube-sphere tile layout with the offline imagery
   converter (added by task 2.3 of `earth-visual-streaming`).
3. Verify the runtime package and fill the `runtime_sha256` fields.
4. Enable the package; a missing or invalid package falls back to the existing
   global albedo and emits a startup availability status.

No simulator path downloads imagery, and a missing package is never a terrain
or collision error.
