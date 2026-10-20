# Open Science Terrain Data v1

Status: Phase 1 registry, validated.

Date: 2026-10-20.

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
  runtime SHA-256, converter, and status.
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

`--check` re-hashes each runtime asset and validates any referenced per-layer
manifest. For a file, the checksum is the SHA-256 of that file. For a tile
directory it is the SHA-256 of the sorted `<file-sha256>  <relative-path>\n`
listing, matching the existing imagery manifest convention. `--report-size`
prints per-dataset and total resident bytes; combined with `--check` it reports
only assets that verified.

The check validates runtime data. Reviewed source rasters are recorded by
checksum but are not required at runtime.

## Verified datasets

| Id | Layer | Coverage | Resolution | License | Converter |
| --- | --- | --- | --- | --- | --- |
| `earth_etopo1_ice_surface` | Dem | global | 1 arc-minute (~1.85 km) | Public domain (NOAA/NCEI) | `etopo1_convert` |
| `earth_imagery_blue_marble_global` | Imagery | global | 2 km source, 8192x4096 runtime | Public domain (NASA) | `imagery_convert` |
| `earth_imagery_sentinel2_papua` | Imagery | 139.0..140.0E, 8.6..7.4S | 10 m | Copernicus Sentinel Data (attribution) | `imagery_convert` |

Details:

- ETOPO1 Ice Surface: 1 arc-minute grid-registered signed i16 LE, mean sea level
  (EPSG:5715), runtime CSDEM at 2048 faces per side. Source SHA-256
  `877cbe01350b009583fd3b9c5ea4231e269484fbdaeb513b17b3a1bfdccce1ce`;
  runtime SHA-256
  `5402d634be8c4ac3897eab53c3f60ee213000752f26bb426ba9c22fdafe60c83`.
  See `docs/datasets/earth_etopo1_ice_surface_v1.md`.
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
| ETOPO 2022 30 arc-second | Dem | global relief replacement | ~900 m | NOAA/NCEI, public domain | 2 |
| NASADEM / SRTM | Dem | regional bare earth | 30 m | NASA/USGS, public domain | 3 |
| USGS 3DEP | Dem | US regional bare earth | 1 m | USGS, public domain | 3 |
| Copernicus GLO-30 | Dem | visual and validation only | 30 m | Copernicus DEM, attribution required | 3 |
| JRC Global Surface Water | Water | lakes and surface-water occurrence | 30 m monthly | EC JRC, attribution required | 3 |
| ESA WorldCover | LandCover | land-cover classes | 10 m | ESA, CC BY 4.0 | 3 |
| SoilGrids | Soil | soil texture and properties | 250 m | ISRIC, CC BY 4.0 | 3 |
| Global canopy height | Canopy | canopy height and density | 30 m | NASA GEDI / partner products, verify per product | 4 |
| ERA5 reanalysis | Climate | climate and atmospheric context | ~31 km | Copernicus CDS, attribution required | later |

Regional DEM priority: KSC/Florida, Papua, Starbase/Boca Chica, Vandenberg,
Wallops, Kourou, Baikonur, Mahia.

## Datums and height normalization

- Earth heights normalize to the WGS 84 ellipsoid through EGM2008 where the
  source is a geoid or mean-sea-level model, unless a dataset documents a
  different reviewed transformation.
- ETOPO1 currently uses mean sea level (EPSG:5715); its runtime package is the
  accepted baseline until the ETOPO 2022 normalization is reviewed.
- KSC 3DEP is NAVD88. A valid NAVD88 to WGS 84 control at the launch site is
  still unresolved. Until it exists, 3DEP supplies relative relief anchored to
  the global EGM2008/WGS 84 height; the offshore VDatum offset must not be
  applied as a constant correction.
- Presentation imagery has no vertical datum and never contributes to height.

## Ingestion commands

Phase 1:

```sh
cargo run --features dem --bin preprocess -- --check
```

Phase 2 (planned):

```sh
python3 scripts/ingest_etopo2022.py --check
cargo run --release --features dem --bin etopo2022_convert -- <normalized.raw> <output.csdem> 2048
cargo run --features dem --bin preprocess -- --layer dem --check
```

Phase 3 (planned):

```sh
python3 scripts/ingest_regional_dem.py --region ksc --source nasadem
cargo run --release --features dem --bin regional_dem_bake -- --region ksc
cargo run --features dem --bin preprocess -- --region ksc --check
```

## Acceptance

- `preprocess --check` completes in under 2 s in release, without generating.
- Every registered runtime asset exists and matches its recorded SHA-256.
- Referenced imagery manifests parse, validate, and contain the registry
  checksums.
- Missing, stale, pending, or unverified data is a hard failure.
- No simulator path downloads terrain or science data.
