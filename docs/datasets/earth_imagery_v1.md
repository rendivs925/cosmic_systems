# Earth Offline Imagery Package v1

## Status

The repository provides the deterministic cube-sphere package format and
converter, but intentionally does not ship a source imagery package. Operators
must select a reviewed, redistributable source and update the accompanying RON
manifest before enabling it.

## Contract

- Body: Earth
- Frame: IAU_EARTH body-fixed directions
- Tile identity: cube face, quadtree level, tile x, tile y
- Encoding: RGBA8 PNG tiles generated from pixel centers
- Runtime: optional presentation data only; no network access, collision, or
  terrain-height authority

## Preparation

```text
cargo run --bin earth_imagery_convert -- \
  source-equirectangular.png \
  assets/large_files/terrain/earth_imagery_v1/tiles \
  0 12 256
```

Record the source URL, version, redistribution license, SHA-256, geographic
datum, coverage, and source resolution in
`assets/configs/terrain/earth_imagery_v1.ron`. Generated package files remain
ignored under `assets/large_files/`.

## USGS NAIP Local Detail

The initial reviewed source is the USGS National Map NAIP imagery service:

- Service: `https://imagery.nationalmap.gov/arcgis/rest/services/USGSNAIPImagery/ImageServer`
- License: public domain; preserve the service attribution `USGS, USDA, The
  National Map: Orthoimagery`.
- Coverage and resolution: the service reports conterminous-US coverage and a
  0.3 m source pixel size. Record the export date and exact requested bounds
  because the mosaic can change.

Export a natural-color PNG in EPSG:4326 using the source's REST `exportImage`
endpoint. Its columns must increase eastward and its rows must increase
southward. Then bake only the cube patch containing the exported corridor:

```text
cargo run --bin earth_imagery_crop_convert -- \
  ksc_naip.png \
  assets/large_files/terrain/earth_imagery_v1/tiles \
  neg_z 12 <tile-x> <tile-y> \
  <west-deg> <south-deg> <east-deg> <north-deg> 1024
```

Add that exact `TerrainPatch` to `coverage_tiles` in the manifest. Sparse tiles
are alpha-masked outside the exported bounds and are only selected for their
declared descendants; terrain outside coverage continues to use the global
albedo fallback. This is presentation-only and does not alter the DEM,
collision, or terrain authority.

### Configured KSC Tile

- Export date: 2026-09-08
- Geographic bounds: west `-80.65567289`, south `28.56358770`, east
  `-80.62843612`, north `28.58676190` degrees in EPSG:4326.
- Export: `4000×3400` RGBA natural-color PNG from the `exportImage` endpoint.
- Source SHA-256: `cbcb50518c9aae1ce920d1494af84e219faf08c76f46e3627768af7d7c5f0bf2`.
- Cube coverage: `NegZ`, level `12`, tile `(2385, 3178)`.
- Baked output: `2048×2048` RGBA PNG, SHA-256
  `4cfd7065a9605ee2ea09afd89215ce6a52f3ba4ec1d7087ff6dfb1f1d395d8e7`.
