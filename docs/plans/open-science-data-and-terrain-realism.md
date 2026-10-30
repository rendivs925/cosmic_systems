# Open Science Data and Terrain Realism Foundation

Status: Phases 0-7 implemented for the locked data layers. Regional DEM packages
for Papua, interim KSC, Starbase, Vandenberg, Wallops, Kourou, Baikonur, and
Mahia, multi-root composition, coverage telemetry, acceptance metrics,
launch-site selection, and KSC 3DEP/NAVD88-to-WGS84 normalization validated.
Measured water, tree cover, and canopy height feed presentation-only surface
fields; TSIS-1 solar irradiance is explicitly deferred.

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
  were deferred to Phase 7 and are implemented there. Measured solar irradiance
  is explicitly deferred; see `docs/datasets/solar_irradiance_tsis1_deferred.md`.

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

Status: complete for the measured layers. JRC water occurrence, Hansen tree
cover, and Meta/WRI average canopy height are combined into three-channel
`.cssfld` packages for the eight priority regions and feed the existing
deterministic scatter and wet-ground material paths. KSC, Starbase, Vandenberg, Wallops, Mahia, and
Baikonur carry measured Meta/WRI canopy height. Papua has no published
downsampled canopy tile, and the Kourou source tile was not provisioned in the
current environment, so those two fall back to zero canopy and the reviewed
species profile. Re-running the offline ingest upgrades Kourou without a format
change. DEM-derived drainage and the river ribbon remain
the hydrology geometry authority. A baked scatter binary remains optional future
work.

### Deliverables

- `scripts/ingest_water_canopy.py`: JRC Global Surface Water occurrence and
  Hansen `treecover2000` at 30 m, cropped from 10-degree source tiles and
  resampled to planetocentric terrain-radial grids.
- `src/domain/services/surface_fields.rs` and `surface_fields_bake`: versioned
  three-channel packages with bilinear sampling and later-region precedence.
- Runtime consumers: measured tree cover multiplies tree-site density, measured
  water occurrence clears vegetation and tints persistently wet close-range
  ground, and measured mean canopy height scales the reviewed tree profile
  within a bounded factor. The existing `surface/scatter.rs` deterministic
  placement is reused; no separate per-region instancing path is introduced.
- Outputs: water-occurrence, tree-cover, and canopy-height grids plus measured
  constraints on the existing scatter mesh. River and lake geometry remains
  DEM-derived; a zero canopy-height sample keeps the reviewed species profile.

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

Status: complete for the named data upgrades; measured solar irradiance is
explicitly deferred. The Moon height authority is upgraded to the 128
pixels-per-degree LOLA LDEM_128 product (236.9 m samples), baked at a
4096-sample cube face. The Mars authority is upgraded to the stitched 64
pixels-per-degree MOLA MEGR map (926 m samples), also baked at 4096. The
starfield now uses the 9,110-star Yale Bright Star Catalogue with an
IAU-aligned galactic band. Earth gravity harmonics are implemented as an opt-in
EGM2008 degree-360 force-model tier. The Moon albedo is upgraded to an
8192x4096 LROC color mosaic, and the Earth planet albedo now uses the reviewed
8192x4096 Blue Marble overview. GMM-3 (Mars, degree 120) and
GRGM660PRIM (Moon, degree 660) gravity packages are baked and validated as data
readiness for future planetary flight. Measured solar irradiance is explicitly
deferred with quantified rationale; further planetary imagery remains open.
See `docs/datasets/moon_lola_ldem_128_v3.md`,
`docs/datasets/mars_mola_megr_64_v2.md`, `docs/datasets/yale_bsc5_v1.md`,
`docs/datasets/earth_gravity_egm2008_v1.md`, `docs/datasets/mars_gmm3_v1.md`,
`docs/datasets/moon_grgm660_v1.md`, `docs/datasets/moon_albedo_v1.md`,
`docs/datasets/earth_albedo_v1.md`, and
`docs/datasets/solar_irradiance_tsis1_deferred.md`.

- Lunar albedo is upgraded to the 8192 LROC mosaic and the Earth planet albedo
  to the reviewed 8192 Blue Marble overview; higher-resolution Martian imagery
  and other body textures remain future work.
- EGM2008 Earth harmonics are implemented as the opt-in `earth-harmonics`
  tier; GMM-3 (Mars) and GRGM660PRIM (Moon) are baked as validated data
  packages. Mars/Moon runtime consumers and their non-IAU frame conversions
  remain future work.
- TSIS-1 solar irradiance is deferred: the measured record starts after the
  default 2000 epoch and varies by only 0.25% peak to peak, below one 8-bit
  render step. See `docs/datasets/solar_irradiance_tsis1_deferred.md`. A fainter
  Gaia DR3 subset remains optional.
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
- Atmosphere-attenuated sun, sky-derived ambient, multi-scatter,
  distance-adaptive steps.
- Multi-layer clouds; cloud shadows on terrain; validation against Phase 6
  climatology.

Status: the current shader implements Beer-Lambert extinction, multi-scatter
octaves, a physically scaled diffuse multiple-scattering term, and measured
coverage/type/wind from the climatology layer. The sun march traverses the full
cloud slab with 8 steps instead of a fixed 12% slice.

The single-scatter step integral is the physical form: the incoming direct and
ambient radiance is multiplied by the droplet single-scattering albedo (0.99)
and the analytic optical-depth integral `1 - exp(-sigma_e * ds)`. The
`strength`, `direct_boost`, and unused `aerial_haze` uniforms are gone. The old
code multiplied by density and divided by `sigma_e`, which made the radiance
depend on `1 / absorption`; an 8x presentation scale (times a 1.6x
`direct_boost`) had been compensating for it, so removing the fudge factors also
removes that 12.8x over-brightness.

The sun march now includes the physical step length in its optical depth. It
previously summed dimensionless density and multiplied by `absorption`, giving
optical depths of about 0.02 instead of about 9, so sunlight was effectively
unattenuated inside the deck. The separate `powder_strength` term then
multiplied that near-zero optical depth by 2.5 and collapsed the direct term to
about 4% of its value, and because it was largest where the sun path is clear it
darkened exactly the sunlit cloud tops. The powder term is removed: optically
thin cloud is already dimmed by the single-scatter integral, and the triple-lobe
phase function keeps the silver lining.

The fixed `ambient_fraction` and `sky_color` uniforms are also gone. Diffuse
multiple scattering is now `albedo * E * cos(sun zenith) / PI`, the near-
Lambertian reflectance of a nearly conservative water-cloud layer, tinted white
at the sunlit top and by the ground bounce at the base. Because the step
integral weights it by the local scattering fraction, thin wisps reflect
proportionally less, and the deck now darkens physically at night and across the
terminator instead of glowing from the fixed 12% term.

Native Vulkan capture (NVIDIA RTX 5070, Xvfb `:99`, rocket mode, `xdotool`
launch, `import`), same flight, 500x300 deck sample:

| build | deck from above, t+105 s | in-deck, t+20 s |
| --- | --- | --- |
| unattenuated sun + powder | sRGB 159, sd 1.4 | sRGB 137, sd 7.3 |
| step-length optical depth | sRGB 149, sd 1.9 | sRGB 142, sd 7.2 |
| + physical diffuse term | sRGB 197, sd 0.8 | sRGB 193, sd 14.8 |

The surface camera at t+132 s now shows a white sunlit deck over blue ocean with
a visible layer edge. Earlier captures either clipped the deck to a flat white
sheet (pre-`b52ed82`) or left it ambient-only gray. The deck still lacks
large-scale billows: the weather map varies at synoptic scale and the density
tiling is 4.5 km, so from 50 km the integrated deck is smooth. That, multi-layer
decks, terrain cloud shadows, atmosphere-attenuated sun, and distance-adaptive
steps remain future work.

The 2D weather map was also sampled only once across the sphere, so its fBm
features spanned thousands of kilometres and any flight view saw a single
coverage value. It is now sampled at an integer tiling factor of 48 (largest
features about 300 km), with the wind drift scaled by the same factor so the
advection speed is unchanged. Tiling only stays seam-free if the map itself
wraps: `fbm2` used a cell period four times its frequency, which left a hard
discontinuity at the antimeridian that tiling would have repeated 48 times. The
generator now uses the per-octave frequency as its cell period, and a unit test
checks both seams at several seeds.

Effect on the same flight: the deck-from-above sample gains real cellular
structure, with sRGB standard deviation rising from 0.8 to 3.7 while the mean
moves from 197 to 177 as coverage holes open. At 3x zoom the deck shows cloud
cells rather than a featureless gradient.

The two noise volumes and the weather map now also carry full box-filtered mip
chains, built with seam-wrapped 2x2x2 (3D) and 2x2 (2D) filtering. The samplers
already asked for linear mip filtering but the textures had a single level, so a
minified deck aliased and shimmered as the camera moved. The chains preserve the
field mean to within one 8-bit step (tested) and add 14% (3D) and 33% (2D)
bytes. On the deck-from-above sample the standard deviation falls from 3.03 to
2.80 with the mean unchanged (180.1 to 180.5), which is the expected smoothing
of sub-pixel detail.

Earth now draws two decks from the same shader: the boundary-layer/mid-
troposphere water deck (1.5-6 km) and a thin cirrus veil (8.0-9.6 km) with a
stratus-like profile, larger features, upper-tropospheric wind, and lower
extinction per unit density. The veil exists only where a measured climatology
drives it (Earth). Both domes are camera-centred, so view distance cannot order
them: the update system sets the transparent sort bias each frame from the
camera's planet-centric radius, drawing the farther deck first (cirrus from the
ground, water deck from orbit). The veil uses 40 view steps and 3 sun steps,
about 15% of the water deck's sample budget.

Coverage is a share of the sky, not a multiplier on the density. The measured
climatology gives a regional cloud fraction, and the weather map's red channel
is now baked as a **rank** (histogram-equalised at generation, still periodic at
both seams), so the fraction is used as a threshold on that rank: below
`1 - fraction` the air is genuinely clear and the band above ramps from a wispy
edge to the dense core of a system. The previous form (`coverage * weather *
1.4`) could not do this, because an fBm concentrates almost every texel near its
mean and the product saturated over most of the map. The cirrus also used the
water deck's extinction, which integrated to `tau ~ 7.7` across its 1.6 km and
rendered as an opaque sheet; it now uses `0.0012` (`tau ~ 1`) and a sparser veil
(0.55x the surface fraction, clamped to 0.05-0.45).

Measured from the chase camera 42 s after liftoff (about 40 km, looking down at
the deck), the fraction of the view that is bright low-saturation cloud fell
from 0.99 to 0.46, with the mean brightness down from 0.66 to 0.42. The frame
now shows a broken cumulus field over open water instead of a white carpet, and
turning the cirrus off under the new parameters changes the fraction by 0.005,
so the veil no longer dominates. At 95 s (about 200 km) the view shows a large
cloud system with clear ocean between it and the next one.

The direct term is also gated by the solar terminator. The sun march previously
reported a clear path for a Sun below the cloud's local horizon, so the night
side received the full solar irradiance: at a night epoch the prelaunch sky band
measured sRGB 96 against a black ground. With the gate it measures 0.8, and the
daylight term is unchanged because the gate is exactly 1 for any Sun more than
about one degree above the horizon.

Direct sunlight now also carries the atmosphere's beam transmittance.
`AtmosphericOptics::sun_transmittance` integrates the shared extinction profile
over the whole column above an altitude (64-step midpoint integral, so Rayleigh,
Mie, and the ozone band keep their own scale heights) and applies the
plane-parallel airmass `1 / cos(zenith)`, clamped so a Sun on the horizon stays
finite. The directional light keeps the top-of-atmosphere illuminance scaled by
the brightest channel and takes the per-channel ratio as its colour; the cloud
decks evaluate the same function at their own altitude, so the cirrus stays
brighter than the water deck at a low Sun. The sky dome still receives the
top-of-atmosphere irradiance, which is the correct source for its scattering
integral. At the pad with the Sun 58 degrees above the horizon the frame darkens
2.3% in red, 5.5% in green, and 10.6% in blue, which is the expected Rayleigh
removal of blue from the direct beam.

Terrain cloud shadows are now cast from the same density field. The terrain
material binds the deck's base and detail volumes and weather map, and its
fragment stage marches the ground-to-Sun ray through the deck slab at six
samples. The optical depth uses the deck's own extinction coefficient, so a
shadow is as opaque as the cloud above it, and a `select` guard keeps the pass
inactive until the cloud system has reported a field (bottom radius zero). The
shared field lives in `assets/shaders/cloud_density.wgsl`, which both the cloud
shader and the terrain shader import so the two can never drift apart.

Two details are worth recording. First, the slab geometry: a ground fragment is
inside both the base and top spheres, so the slab segment runs from the *far*
root of the base sphere to the *far* root of the top sphere, a few kilometres
apart. Using the near root of the top sphere inverts the segment and silently
returns full sunlight. Second, the diffuse floor: a fully overcast deck is
optically thick enough to remove the direct beam entirely, but a real overcast
day still delivers roughly a fifth of the clear-sky irradiance as diffuse
skylight. The renderer does not inject the deck's hemisphere radiance into the
scene ambient, so `CLOUD_DIFFUSE_TRANSMITTANCE = 0.2` keeps that fraction
(`mix(0.2, 1, T)`) and is documented as a presentation approximation. At the
pad under an overcast deck this leaves the ground at 53% of its sunlit
brightness instead of 27%, and the clear-sky case is unchanged because the mix
is exactly one at `T = 1`.

The ocean carries the same shadow. The water material binds the same three cloud
volumes and a `CloudShadow` uniform (field, Sun, inertial-to-body rotation), and
`water.wgsl` multiplies the terrain-occlusion landscape term by the shared
`cloud_shadow_visibility` march. With coverage forced to 0.95 for the test, the
sea measured 75 against a sunlit 75 with the shadow disabled and 40 with it
enabled, so land and water now darken under the same deck instead of the sea
staying sunlit. The march, the diffuse floor, and the field definition live in
`cloud_density.wgsl`, so the three consumers cannot drift apart.

The sea now reflects what is above it. A dielectric mirror reflects a growing
share of its surroundings as the view grazes the surface, so the water evaluates
two radiances along the mirrored view direction and blends them by the deck's
own opacity on that ray:

- the clear sky, from `sky_radiance.wgsl` -- the single-scatter integral the sky
  dome draws with, extracted into one module and imported by both shaders so the
  reflected sky cannot drift away from the sky above it;
- the deck, from `cloud_density.wgsl` -- the near-Lambertian radiance a thick,
  conservatively scattering layer returns downward (`albedo * E * cos(sun
  zenith) / PI`, gated on the Sun's local horizon and extinguished by the same
  beam transmittance the drawn deck uses).

The deck's share comes from the same density march that casts the ground
shadow, so a reflected cloud is exactly the cloud overhead. The term replaces
the uniform ambient specular the PBR path already reflected by adding only the
difference, which keeps a clear sky's horizon gradient and gives an overcast sea
the grey of the deck instead of a blue that is not there.

Measured at the Mahia coast (one binary, shader-only toggle, water pixels
selected by an opaque marker pass):

| condition | water, reflection off | water, reflection on | adjacent sky |
| --- | --- | --- | --- |
| clear | 62, 84, 90 | 143, 145, 139 | 141, 143, 146 |
| forced 0.95 deck | 50, 69, 80 | 142, 141, 135 | -- |

At grazing the sea now matches the sky it reflects to within a few sRGB units
instead of falling back to a dark teal. Under the forced deck the mirror ray's
transmittance (rendered as a false-colour overlay of `deck_transmittance`) is
down to a few percent over the whole bay, so the reflection there is the deck's
grey rather than a hidden blue sky.

One presentation limit remains: the sky model has no clouds, so a reflection of
a *partly* cloudy sky is a blend of clear airlight and the deck's mean radiance
rather than a picture of individual clouds. The mirror direction is also a
single sample of the wave-perturbed normal, so an unresolved sea reflects a
smooth field rather than a blurred one.

One diagnostic pitfall: an imported shader module that declares
`#define_import_path` is registered under that custom name only, so an
asset-path `#import "shaders/..."` silently fails. When it does, the material
pipeline is never created and nothing is logged at the default filter -- the
terrain simply disappears and the fallback globe shows through. The module is
therefore imported by asset path only, guarded by a regression test.

The ground now also carries mid-scale albedo patchiness at 6-17 m
(`MESO_FREQUENCY`/`MESO_STRENGTH` in the terrain shader). That is the band the
streamed imagery cannot resolve and the sub-metre detail textures average away
beyond a few metres, so close ground previously read as a flat wash of colour.
The field is evaluated from the precise render-relative position plus a
fractional phase anchor, the same pattern the detail and layer textures use, so
it stays pinned to the surface across patches and render-origin recentres. A
unit test mirrors the shader hash and proves the field varies across a 200 m
view at both octaves. A GPU A/B was inconclusive in this environment because the
prelaunch framing is not stable across rebuilds; the amplitudes are deliberately
modest (+-22% at 17 m and +-9% at 6 m, faded out beyond 3-10 km) and
presentation-only. Measured on the same land framing, the field raises the
foreground albedo standard deviation from 2.39 to 2.87 sRGB (+20%) with no
change in mean brightness, and with the strength forced to 1.0 it reaches 6.70,
which confirms the wiring rather than a shader that silently samples a constant.

Distance-adaptive cloud steps remain future work.

Reproducible capture (no window manager): `Xvfb :99 -screen 0 1600x900x24`,
`DISPLAY=:99 i3 -c /dev/null`, run the rocket binary, then
`xdotool windowfocus <id> key space` and `import -window root frame.png`.

The prelaunch chase view follows the commanded launch heading. The body axis is
within a couple of degrees of vertical while the vehicle stands on the pad, and
its horizontal residual is the difference between the radial up and the pad
normal, so its aspect depends on whichever pad elevation sample was resident at
launch. Using that residual made the upright side view alternate by 180 degrees
between runs. The chase camera now takes the heading when the body axis is
within 25 degrees of vertical; during ascent the heading and the actual pitch
direction agree, so only the slope-dependent choice is removed.

Framing is repeatable *within* a build: consecutive runs then produce the same
view. It is still not repeatable across rebuilds, because the pad elevation
sample that a run sees depends on startup timing. Ground A/B captures therefore
have to be taken as a pair from one build (a shader-only change keeps the binary
identical), and any comparison that needs land should confirm the crop is ground
(`g > b`) before trusting it -- several early measurements in this work were
silently sampling the seaward fallback globe.

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
