# Earth Soil Texture - SoilGrids v1

Status: Ready, presentation-only measured soil texture for all eight priority
regions plus a coarse global fallback.

These packages supply measured 0-5 cm clay, sand, and silt mass fractions at
250 m. They refine dry, bare, and sparsely vegetated ground materials only. They
never contribute to height, altitude, collision, erosion, or any physics value,
and they are not a second terrain height authority.

## Source

- Dataset: ISRIC SoilGrids 2.0, `clay_0-5cm_mean`, `sand_0-5cm_mean`, and
  `silt_0-5cm_mean`.
- Resolution: 250 m.
- Units: mass fraction in per-mille (g/kg). The offline converter divides by 10
  and normalizes each sample to integer percent summing to 100.
- License: CC BY 4.0.
- Required attribution: `ISRIC - World Soil Information (SoilGrids 2.0)`.
- Vertical datum: not applicable. Soil texture is a surface material property;
  the WCS response carries no height and no orthometric or ellipsoidal datum.
- Endpoint: `https://maps.isric.org/mapserv`.
- Coverage IDs:
  `clay_0-5cm_mean`, `sand_0-5cm_mean`, `silt_0-5cm_mean`.

Request template (one variable shown; sand and silt use their own map and
coverage id):

```text
https://maps.isric.org/mapserv?map=/map/clay.map&SERVICE=WCS&VERSION=2.0.1
  &REQUEST=GetCoverage&COVERAGEID=clay_0-5cm_mean&FORMAT=GEOTIFF_INT16
  &SUBSET=long(WEST,EAST)&SUBSET=lat(SOUTH,NORTH)
  &SUBSETTINGCRS=http://www.opengis.net/def/crs/EPSG/0/4326
  &OUTPUTCRS=http://www.opengis.net/def/crs/EPSG/0/4326
  &INTERPOLATION=AVERAGE
```

The global fallback adds `SCALESIZE=long(4320),lat(1728)`. The WCS scaling
extension places the output grid slightly inside the requested envelope; the
package records the actual returned bounds rather than claiming unsupported
coverage.

The WCS responses are cached only in the ignored raw-asset tree. The simulator
never downloads soil data at runtime.

## Transformation

`scripts/ingest_soil.py` performs the deterministic preparation:

1. Fetch clay, sand, and silt for the geodetic envelope that contains the
   region's planetocentric terrain bounds.
2. Read the GeoTIFF `ModelTiepoint` and `ModelPixelScale` tags directly, so no
   GDAL or rasterio dependency is needed.
3. Convert terrain-radial (planetocentric) output latitudes to WGS 84 geodetic
   latitudes before sampling, matching `regional_dem.rs` and the cube-sphere
   terrain convention.
4. Bilinearly sample the three continuous fractions.
5. Treat ocean/nodata samples as neutral loam, then normalize every sample to
   integer clay/sand/silt percentages summing exactly to 100.
6. Write a north-first RGB8 PNG (clay, sand, silt) for `soil_bake`.

The normalized PNGs stay under `assets/large_files/terrain/raw/soil/`. They are
intermediate evidence, not runtime assets. `src/bin/soil_bake.rs` validates the
per-sample sum and writes the versioned `.cssoil` package.

Reproduction for one region:

```sh
python3 scripts/ingest_soil.py --region papua_coastal_lowland
cargo run --release --features native --bin soil_bake -- \
  assets/large_files/terrain/raw/soil/papua_coastal_lowland_soil.png \
  assets/large_files/terrain/soil_v1/papua_coastal_lowland.cssoil \
  139.000000000 -8.543280680 140.000000000 -7.351005219 \
  <source-aggregate-sha256> <clay-url>;<sand-url>;<silt-url> \
  'SoilGrids 2.0 clay/sand/silt 0-5 cm mean, 250 m' 'CC BY 4.0' \
  'ISRIC - World Soil Information (SoilGrids 2.0)' 250.0
```

All regional and global inputs can be prepared with:

```sh
python3 scripts/ingest_soil.py --region all --global
```

## Runtime packages

All packages use `.cssoil` v1 with north-first, terrain-radial grids and
per-sample clay/sand/silt integer percentages. Metadata and payload checksums
are validated at startup.

| Region | Grid | Bounds (terrain-radial) | Bytes | Source SHA-256 | Runtime SHA-256 |
| --- | --- | --- | --- | --- | --- |
| Global fallback | 4321x1663 | `179.998W..179.994E`, `55.799S..82.671N` | 21,559,214 | `49e1d2efae14067d31fbd212cfe9f06bf1c4230ef8185e58f5db8f4e64d8af7a` | `07b078e51a429f27f255c81e1743164ded4e3ed3238c007d93b17c1fdb55d1be` |
| Papua | 401x478 | `139.0..140.0E`, `8.543S..7.351S` | 576,647 | `760c11b0b2144d71d0c3e56b418909031c017351c416d1a7489fcb25b0d7638f` | `3ee62c165ec3d6dbf519ca9c853dd8f19b7609b370b14c6b6ddbba46e127ab71` |
| KSC | 161x140 | `80.85..80.45W`, `28.239..28.588N` | 69,272 | `b33fec493f644054bbfeb28f12d91303fc800cd650ba87d2444b1c01d262c68e` | `bcca6e3e8019b09ab71ac6c27a83bcc8c00860db4d4d4f1ad6753f482e40847a` |
| Starbase | 101x101 | `97.30..97.05W`, `25.699..25.948N` | 32,255 | `77bfc3ca565e20711f7a892ef2ba247efb297738e4e4be594777ee9966feef5b` | `540e98a20982e723da34257ab1aa3fbaf326960f662f82aee2b94472ba4a18ca` |
| Vandenberg | 121x101 | `120.75..120.45W`, `34.420..34.670N` | 38,282 | `b4af1335ed3e4b15bb333a5b0cfb2b58f8f447fa3ee9cc81baa20fe903eac8cf` | `08ea7f32d0c5c16549ebbfbd78be0497806e66dac309be9e6c28d530d61d244f` |
| Wallops | 141x101 | `75.70..75.35W`, `37.614..37.863N` | 44,339 | `69cbd5a2864489cd5e4468217afabdbc12b2644f080915a7934aeff2986fbdc6` | `744250087d4b623e1e79fd4461a58c8bceff3d4d9d335a8f5076bec8f4d50357` |
| Kourou | 201x140 | `52.95..52.45W`, `4.967..5.314N` | 86,033 | `352b992064b5c006cbf717eb8da64bc153e1360e16589858d7b8294c3222ff4d` | `c46ed4b9019d0ecddb1b03450864b4d137f1466c0fe79b078cd018eadc6d8887` |
| Baikonur | 201x141 | `63.05..63.55E`, `45.558..45.908N` | 86,630 | `2c98b8d3506bfa8560500e08b7c7ca5de34ac1bf1df0cfbb1c195d2fd11557bb` | `3e753ccf7ad3ae4e3a393b749fdedde59ae3ba64f2ca89efcce6ed59c9fed57a` |
| Mahia | 121x101 | `177.70..178.00E`, `39.211..38.962S` | 38,279 | `5fb177bc00c087c12643d259b363ed70ea6f4fbc7312d45a65bde04c314009b9` | `12cabc16aad0e34aaf1b16b217bd559457a4408b37030195fd2d56e667b564b1` |

The global package uses the `00_` prefix so it loads first. The runtime loads
every `.cssoil` file in `soil_v1` in file-name order; regional packages override
overlapping coverage. Outside all packages, the source-derived material path is
used unchanged.

Source SHA-256 values are aggregates over the three WCS responses:

```text
sha256(clay.tif)  clay  <clay request URL>
sha256(sand.tif)  sand  <sand request URL>
sha256(silt.tif)  silt  <silt request URL>
```

## Presentation consumer

At close patch LOD, `build_patch_surfaces` samples the composed soil set at the
same terrain-radial coordinate as the authoritative appearance. Measured
texture is blended toward restrained canonical tints:

- sand-rich soil is warmer and paler,
- clay-rich soil is darker and redder,
- silt-rich soil sits between them.

The blend weight is at most 0.28 and is further reduced by moisture and by the
measured land-cover class: bare/sparse ground receives the strongest effect,
cropland/grass/shrub less, forest and built-up very little, and water or snow
none. Below sea level the weight is zero, so an absent or partial land-cover
package cannot tint open water. The source-derived biome, elevation, slope, and
river shading remains the base material. Soil never changes geometry, collision,
altitude, or flight physics.

## Validation

- Package round-trip, checksum rejection, coordinate coverage, bilinear
  sampling, per-sample percentage invariant, and overlapping-package precedence
  are covered by `soil.rs` tests.
- `preprocess --layer soil --check` verifies all nine runtime SHA-256 values.
- The close-range material tint and its restrained weight are covered by
  `surface_maps` tests.
- Source samples are converted from WGS 84 geodetic to planetocentric
  terrain-radial latitude before baking; the packages align with their reviewed
  geography.
- The packages have no vertical datum and are never read by terrain height,
  collision, or flight physics.
