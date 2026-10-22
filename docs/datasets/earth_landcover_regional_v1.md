# Earth Land Cover - Priority Regions v1

Status: Ready, presentation-only vegetation input for all eight Phase 3 regions.

These packages supply measured broad surface classes for the priority regional
terrains. They refine vegetation species and density only. They never contribute
to height, altitude, collision, or any physics value.

## Source

- Dataset: ESA WorldCover 2021 v200.
- Resolution and projection: 10 m, WGS 84 geographic (EPSG:4326).
- Tiles: 3-degree WorldCover tiles, one per region.
- License: CC BY 4.0.
- Required attribution: `ESA WorldCover project 2021 / Contains modified
  Copernicus Sentinel data (2021) processed by ESA`.
- Base URL:
  `https://esa-worldcover.s3.eu-central-1.amazonaws.com/v200/2021/map/`.

Pinned source tiles:

| Region | Tile | Source SHA-256 |
| --- | --- | --- |
| Papua | `S09E138` | `884c7ddcd6175d4711305f6a7e706ac2fa381867b51400e883bbacc8a001abb0` |
| KSC | `N27W081` | `9a3cbe611f81f2067f37a46725546e6dad1d497a01eb3a92776cd62eb04cf5b0` |
| Starbase | `N24W099` | `bd6196e43d74707d70cba7f32f461773d02d8d5489b367f0d585397e97131d0f` |
| Vandenberg | `N33W123` | `1c8fc597b2e6299b7d90425d2092590ce0dcd15d6c354f42ecf13fbc398df121` |
| Wallops | `N36W078` | `ff7190510980a0a3775b0bf565d6f706b159b38b365f4405b6f48fd3a3d6bc04` |
| Kourou | `N03W054` | `28f40ddcc29840f80c52e4b9070921c960542484ff2597ad5d798bfe409a1793` |
| Baikonur | `N45E063` | `920936bf3f18e8c0f5e5c89706b2320a7790ad9f9c5a65c6419a1db6af66bc5d` |
| Mahia | `S42E177` | `f43e031376c8480e2b8d6805e8160129b9264fa44ada05bbf6ef88f7acb63f33` |

The source tiles are cached only in the ignored raw-asset tree. The simulator
never downloads them at runtime.

## Transformation

`scripts/ingest_landcover.py` performs the deterministic preparation:

1. Verify the pinned source SHA-256.
2. Crop to the region's terrain bounds.
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

The normalized PNGs stay under
`assets/large_files/terrain/raw/landcover/`. They are intermediate evidence, not
runtime assets.

Reproduction for one region:

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

## Runtime packages

All packages use `.clcvr` v1 with north-first, terrain-radial 1 arc-second
grids. Metadata and payload checksums are validated at startup.

| Region | Grid | Bytes | Runtime SHA-256 |
| --- | --- | --- | --- |
| Papua | 3601x4293 | 15,459,573 | `c0552a1d314e07fcac8f39305e5c3eb25f4ae875663a2283b8f7e3ad390f99ab` |
| KSC | 1441x1256 | 1,810,376 | `67411545d342a9ef917c726f67530a0c0bc3104e68c8a0e6b8cc2cb976c1ce8b` |
| Starbase | 901x897 | 808,677 | `cbad33a5fdb6ba3b7286947482eb0a7eb5feed23276f3469df2610ed53086801` |
| Vandenberg | 1081x899 | 972,299 | `a1d6fbdf69f5897eec7497266d1c9f96ffd0a7fe456a4e8530abf743057cdb2c` |
| Wallops | 1261x900 | 1,135,380 | `17cf9dd1eea55ad369ec92320834666f6dcf1b2cb8268adee244833ec073051f` |
| Kourou | 1801x1253 | 2,257,133 | `24802dc7e835a4b5fd7dedfbb7da3d389510711344e983b1709c2c66a8bdf899` |
| Baikonur | 1801x1261 | 2,271,541 | `20a2c8d9e56a816aa5a0810563cdc1fa16264d992e85a798e96ab25ef9121564` |
| Mahia | 1081x900 | 973,380 | `9b951f295f1c6d97aae7bf83497c8f882805284f22e564004f699d9630ff6daa` |

Dominant measured classes:

| Region | Dominant classes |
| --- | --- |
| Papua | tree 46.9%, water 31.4%, bare 8.3%, wetland 7.4% |
| KSC | water 61.0%, tree 22.6%, wetland 7.1%, grass 5.7% |
| Starbase | water 59.8%, wetland 14.8%, grass 13.6%, bare 6.4% |
| Vandenberg | water 43.6%, grass 25.6%, tree 16.7%, shrub 5.7% |
| Wallops | tree 36.2%, water 28.2%, crop 14.6%, wetland 12.4% |
| Kourou | tree 48.0%, water 38.9%, grass 8.2%, mangrove 2.4% |
| Baikonur | grass 76.1%, bare 22.2%, shrub 1.4%, built 0.3% |
| Mahia | water 90.1%, grass 5.7%, tree 4.0% |

The runtime loads every `.clcvr` file in `landcover_v1` in file-name order.
The `00_global_landcover_5arcmin.clcvr` fallback loads first, then these regional
packages override overlapping coverage. Outside all packages, vegetation
placement falls back to the source's climate-derived density. See
`docs/datasets/earth_landcover_global_v1.md`.

## Presentation materials

At close patch LOD, measured classes also bias the locally baked albedo and
roughness toward canonical class tints. The authoritative biome/elevation/slope
appearance remains the base, and measured cover is blended at a restrained
weight. This is a presentation refinement, not measured reflectance: the
source-derived albedo path still owns the unmeasured regions, and the packages
never feed geometry, collision, or physics.

## Validation

- The normalized rasters are compared with the pinned source tiles by
  construction: the converter only crops and nearest-neighbour remaps class
  codes.
- Package round-trip, checksum rejection, coverage, class-density ordering, and
  overlapping-package precedence are covered by `land_cover.rs` tests.
- `preprocess --check` verifies all eight runtime SHA-256 values.
- Scatter determinism with measured cover is covered by `surface` tests.
- The packages have no vertical datum and are never read by terrain height,
  collision, or flight physics.
