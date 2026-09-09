## 1. Terrain Authority Cleanup

- [x] 1.1 Remove the temporary `CSMSH` KSC mesh baker and runtime child mesh.
- [x] 1.2 Ensure rocket-mode terrain presentation uses one visible terrain surface
  authority and no overlapping local mesh.

## 2. Local Elevation Package

- [x] 2.1 Define a versioned sparse local elevation package with coverage, nodata,
  source metadata, checksum, and deterministic interpolation.
- [ ] 2.2 Extend `EarthTerrainSource` to select valid measured local elevation
  before its global/procedural fallback and retain site calibration.
- [ ] 2.3 Add an offline converter from an explicitly normalized local raster to
  the runtime package, with input datum and coordinate validation.
- [x] 2.4 Add a KSC local-elevation manifest and provenance document for the
  selected reviewed USGS 3DEP product.

## 3. Data-Backed Presentation

- [ ] 3.1 Restrict refined KSC terrain geometry to local elevation coverage and
  profile bounded level-17 refinement.
- [ ] 3.2 Prepare aligned NAIP imagery levels and DEM-derived normal detail for
  local terrain material presentation.
- [ ] 3.3 Add geodetically anchored KSC structure asset definitions and bounded
  LOD presentation without changing terrain collision.

## 4. Validation

- [ ] 4.1 Test package validation, sampling, nodata, local/global transitions,
  source/collision/render agreement, and KSC pad calibration.
- [ ] 4.2 Test body-fixed placement and asset lifetime for KSC infrastructure.
- [ ] 4.3 Run formatting, checks, clippy, full tests, release build, and bounded
  startup checks for normal, craft, and rocket modes.
- [ ] 4.4 Perform native-display visual and telemetry validation from orbit,
  prelaunch, ascent, and KSC return.
