## 1. Shared Terrain Detail

- [x] 1.1 Add a deterministic, seam-safe local-detail terrain wrapper after procedural Earth erosion, with bounded ridged and drainage-like relief appropriate for near-surface patch sampling.
- [x] 1.2 Replace hard site cutoffs with a pad-scale flat inner footprint and deterministic feathered transition to the shared base terrain.
- [x] 1.3 Add pure terrain-source tests for deterministic detail, non-flat nearby terrain, pad flatness, and continuous site boundaries.

## 2. Earth Elevation Data

- [x] 2.1 Add one documented application-level configuration path for an optional local SRTM directory in DEM-enabled Earth terrain selection.
- [x] 2.2 Use raw `DemTerrainSource` data directly when configured, retaining procedural fallback for unavailable coverage and avoiding the coarse erosion wrapper.
- [x] 2.3 Reorder DEM cache lookup and on-disk tile loading so cache hits never parse the same tile again.
- [x] 2.4 Add DEM-feature tests for configured coverage, fallback behavior, interpolation, and tile-cache reuse.

## 3. Streaming Reuse

- [x] 3.1 Generate patch geometry only when a requested patch is not already valid in the streaming cache.
- [x] 3.2 Add streaming regressions proving stable visible patches retain geometry and newly requested patches still produce ready events.

## 4. Validation

- [x] 4.1 Run formatting, default checks, clippy, and all default tests.
- [x] 4.2 Run DEM-feature checks and tests, then smoke-test rocket mode with procedural fallback and, when a local SRTM directory is available, real elevation data.
