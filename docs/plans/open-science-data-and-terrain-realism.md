# Open Science Data and Terrain Realism Foundation

Status: approved plan; Phases 0-2 implemented; Phase 3 Papua, interim KSC, and
Starbase NASADEM packages, multi-root composition, coverage telemetry, and
acceptance metrics validated. KSC 3DEP/vertical datum and the remaining priority
regions are pending.

Date: 2026-10-20.

This plan extends the preprocessed-first terrain pipeline with a unified open
science data layer. It is the data foundation for the terrain realism work and
the physical cloud overhaul. Decisions below are locked.

## Locked decisions

- Earth-first; global coarse plus regional high-resolution; preprocessed-first;
  hard-fail on missing or stale data; no runtime downloads.
- Global relief upgrades to ETOPO 2022 30 arc-second (CC0), normalized to
  WGS 84 ellipsoid via EGM2008.
- Regional height authority: NASADEM/SRTM (bare-earth) plus USGS 3DEP (US high
  resolution). Copernicus GLO-30 is visual and validation only, never a
  bare-earth collision authority.
- Priority regions: KSC/Florida, Papua, Starbase/Boca Chica, Vandenberg,
  Wallops, Kourou, Baikonur, Mahia.
- Include land cover, albedo, and soil in the terrain phase; canopy with the
  scatter phase.
- Hydrology: DEM-derived rivers and lakes plus JRC Global Surface Water. Do not
  ingest HydroSHEDS unless its license is confirmed permissive.
- Moon/Mars higher-resolution data, gravity harmonics, and star catalog are a
  later phase.

## Target architecture

```text
raw open dataset
   |
   v  Python: download, verify SHA-256, reproject, vertical-datum normalize, clip
normalized raster + provenance
   |
   v  Rust: deterministic cube-sphere resample + package
baked package (elevation pyramid, surface maps, water masks, scatter, LUTs)
   |
   v  manifest + SHA-256 + bake signature
runtime: validate -> stream/read baked assets only
```

- Extend existing provenance patterns (`EarthImageryManifest`,
  `ElevationPyramidManifest`); do not duplicate validation.
- New unified registry references per-layer manifests.
- Python handles CRS and georeferencing (`rasterio`/`pyproj`); Rust handles
  cube-sphere packaging. No new GDAL Rust dependency.
- All Earth heights normalize to WGS 84 ellipsoid; record source and target
  datum in every manifest.
- Large assets remain gitignored under `assets/large_files/`; manifests, docs,
  and checksums are committed.

## Phase 0 - Terrain realism definition and metric harness

### Deliverables

- `docs/plans/terrain-realism-definition.md`: Earth parity targets, resolution
  rule, data provenance policy, acceptance gates.
- `src/domain/services/terrain_metrics.rs`: hypsometry, slope, drainage
  density, Hack exponent, concavity, Strahler, sinuosity, coastline fractal,
  PSD; area weights; per-tile computation.
- `src/bin/terrain_metrics.rs`: `--quick` (under 5 s), `--full` (under 60 s
  single-threaded, under 15 s parallel), `--assert`, `--baseline`.
- `tests/baselines/terrain-metrics-earth-etopo1.ron`.

### Acceptance

- Deterministic output; quick and full budgets met; baseline reproducible.
- Global invariants absolute; resolution-dependent metrics compared against
  ETOPO1 at matched grid resolution.
- Unit tests for metric formulas.

## Phase 1 - Dataset registry and `preprocess --check`

### Deliverables

- `src/domain/value_objects/dataset_manifest.rs`: registry entry fields (layer,
  dataset, URL or DOI, version, resolution, bounds, horizontal and vertical
  datum, license, attribution, SHA-256, runtime asset, converter, status);
  `validate()` rejects missing provenance and `pending` hashes.
- `assets/configs/terrain/earth_datasets_v1.ron`: references existing imagery
  and elevation manifests and new layers.
- `src/bin/preprocess.rs`: `--body earth --layer
  all|dem|imagery|landcover|soil|water|canopy|climate --region --quality
  --check --report-size`.
- `docs/datasets/open_science_data_v1.md`: full inventory, licenses,
  attribution, ingestion commands.
- Extend `docs/rocket_external_asset_provenance.md` with a terrain and science
  dataset section referencing the new doc.

### Acceptance

- `preprocess --check` validates all hashes and signatures in under 2 s without
  generating.
- Missing or stale data hard-fails; no runtime download path.
- Manifest and provenance tests pass.

## Phase 2 - Global relief upgrade

### Deliverables

- `scripts/ingest_etopo2022.py`: download 30 arc-second surface, verify
  SHA-256, convert EGM2008 to WGS 84 ellipsoid, emit signed i16 little-endian
  normalized raster plus provenance.
- Refactor the shared cube-sphere DEM resampler into a domain function;
  `etopo1_convert` and new `etopo2022_convert` become thin CLIs.
- New package: `earth_etopo2022_30s_ice_surface_cs2048_v3.csdem`; config
  `earth_etopo2022_30s_v1.ron`; docs
  `docs/datasets/earth_etopo2022_30s_v1.md`.
- Regenerate `earth_terrain_overview_v1.png`; update `planet_configs.rs`
  terrain authority.
- `scripts/ingest_egm2008_geoid.py` plus a generated 0.5 degree EGM2008
  geoid grid and a `GeoidModel`/`TerrainSource::sea_level_height_m` authority,
  so ellipsoidal heights keep sea level and land/ocean classification correct.
- Launch-site height delta report (old versus new) before touching physics
  baselines.

### Acceptance

- Extrema, land/ocean fraction, and mean elevation match published ETOPO 2022
  statistics.
- Launch-site altitude deltas documented; physics baselines regenerated only
  after review.
- Existing runtime budgets unchanged; old ETOPO1 package retained for rollback.

## Phase 3 - Regional DEM packages

Status: the Papua coastal lowland, interim KSC/Florida, and Starbase/Boca Chica
NASADEM packages are implemented and registered in `earth_datasets_v1.ron`.
Papua acceptance is
source RMSE 0.109 m, max error 0.866 m, tile-load p95 0.020 ms, zero
requested-coverage fallback at steady state, and 111.5 MiB peak resident
geometry. KSC acceptance is RMSE 0.147 m, max error 1.442 m, and tile-load p95
0.040 ms; Starbase is RMSE 0.166 m, max error 1.333 m, and tile-load p95
0.014 ms. Runtime composition accepts multiple roots and telemetry reports
packages and region IDs. KSC still needs a valid NAVD88-to-WGS 84 control and
the adjacent 3DEP coverage reviewed together; interactive rocket mode still
launches at Papua and needs KSC site/scenario selection. The remaining priority
regions are pending.

### Deliverables

- `scripts/ingest_regional_dem.py`: NASADEM/SRTM, USGS 3DEP, Copernicus GLO-30
  (visual only); reproject to WGS 84, normalize vertical datum, clip.
- `src/bin/regional_dem_bake.rs`: reuse the extracted elevation-pyramid
  builder; emit `.cstile` packages.
- `assets/configs/terrain/regions_v1.ron`: region IDs, bounds, priority, layer
  set.
- Region payloads under
  `assets/large_files/terrain/earth_elevation_pyramid/regions/<id>/tiles/`,
  indexed by the root `manifest.ron`; imagery links stay in
  `earth_imagery_v1.ron`. Runtime composition now accepts several package roots
  (`from_manifest_roots` / `with_dem_and_elevation_manifest_roots`); the
  default list still contains one root and later roots win at identical patch
  addresses.
- KSC absolute-datum task: resolve NAVD88 to WGS 84 using valid NOAA/NGS
  control or surveyed benchmarks. Until resolved, 3DEP supplies relative relief
  only, anchored to the global EGM2008/WGS 84 height; never apply the invalid
  offshore correction.

### Acceptance

- Source-versus-baked elevation RMSE within resampling tolerance; seam
  continuity at region edges.
- First-tile load p95 at most 50 ms; resident geometry at most 160 MiB; no
  runtime I/O blocking.
- Coverage telemetry lists installed regions and layers; missing package falls
  back explicitly.

## Phase 4 - Imagery, albedo, land cover, soil

### Deliverables

- `scripts/ingest_imagery.py`: Landsat 8/9 Collection 2 at 30 m, Sentinel-2 L2A
  at 10 m (regional); MODIS MCD43 at 500 m (global).
- `scripts/ingest_landcover.py`: ESA WorldCover at 10 m (regional), MODIS
  MCD12Q1 at 500 m (global).
- `scripts/ingest_soil.py`: SoilGrids at 250 m (regional), aggregated global
  soil.
- `src/bin/surface_bake.rs`: reuse `build_patch_surfaces` and
  `surface/layers.rs`; bake albedo RGBA8, normal/roughness, land-cover R8, soil
  R8.
- Extend region packages and global fallback textures.

### Acceptance

- Land-cover class fractions and albedo means match source within tolerance at
  reference regions.
- Texture budget respected; no per-frame dataset sampling.
- Attribution recorded per manifest and in
  `docs/datasets/open_science_data_v1.md`.

## Phase 5 - Hydrology, canopy, scatter

### Deliverables

- `scripts/ingest_water.py`: JRC Global Surface Water at 30 m; reuse
  `flow_accumulation` for DEM-derived rivers and lakes.
- `scripts/ingest_canopy.py`: Hansen Global Forest Change at 30 m;
  GEDI/ICESat-2 samples for validation only.
- `src/bin/scatter_bake.rs`: reuse `surface/scatter.rs` deterministic placement;
  bake full instances regionally; coarse outside.
- Outputs: water masks, river and lake channels, canopy height and density
  fields, scatter binary (`position`, `species_id`, `yaw`, `scale`,
  `sway_phase`, `site_level`).

### Acceptance

- Drainage density, sinuosity, water occurrence, and canopy height distribution
  match source or published targets.
- Scatter counts and viewport leaves at most 300; no runtime placement inside
  covered regions.
- Region packages remain within size budget; `--report-size` reports per-layer
  totals.

## Phase 6 - Atmosphere climatology

### Deliverables

- `scripts/ingest_era5.py`: monthly climatology of temperature, pressure, wind,
  humidity, and cloud cover (regional extracts plus global coarse).
- `src/bin/atmosphere_lut_bake.rs`: bake RGBA16F LUTs; optional
  `CloudValidationTargets` from MODIS, CloudSat, and CALIPSO statistics.
- Wire LUTs into existing atmosphere, sky, and cloud parameters; no runtime
  download.

### Acceptance

- LUT values match ERA5 at reference points within tolerance.
- Cloud fraction and coverage targets documented for validation.
- No runtime generation; startup validation remains under 2 s for `--check`.

## Phase 7 - Later: Moon/Mars, gravity, astronomy

- Higher-resolution LOLA/MOLA and lunar/Martian imagery.
- EGM2008 harmonics, GMM-3 (Mars), GRGM (Moon).
- Gaia DR3 or Hipparcos subset for starfield; TSIS-1 solar irradiance.
- Not part of the first execution.

## Data sources for the first execution

| Layer | Dataset | Resolution | License | Role |
| --- | --- | --- | --- | --- |
| Global relief | ETOPO 2022 v1 surface | 30 arc-second | CC0 | Global height authority |
| Regional relief | ETOPO 2022 15 arc-second, NASADEM/SRTM | 15 arc-second, 30 m | CC0; public domain | Regional height |
| US relief | USGS 3DEP | 1-10 m | Public domain | High-resolution US sites |
| Visual DSM | Copernicus GLO-30 Public / GLO-90 | 30/90 m | Free and open, attribution | Visual and validation only |
| Global imagery | Blue Marble NG; MODIS MCD43 | 2 km; 500 m | Public domain | Global fallback albedo |
| Regional imagery | Landsat 8/9 Collection 2; Sentinel-2 L2A | 30 m; 10 m | Public domain; Copernicus open | Surface albedo and detail |
| Land cover | MODIS MCD12Q1; ESA WorldCover | 500 m; 10 m | Public domain; CC BY 4.0 | Biomes and surface materials |
| Soil | SoilGrids; aggregated global soil | 250 m | CC BY 4.0 | Soil and erosion parameters |
| Water | JRC Global Surface Water; DEM-derived flow | 30 m | Open (Copernicus/JRC) | Rivers, lakes, water masks |
| Canopy | Hansen Global Forest Change; GEDI samples | 30 m; samples | CC BY 4.0; public domain | Scatter density and height |
| Atmosphere | ERA5/ERA5-Land monthly climatology | 0.25 degree | C3S licence, attribution | Atmosphere and cloud LUTs |
| Cloud validation | MODIS, CloudSat, CALIPSO statistics | n/a | Public domain | Validation targets |

Verified references: ETOPO 2022 <https://doi.org/10.25921/fd45-gt74>;
Copernicus DEM <https://doi.org/10.5270/ESA-c5d3d65>; ESA WorldCover
<https://doi.org/10.5281/zenodo.7254221>. Verify the exact URL and license at
ingestion; reject NC, ND, registration-gated, or unverifiable sources.

## Downstream: physical cloud overhaul

The already-agreed cloud model is unchanged and follows Phase 6:

- LCL via Magnus/Tetens; LWC 0.5-2 g/m^3; sigma_e = 3 LWC / (2 rho_w r_e);
  single-scattering albedo about 0.99-1.0.
- Beer-Lambert extinction; full-slab sun march with delta-s and 8-12 steps;
  remove `strength`, `direct_boost`, and dead `aerial_haze`.
- Atmosphere-attenuated sun, sky-derived ambient, multi-scatter, physical
  powder, distance-adaptive steps.
- Multi-layer clouds; cloud shadows on terrain; validation against Phase 6
  climatology.

## Validation and commit protocol

Per phase:

```sh
cargo fmt --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Then run each mode:

```sh
cargo run
cargo run -- craft
cargo run -- rocket
```

Instrumented budgets:

```sh
COSMIC_SYSTEMS_PERFORMANCE_METRICS=1 cargo run --release -- rocket
```

- One validated commit per phase, only after acceptance.
- Never silently regenerate physics baselines. Record launch-site deltas and
  review before updating `tests/baselines/*`.
- Do not commit generated large assets; commit manifests, docs, and checksums.
- Roll back per package: keep the previous verified package and manifest
  version.

## Risks and mitigations

| Risk | Mitigation |
| --- | --- |
| KSC NAVD88 to WGS 84 unresolved | Use relative 3DEP relief anchored to global height; obtain NGS/KSC control; never apply the invalid offshore correction |
| Copernicus is a DSM | Use only for visual and validation; height authority is NASADEM/SRTM/3DEP |
| License and attribution | Per-manifest license and attribution; reject NC, ND, registration-gated; verify at ingestion |
| Vertical datum mixing | Mandatory EGM2008/WGS 84 normalization; record source and target datum |
| Storage growth | Regional-only high resolution; gitignored packages; `--report-size` budget gate |
| Seams and feathering | Reuse `DetailLodFade`; validate height and albedo continuity at region boundaries |
| Data availability | Pin SHA-256 and version; document DOI or mirror; keep source archive |
| Precision and quantization | i16 metres for global base; f32 `.cstile` for regional; document quantization error |
| Runtime regression | Preprocessed assets only; `--check` hard-fail; no per-frame dataset sampling |

## Execution order

1. Phase 0: definition and metric harness.
2. Phase 1: registry and `preprocess --check`.
3. Phase 2: ETOPO 2022 global upgrade.
4. Phase 3: regional DEM packages (KSC datum first).
5. Phase 4: imagery, albedo, land cover, soil.
6. Phase 5: hydrology, canopy, scatter.
7. Phase 6: atmosphere climatology.
8. Physical cloud overhaul.
9. Phase 7: Moon/Mars, gravity, astronomy.
