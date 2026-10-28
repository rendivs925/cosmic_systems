# Open Science Data and Terrain Realism Foundation

Status: approved plan; Phases 0-2 implemented; Phase 3 regional packages for
Papua, interim KSC, Starbase, Vandenberg, Wallops, Kourou, Baikonur, and Mahia,
multi-root composition, coverage telemetry, acceptance metrics, launch-site
selection, and KSC 3DEP/NAVD88-to-WGS84 normalization validated.

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
- Moon/Mars higher-resolution data, gravity harmonics, and the star catalog
  were deferred to Phase 7 and are implemented there; measured solar irradiance
  remains open.

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

Status: the Papua coastal lowland, interim KSC/Florida, Starbase/Boca Chica,
Vandenberg, Wallops, Kourou, Baikonur, and Mahia NASADEM packages are
implemented and registered in `earth_datasets_v1.ron`. Papua acceptance is
source RMSE 0.163 m, max error 2.457 m, tile-load p95 0.102 ms. KSC NASADEM is
RMSE 0.194 m, max error 1.529 m, p95 0.022 ms; Starbase is RMSE 0.220 m, max
error 1.399 m, p95 0.015 ms; Vandenberg is RMSE 0.225 m, max error 2.550 m,
p95 0.015 ms; Wallops is RMSE 0.268 m, max error 1.747 m, p95 0.012 ms; Kourou
is RMSE 0.245 m, max error 2.139 m, p95 0.015 ms; Baikonur is RMSE 0.129 m, max
error 0.706 m, p95 0.014 ms; Mahia is RMSE 0.185 m, max error 1.834 m, p95
0.011 ms; KSC 3DEP is RMSE 0.025 m, max error 0.099 m, p95 0.013 ms. Regional
sources convert planetocentric terrain latitude to WGS 84 geodetic latitude
before sampling, so packages align with their reviewed geography. Runtime
composition accepts multiple roots and telemetry reports packages and region
IDs. Rocket mode selects any regional site with
`cargo run -- rocket --site <id>`; headless scenarios accept the same `--site`
option and load the pad's covering payload synchronously. Adjacent KSC 3DEP
tiles remain future work. All listed priority regions are implemented;
site-specific mission profiles remain future work.

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
  `earth_imagery_v1.ron`. Runtime composition accepts several package roots
  (`from_manifest_roots` / `with_dem_and_elevation_manifest_roots`); the
  default list composes all reviewed regional packages and later roots win at
  identical patch addresses.
- KSC absolute-datum task: NOAA VDatum now supplies the NAVD88-to-WGS 84
  ellipsoid control used by `scripts/normalize_ksc_3dep.py`. The normalized
  3DEP source is baked as `ksc_3dep_launch` and composed after KSC NASADEM.
- Regional sources are geodetic rasters while cube-sphere terrain coordinates
  are planetocentric. The baker and runtime convert terrain latitude to WGS 84
  geodetic latitude before sampling, so packages align with their reviewed
  geography.

### Acceptance

- Source-versus-baked elevation RMSE within resampling tolerance; seam
  continuity at region edges.
- First-tile load p95 at most 50 ms; resident geometry at most 160 MiB.
  Streaming never blocks terrain queries; launch setup performs one deliberate
  synchronous load of the pad's covering payload tile so the initial altitude
  is authoritative.
- Regional package probes and launch sites match their reviewed geodetic
  coordinates after planetocentric conversion.
- Coverage telemetry lists installed regions and layers; missing package falls
  back explicitly.

## Phase 4 - Imagery, albedo, land cover, soil

Status: complete. ESA WorldCover supplies a global 5 arc-minute fallback plus
eight 1 arc-second regional packages that compose deterministically; measured
imagery albedo is active where Sentinel-2/Blue Marble tiles verify; SoilGrids
supplies a global 5 arc-minute texture fallback plus eight regional 250 m
packages that modestly refine dry ground materials.

### Deliverables

- `scripts/ingest_imagery.py`: Landsat 8/9 Collection 2 at 30 m, Sentinel-2 L2A
  at 10 m (regional); MODIS MCD43 at 500 m (global).
- `scripts/ingest_landcover.py`: ESA WorldCover at 10 m (regional), MODIS
  MCD12Q1 at 500 m (global).
- `scripts/ingest_soil.py`: SoilGrids 2.0 clay/sand/silt 0-5 cm mean at 250 m
  (regional) plus a WCS-scaled global 5 arc-minute fallback; writes a
  normalized RGB8 PNG and provenance.
- `src/bin/soil_bake.rs`: versioned `.cssoil` package with per-sample integer
  clay/sand/silt percentages and provenance metadata.
- `src/domain/services/soil.rs`: bilinear `SoilPackage` / `SoilSet` sampling;
  later regional packages override the global fallback.
- `build_patch_surfaces` consumes the composed soil set at close LOD and blends
  restrained dry-ground tints; the effect is suppressed by moisture, water,
  snow, and non-bare land-cover classes.
- Extend region packages and global fallback textures.

### Acceptance

- Land-cover class fractions and albedo means match source within tolerance at
  reference regions.
- Texture budget respected; no per-frame dataset sampling.
- Attribution recorded per manifest and in
  `docs/datasets/open_science_data_v1.md`.

## Phase 5 - Hydrology, canopy, scatter

Status: in progress. JRC water occurrence and Hansen tree cover are implemented
as combined two-channel `.cssfld` packages for the eight priority regions and
feed the existing deterministic scatter and wet-ground material paths.
DEM-derived drainage and the river ribbon remain the hydrology geometry
authority. A measured canopy-height source (GEDI) and a baked scatter binary
remain open.

### Deliverables

- `scripts/ingest_water_canopy.py`: JRC Global Surface Water occurrence and
  Hansen `treecover2000` at 30 m, cropped from 10-degree source tiles and
  resampled to planetocentric terrain-radial grids.
- `src/domain/services/surface_fields.rs` and `surface_fields_bake`: versioned
  two-channel packages with bilinear sampling and later-region precedence.
- Runtime consumers: measured tree cover multiplies tree-site density, measured
  water occurrence clears vegetation and tints persistently wet close-range
  ground. The existing `surface/scatter.rs` deterministic placement is reused;
  no separate per-region instancing path is introduced.
- Outputs: water-occurrence and tree-cover masks plus measured constraints on
  the existing scatter mesh. River and lake geometry remains DEM-derived; canopy
  height remains species-profile-derived until a measured height source is
  ingested.

### Acceptance

- Drainage density, sinuosity, water occurrence, and canopy height distribution
  match source or published targets.
- Scatter counts and viewport leaves at most 300; no runtime placement inside
  covered regions.
- Region packages remain within size budget; `--report-size` reports per-layer
  totals.

## Phase 6 - Atmosphere climatology

Status: implemented as a presentation-only global monthly climatology. ERA5 was
unavailable without CDS credentials, so the layer uses NASA POWER v10
(2001-2020) MERRA-2 meteorology and CERES SYN1deg cloud amount. The runtime
package is `.csclim`; the existing standard atmosphere remains the flight-physics
authority. Measured pressure scales sky optical depth and measured cloud/wind
drive the cloud presentation. Vertical profiles and physics integration remain
future work. See `docs/datasets/earth_climate_v1.md`.

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

Status: in progress. The Moon height authority is upgraded to the 128
pixels-per-degree LOLA LDEM_128 product (236.9 m samples), baked at a
4096-sample cube face. The Mars authority is upgraded to the stitched 64
pixels-per-degree MOLA MEGR map (926 m samples), also baked at 4096. The
starfield now uses the 9,110-star Yale Bright Star Catalogue with an
IAU-aligned galactic band. Earth gravity harmonics are implemented as an opt-in
EGM2008 degree-360 force-model tier. The Moon albedo is upgraded to an
8192x4096 LROC color mosaic, and the Earth planet albedo now uses the reviewed
8192x4096 Blue Marble overview; measured solar irradiance and further planetary
imagery remain open.
See `docs/datasets/moon_lola_ldem_128_v3.md`,
`docs/datasets/mars_mola_megr_64_v2.md`, `docs/datasets/yale_bsc5_v1.md`,
`docs/datasets/earth_gravity_egm2008_v1.md`, `docs/datasets/moon_albedo_v1.md`,
and `docs/datasets/earth_albedo_v1.md`.

- Lunar albedo is upgraded to the 8192 LROC mosaic and the Earth planet albedo
  to the reviewed 8192 Blue Marble overview; higher-resolution Martian imagery
  and other body textures remain future work.
- EGM2008 Earth harmonics are implemented as the opt-in `earth-harmonics`
  tier; GMM-3 (Mars) and GRGM (Moon) remain future work.
- TSIS-1 solar irradiance; a fainter Gaia DR3 subset remains optional.
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
| KSC NAVD88 control changes | NOAA VDatum plane fit is pinned with sub-centimetre residual and 0.06 m uncertainty; re-verify control when updating the source tile |
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
