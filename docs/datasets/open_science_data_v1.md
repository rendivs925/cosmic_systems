# Open Science Terrain Data v1

Status: Phase 3 priority regional packages validated; Phase 4 surface-data ingestion started with Papua land cover.

Date: 2026-10-22.

This is the inventory and provenance policy for open terrain and Earth-science
data. It complements `docs/plans/open-science-data-and-terrain-realism.md` and
the machine-readable registry at
`assets/configs/terrain/earth_datasets_v1.ron`.

## Policy

- One terrain height authority: `TerrainSource::height_m`.
- No runtime downloads. Missing or stale runtime data hard-fails `preprocess
  --check`; it never silently falls back to a different source unless that
  fallback is explicitly configured.
- Every dataset records exact source URL, version, resolution, bounds, license,
  attribution, horizontal and vertical datum, source SHA-256, runtime asset,
  runtime SHA-256, any auxiliary runtime assets (for example a geoid grid),
  converter, and status.
- `status: Ready` is the only accepted state. `Pending`, `Missing`, and
  `Retired` entries are rejected by `TerrainDatasetRegistry::validate`.
- Planned layers are documented here and are added to the registry only after a
  reviewed runtime package exists and is checksummed.
- Large runtime assets stay gitignored under `assets/large_files/`. Registry,
  manifests, docs, and checksums are committed.
- Copernicus GLO-30 is a visual and validation source only. It is never a
  bare-earth collision authority.
- HydroSHEDS is not approved unless its license is confirmed permissive for this
  use.

## Registry check

```sh
cargo run --features dem --bin preprocess -- --check --report-size
cargo run --features dem --bin preprocess -- --layer dem --check
cargo run --features dem --bin preprocess -- --region papua_coastal_lowland --check
```

`--check` re-hashes each runtime asset and auxiliary asset, and validates any
referenced per-layer manifest. For a file, the checksum is the SHA-256 of that file. For a tile
directory it is the SHA-256 of the sorted `<file-sha256>  <relative-path>\n`
listing, matching the existing imagery manifest convention. `--report-size`
prints per-dataset and total resident bytes; combined with `--check` it reports
only assets that verified.

The check validates runtime data. Reviewed source rasters are recorded by
checksum but are not required at runtime.

## Verified datasets

| Id | Layer | Coverage | Resolution | License | Converter |
| --- | --- | --- | --- | --- | --- |
| `earth_etopo2022_30s_ice_surface` | Dem | global | 30 arc-second (~927 m) | Public domain (NOAA/NCEI) | `etopo2022_convert` |
| `earth_etopo1_ice_surface` | Dem | global | 1 arc-minute (~1.85 km) | Public domain (NOAA/NCEI) | `etopo1_convert` |
| `earth_imagery_blue_marble_global` | Imagery | global | 2 km source, 8192x4096 runtime | Public domain (NASA) | `imagery_convert` |
| `earth_regional_dem_papua` | Dem | 139.0..140.0E, 8.6..7.4S | 30 m | Public domain (NASA/NASADEM) | `regional_dem_bake` |
| `earth_imagery_sentinel2_papua` | Imagery | 139.0..140.0E, 8.6..7.4S | 10 m | Copernicus Sentinel Data (attribution) | `imagery_convert` |
| `earth_landcover_worldcover_papua` | LandCover | 139.0..140.0E, 8.54..7.35S terrain-radial | 10 m source, 1 arc-second package | CC BY 4.0 (ESA) | `land_cover_bake` |

Details:

- ETOPO 2022 30 arc-second Ice Surface: the active global authority, WGS 84
  ellipsoid heights normalized from EGM2008 with the published geoid tile.
  Source SHA-256
  `3d76cea4fca0a056b6d8c4bcf7f4073465b975489849235309e65d44ed4ed404`;
  runtime SHA-256
  `2e30bb5498ce9ec8a12020cefde669208f0dba09d5a6b66861fa6e9ee11d33b6`.
  Sea level is supplied by the generated 0.5 degree EGM2008 geoid grid
  (`earth_egm2008_geoid_0p5deg_i16cm.raw`, SHA-256
  `f4ea75f9721eee1d0a3fea97a05d9dda6c166bed8300e73f8e7163d1cf59ffea`). See
  `docs/datasets/earth_etopo2022_30s_v1.md`.
- ETOPO1 Ice Surface (retained rollback): 1 arc-minute grid-registered signed i16 LE, mean sea level
  (EPSG:5773), runtime CSDEM at 2048 faces per side. Source SHA-256
  `877cbe01350b009583fd3b9c5ea4231e269484fbdaeb513b17b3a1bfdccce1ce`;
  runtime SHA-256
  `5402d634be8c4ac3897eab53c3f60ee213000752f26bb426ba9c22fdafe60c83`.
  See `docs/datasets/earth_etopo1_ice_surface_v1.md`.
- Papua regional NASADEM: 1 arc-second bare-earth over `139..140E`,
  `8.6..7.4S`, EGM96 heights corrected with the runtime EGM2008 geoid.
  Combined source SHA-256
  `1d353269f0710334541f4404eafc4c3d7531f242bcd45873bef9fd7779455625`;
  aggregate runtime SHA-256
  `74c3c99a09ebd4c74f53a2b984f8552a2dbd7501f7adbbf34743e18ebd0aa784`.
  See `docs/datasets/earth_regional_papua_v1.md`.
- Papua land cover: ESA WorldCover 2021 v200 tile S09E138, nearest-neighbour
  resampled to a 1 arc-second terrain-radial grid for presentation-only
  vegetation. Source SHA-256
  `884c7ddcd6175d4711305f6a7e706ac2fa381867b51400e883bbacc8a001abb0`;
  runtime SHA-256
  `c0552a1d314e07fcac8f39305e5c3eb25f4ae875663a2283b8f7e3ad390f99ab`.
  Required attribution: `ESA WorldCover project 2021 / Contains modified
  Copernicus Sentinel data (2021) processed by ESA`. See
  `docs/datasets/earth_landcover_papua_v1.md`.
- KSC regional NASADEM: interim 1 arc-second bare-earth over `80.85..80.45W`,
  `28.40..28.75N`, EGM96 heights corrected with the runtime EGM2008 geoid.
  Source SHA-256
  `62c3819f9ace4c785add8bbea485e3b031644087b62eaf5e951e3147c4573c09`;
  aggregate runtime SHA-256
  `88e30df9357131c1ee626542b82e251f4e3da3d98d61fc0095919582fa13ba0d`.
  See `docs/datasets/earth_regional_ksc_v1.md`.
- KSC 3DEP local refinement: 1 m USGS bare earth over `80.68..80.59W`,
  `28.567..28.620N`, normalized to 1 arc-second WGS 84 ellipsoid through a
  local NOAA VDatum NAVD88 transformation. Source SHA-256
  `66ccf1c4473b75cfa2f21cc682b3ccaf5dc391eb3b26df1555c57777c16b95d7`;
  aggregate runtime SHA-256
  `54d71e45d8f957f575b5959f08a945d77e25030e94985c96363bab2e7530b095`.
  See `docs/datasets/earth_regional_ksc_3dep_v1.md`.
- Starbase/Boca Chica regional NASADEM: 1 arc-second bare-earth over
  `97.30..97.05W`, `25.85..26.10N`, EGM96 heights corrected with the runtime
  EGM2008 geoid. Source SHA-256
  `2b0d248e76634623e77add69b577898f3ed1ff6d3aa922b4a60b2179199de3c4`;
  aggregate runtime SHA-256
  `1d3d022088d11441ac882a2060d55aa32a53d9f099385af5742f4dfeb52ae7e3`.
  See `docs/datasets/earth_regional_starbase_v1.md`.
- Vandenberg regional NASADEM: 1 arc-second bare-earth over
  `120.75..120.45W`, `34.60..34.85N`, EGM96 heights corrected with the runtime
  EGM2008 geoid. Source SHA-256
  `be9c3388c9ed196c23f6fcf473cf5ec7c978da569f79b60046590ee640e79e7e`;
  aggregate runtime SHA-256
  `ce6f44f28d53f9286f7767668e2e70ed40842ffaf02de5f623c512f5bc4ee6ff`.
  See `docs/datasets/earth_regional_vandenberg_v1.md`.
- Wallops regional NASADEM: 1 arc-second bare-earth over
  `75.70..75.35W`, `37.80..38.05N`, EGM96 heights corrected with the runtime
  EGM2008 geoid. Source SHA-256
  `9c4c90cf4c47bfb8f07e2a28279a501b90f952f5dcaa99891117d3a5a59a0cb5`;
  aggregate runtime SHA-256
  `a39fb4363042654719f57b06a00e1d97d55750f7d29035c92118fe3882581f69`.
  See `docs/datasets/earth_regional_wallops_v1.md`.
- Kourou regional NASADEM: 1 arc-second bare-earth over `52.95..52.45W`,
  `5.00..5.35N`, EGM96 heights corrected with the runtime EGM2008 geoid. Source
  SHA-256
  `f2709cb4365681f45695153775512033cc394440b1da5545f5c4083c5f7842e0`;
  aggregate runtime SHA-256
  `dfd264e18e95f0f6fe5e5690cd83a65dac6cd8e725d6e2e0986e67e3b2c63634`.
  See `docs/datasets/earth_regional_kourou_v1.md`.
- Baikonur regional NASADEM: 1 arc-second bare-earth over `63.05..63.55E`,
  `45.75..46.10N`, EGM96 heights corrected with the runtime EGM2008 geoid.
  Source SHA-256
  `a7d7e46b325b579c4e764026173774b9ac939f90bf31c77cb612f6c201ba0274`;
  aggregate runtime SHA-256
  `cb5b5dd1721e1ec3a78f122a6cbe5f81a5e2cea74f5293fa7f0ee600b64a3742`.
  See `docs/datasets/earth_regional_baikonur_v1.md`.
- Mahia regional NASADEM: 1 arc-second bare-earth over `177.70..178.00E`,
  `39.40..39.15S`, EGM96 heights corrected with the runtime EGM2008 geoid.
  Source SHA-256
  `318ba7635087d7ff585c0b53ee7f183d08d63e147697fa5284522afb0ee37772`;
  aggregate runtime SHA-256
  `0c601507d2566fae58f49cd8a2d849015c967f450baf495b6af3439113d04bb4`.
  See `docs/datasets/earth_regional_mahia_v1.md`.
- Blue Marble: August 2004 global composite with topography and bathymetry,
  WGS 84 geographic, presentation albedo only.
- Sentinel-2 L2A TCI: 2026-08-22, MGRS 54LTR/54LUR/54MTS/54MUS, reprojected
  from UTM 54S to WGS 84. Required notice: `Contains modified Copernicus
  Sentinel data 2026`. See `docs/datasets/earth_imagery_v1.md`.

## Planned and source-only datasets

These are reviewed candidates, not active registry entries. Each must pass the
same provenance and checksum gate before use.

| Dataset | Layer | Role | Resolution | License / attribution | Phase |
| --- | --- | --- | --- | --- | --- |
| SRTM GL1 (remaining regions) | Dem | regional bare earth | 30 m | NASA/USGS, public domain | 3 |
| USGS 3DEP | Dem | US regional bare earth | 1 m | USGS, public domain | 3 |
| Copernicus GLO-30 | Dem | visual and validation only | 30 m | Copernicus DEM, attribution required | 3 |
| JRC Global Surface Water | Water | lakes and surface-water occurrence | 30 m monthly | EC JRC, attribution required | 3 |
| SoilGrids | Soil | soil texture and properties | 250 m | ISRIC, CC BY 4.0 | 3 |
| Global canopy height | Canopy | canopy height and density | 30 m | NASA GEDI / partner products, verify per product | 4 |
| ERA5 reanalysis | Climate | climate and atmospheric context | ~31 km | Copernicus CDS, attribution required | later |

Regional DEM priority: KSC/Florida, Papua, Starbase/Boca Chica, Vandenberg,
Wallops, Kourou, Baikonur, Mahia.

## Datums and height normalization

- Earth heights normalize to the WGS 84 ellipsoid through EGM2008 where the
  source is a geoid or mean-sea-level model, unless a dataset documents a
  different reviewed transformation.
- ETOPO 2022 30 arc-second is the active authority, normalized from EGM2008
  orthometric heights to WGS 84 ellipsoidal heights. The ETOPO1 package
  (EPSG:5773) remains the reviewed rollback baseline.
- KSC 3DEP is NAVD88. NOAA VDatum now supplies a launch-area NAVD88-to-WGS 84
  ellipsoid transformation; the offline normalizer fits a local plane to nine
  VDatum samples (residual under 1 cm, 0.06 m VDatum uncertainty). The
  normalized 1 arc-second source is baked as `ksc_3dep_launch` and composed
  after the KSC NASADEM root.
- Presentation imagery has no vertical datum and never contributes to height.

## Ingestion commands

Registry check:

```sh
cargo run --features dem --bin preprocess -- --check
```

Phase 2 (implemented):

```sh
python3 scripts/ingest_etopo2022.py --check
cargo run --release --features dem --bin etopo2022_convert -- <normalized.raw> <output.csdem> 2048
python3 scripts/ingest_egm2008_geoid.py
cargo run --features dem --bin preprocess -- --layer dem --check
```

Phase 3 Papua (implemented):

```sh
python3 scripts/ingest_regional_dem.py --region papua_coastal_lowland
cargo run --release --features dem --bin regional_dem_bake -- --region papua_coastal_lowland
cargo run --features dem --bin preprocess -- --region papua_coastal_lowland --check
```

Phase 4 Papua land cover (implemented):

```sh
python3 scripts/ingest_landcover.py --region papua_coastal_lowland ...
cargo run --release --features native --bin land_cover_bake -- <normalized.png> \
  assets/large_files/terrain/landcover_v1/papua_coastal_lowland.clcvr ...
cargo run --features dem --bin preprocess -- --layer landcover --check
```

## Acceptance

- `preprocess --check` completes in under 2 s in release, without generating.
- Every registered runtime asset exists and matches its recorded SHA-256.
- Referenced imagery manifests parse, validate, and contain the registry
  checksums.
- Missing, stale, pending, or unverified data is a hard failure.
- No simulator path downloads terrain or science data.
