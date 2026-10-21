# Terrain Realism Definition

Status: Phase 0 definition, implemented.

Date: 2026-10-20.

This document defines what "terrain realism" means for the simulator and how it
is measured. It accompanies `src/domain/services/terrain_metrics.rs` and the
`terrain_metrics` binary. It is the acceptance reference for the open-science
data plan (`docs/plans/open-science-data-and-terrain-realism.md`).

## 1. Purpose and non-goals

Realism is defined as **measurable parity with Earth**, not visual plausibility.
A terrain change is realistic when the authoritative terrain field moves toward
the targets in section 5 without breaking the invariants in section 6.

The harness is measurement-only:

- it reads the authoritative `TerrainSource` and never writes simulation state;
- it never feeds guidance, collision, rendering, or physics;
- a metric gate must never be satisfied by manipulating visuals or by special
  casing the sampler.

## 2. Authority and data policy

- Runtime heights have one authority: `TerrainSource::height_m`.
- Global parity is measured on the **base measured DEM only**, without erosion,
  procedural detail, river carving, or pad flattening. This separates data
  quality from procedural contributions.
- Composed terrain (erosion + detail) gets regional acceptance once the regional
  data phase lands. It is not compared against global Earth statistics.
- Source hierarchy:
  - current baseline: ETOPO1 Ice Surface 1 arc-minute cube-sphere package;
  - Phase 2: ETOPO 2022 30 arc-second, normalized to the WGS 84 ellipsoid via
    EGM2008;
  - Phase 3: NASADEM/SRTM bare-earth and USGS 3DEP for priority regions.
- Copernicus GLO-30 is visual and validation input only; it is never a
  bare-earth collision authority.
- No runtime downloads. Missing or stale packages hard-fail.

## 3. Measurement grid and resolution rule

| Pass | Grid | Cell size | Channel threshold | Budget |
| --- | --- | --- | --- | --- |
| `--quick` | 720 x 360 | 0.5 deg | 32 cells | < 5 s |
| `--full` | 1440 x 720 | 0.25 deg | 32 cells | < 60 s single, < 15 s parallel |

Samples sit at cell centers, rows run north to south, and row areas use the
exact spherical band `R^2 * dlon * |sin(lat_north) - sin(lat_south)|`.

**Resolution rule.** Area fractions, land/ocean split, and datum-level mean
elevation are global invariants and are compared absolutely. Everything that
depends on cell size or stream threshold (slope percentiles, drainage density,
Hack, concavity, sinuosity, coastline length and fractal dimension, PSD) is
compared only against a baseline measured with the same source, grid, and
threshold.

Coarse grids describe the *global relief field*, not hillslopes. The 5-10 deg
slope target and the 1-5 km/km^2 drainage target apply to high-resolution
regional DEMs, not to a 0.25 deg global grid.

## 4. Metrics and formulas

### Hypsometry

Area-weighted elevation distribution: minimum, maximum, land fraction, ocean
fraction, land mean, land median, and land p95. Percentiles are area-weighted,
not cell-weighted.

### Slope

Central differences on the sample grid:

```
dz/dx = (h_east - h_west) / (2 R cos(lat) dlon)
dz/dy = (h_south - h_north) / (2 R dlat)
slope = atan(hypot(dz/dx, dz/dy))
```

Reported area-weighted median and p95, the maximum, and the land fraction above
35 degrees (the conventional talus limit).

### Drainage network

Flow accumulation and downstream routing come from the authoritative erosion
functions (`flow_accumulation_geographic`, `d8_downstream_geographic`), so the
measurement network and the erosion network cannot disagree on tie-breaking.

```
drainage density = channel length / land area
```

A cell is a channel when flow accumulation reaches `channel_threshold_cells`.
Channel length is the sum of D8 edge distances for channel cells.

### Hack exponent

`L ~ A^h`, fit by least squares on `log(L)` against `log(A)` over stream cells.
The proxy for basin main-stem length is the longest upstream flow-path length
within the stream network. It is resolution dependent and is compared only at
matched resolution.

### Concavity

`S ~ A^-theta`, fit by least squares on `log(S)` against `log(A)` using the
local D8 channel slope. Reported as `theta`.

### Strahler order

Computed on the channel network with a descending-height topological pass.
Two equal-order tributaries raise the order by one. Reported as the maximum
order on the grid.

### Sinuosity

For channel cells with at least twice the threshold accumulation, the ratio of
D8 path length from the farthest upstream source to the straight great-circle
distance between that source and the cell. Reported as the median.

### Coastline

Length is the sum of land/ocean cell edges, counted once from the land side.
Pole rows are not treated as ocean. Fractal dimension is the box-counting slope
of `log(box count)` against `log(1 / box size)` for power-of-two box sizes.

### Roughness

The elevation variogram is estimated on land pairs at increasing lags:

```
gamma(d) = 0.5 * mean((h(x + d) - h(x))^2)
```

The slope of `log(gamma)` against `log(d)` gives `2H`; the 2D power-spectrum
exponent is `beta = 2 + 2H = 2 + slope`. The ramp test pins the formula.

## 5. Earth parity targets

| Metric | Target | Class |
| --- | --- | --- |
| Land / ocean fraction | 0.292 / 0.708 | absolute, +/-0.005 |
| Land mean elevation | about 840 m | absolute on source raster, +/-5% |
| Elevation extrema | -10,935 m / +8,849 m | source raster at native resolution |
| Slope median / p95 | 5-10 deg / 25-35 deg | high-resolution regional |
| Slopes above talus | <= 35 deg | high-resolution regional |
| Drainage density | 1-5 km/km^2 | high-resolution regional |
| Hack exponent | 0.5-0.7 | matched-resolution comparison |
| Concavity theta | 0.4-0.5 | matched-resolution comparison |
| Strahler order | 3-5 for large basins | matched-resolution comparison |
| Sinuosity | 1.1-2.5 | matched-resolution comparison |
| Coastline fractal dimension | 1.2-1.3 | multi-scale comparison |
| PSD beta | 1.8-2.8 | matched-resolution comparison |

Extrema are validated on the normalized source raster. A cube-sphere package
resampled at 2048 faces per side (about 5 km at the equator) smooths isolated
summits and trenches; the runtime package is checked for non-regression against
the source's sampled extrema at package resolution, not for preserving
single-cell peaks.

## 6. Acceptance gates

Phase 0 gates:

- `--quick` completes in under 5 s and `--full` in under 60 s single-threaded.
- The full report reproduces the committed baseline exactly.
- Unit tests cover percentile weighting, analytic slope, variogram slope,
  coastline length, D8 channel ordering, report round-trip, and config
  validation.

Later gates:

- Phase 2 must record source-raster statistics and the cube-package deltas for
  extrema, land fraction, and launch-site heights before any physics baseline is
  regenerated.
- Phase 3 must add regional metrics at native resolution for slope, drainage,
  Hack, concavity, sinuosity, and coastline fractal dimension.
- CI recommendation: hard-fail `--full --assert` plus `--quick`; run the GPU and
  regional suites advisorily.

No gate may be met by changing the measurement grid, threshold, or baseline
without an explicit, reviewed rationale.

## 7. Baseline policy

- Baseline: `tests/baselines/terrain-metrics-earth-etopo2022.ron` (active).
  `tests/baselines/terrain-metrics-earth-etopo1.ron` is retained for rollback.
- The baseline records the current runtime authority: ETOPO 2022 30 arc-second
  Ice Surface cube-sphere package at 2048 faces per side, 0.25 deg measurement
  grid, 32-cell channel threshold.
- Update the baseline only when the terrain authority changes intentionally.
  Every update must include a before/after table and the reason.
- Never regenerate a baseline to hide a regression.

## 8. Current reading and limitations

Measured on the ETOPO 2022 package (full pass, 2026-10-21):

| Metric | Measured | Interpretation |
| --- | --- | --- |
| Land fraction | 0.29087 | Matches the 0.292 target. |
| Ocean fraction | 0.70913 | Matches the 0.708 target. |
| Land mean | 789.6 m | Within the documented source mean. |
| Min / max | -10,303.7 / 6,767.6 m | Smoothed by the 2048-face resample. |
| Slope p50 / p95 | 0.15 / 1.29 deg | Coarse global relief; not the hillslope target. |
| Drainage density | 0.0022 km/km^2 | Only large channels exceed 32 cells at 0.25 deg. |
| Hack exponent | 0.747 | Within the 0.7 upper bound; coarse proxy. |
| Concavity theta | 0.139 | Coarse D8 network biases concavity low. |
| Max Strahler order | 4 | Within the 3-5 target. |
| Median sinuosity | 1.146 | Above the 1.1 lower bound. |
| Coastline length | 593,818 km | Coarse-grid edge length. |
| Coastline fractal dimension | 1.186 | Close to the 1.2-1.3 target. |
| PSD beta | 2.812 | At the upper bound. |

The ETOPO1 reading (land 0.29105, land mean 793.2 m) remains in
`terrain-metrics-earth-etopo1.ron` for rollback comparison.

Known limitations:

- The 2048-face cube package is about 5 km at the equator and cannot preserve
  single-summit extrema. On ETOPO 2022 this retains +6,767.6 m of the raster's
  +8,325 m maximum and -10,303.7 m of its -10,741 m minimum, a documented
  -1,557.4 m / +437.3 m extrema loss. The 50 MiB budget was kept deliberately;
  summit-scale claims require a higher-face or regional package.
- Coarse-grid drainage, Hack, concavity, and sinuosity are proxies, not
  hillslope measurements. The regional phase is authoritative for those targets.
- The KSC absolute vertical datum is unresolved. 3DEP supplies relative relief
  only until a valid NAVD88 to WGS 84 control is available; no offshore
  correction is applied.

## 9. Provenance

Current baseline source:

- dataset: ETOPO 2022 v1 30 arc-second Ice Surface, WGS 84 ellipsoid heights;
- config: `assets/configs/terrain/earth_etopo2022_30s_v1.ron`;
- sea level: `assets/large_files/terrain/earth_egm2008_geoid_0p5deg_i16cm.raw`;
- runtime package:
  `assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem`;
- the ETOPO1 package remains the documented rollback authority;
- source and runtime checksums are recorded in the dataset documentation and
  manifest. Large assets remain gitignored; manifests, docs, and the baseline
  are committed.

## 10. Commands

```text
cargo run --features dem --bin terrain_metrics -- --quick
cargo run --features dem --bin terrain_metrics -- --full
cargo run --features dem --bin terrain_metrics -- --full --write
cargo run --features dem --bin terrain_metrics -- --full --assert
cargo run --features dem --bin terrain_metrics -- --body moon --quick
```

`--source PATH` overrides the body package, `--grid WxH` changes the measurement
grid, and `--channel-threshold N` changes the stream threshold. Any change to
the last two invalidates comparison with the committed baseline.

## 11. Follow-up work

1. Phase 1 (complete): dataset registry and `preprocess --check` so the harness
   always measures a validated authority.
2. Phase 2: source-raster extrema and land statistics; launch-site height delta
   report; package resolution decision.
3. Phase 3: regional high-resolution metrics against the hillslope and drainage
   targets.
4. Phase 4+: canopy, land cover, and soil metrics as those layers become
   authoritative.
