# Moon Albedo v1

The Moon's planet albedo is upgraded from a 2048x1024 texture to an 8192x4096
LROC color mosaic. The layer is presentation-only: terrain height, collision,
altitude, orientation, and every physics value remain owned by the existing
LOLA CSDEM and kernel authorities.

## Source

- Product: NASA Scientific Visualization Studio 4720, "Moon Phase and
  Libration", LRO LROC WAC color mosaic with polar coverage.
- Source file:
  <https://svs.gsfc.nasa.gov/vis/a000000/a004700/a004720/lroc_color_poles_8k.tif>
- Source properties: 8192x4096, RGB, equirectangular, 50,641,970 bytes.
- Verified source SHA-256:
  `4af8b0cd4d50c30851359d98e7e72040240dd8d03256b58b345b5b76e9edb4ef`.
- Licence: NASA public domain. LRO/LROC data courtesy NASA/GSFC and Arizona
  State University.

The source is already an equirectangular map in the body-fixed
longitude/latitude layout used by the existing texture. No geometric
reprojection, rotation, or mirroring is applied. Alignment was verified by
cross-correlating the new map with the superseded texture: the direct,
zero-shift orientation scored highest (0.77), while longitude-mirrored,
latitude-flipped, and shifted variants scored lower. Mare Crisium (17N, 59E)
and Tycho (43S, 11W) land at the expected map positions.

## Processing

`scripts/ingest_moon_albedo.py` downloads the source into the ignored
`assets/large_files/imagery/raw/moon/` cache, verifies its exact size and
SHA-256, converts it to RGB, and applies a linear luminance contrast match to
the superseded 2048 texture (mean 0.580, std 0.1354). The match keeps the
Moon's established on-screen brightness continuous across the resolution
upgrade; it is a display calibration, not an absolute photometric claim.
Clipping affects under 0.05% of samples.

```sh
python3 scripts/ingest_moon_albedo.py
```

## Runtime asset

- `assets/textures/planets/moon/albedo.png`
- 8192x4096 RGB PNG, 52,267,071 bytes.
- SHA-256: `9722e1547ac673036ad0ddc6e720ebb0722a640012626a73120ba027e394a491`.
- Loaded by the existing planet material path (`textures/planets/moon/albedo.png`);
  no new runtime loader, package format, or asset registry is introduced.
- No normal map, emissive map, or displacement is added.

## Validation

- Source dimensions and SHA-256 match the recorded contract.
- Zero-shift correlation against the superseded texture and known-feature
  placement confirm longitude/latitude alignment. At the recorded coordinates,
  Mare Crisium (17N, 59E) averages luminance 0.272 against 0.539 in the nearby
  highland, and Tycho (43S, 11W) averages 0.845 against 0.740 nearby.
- Output luminance mean/std match the display target (0.5800/0.1353).
- No physics baseline was regenerated; the texture is not read by terrain,
  collision, or flight dynamics.

## Limitations

- The source includes smooth polar fill rows; they are a property of the
  published mosaic, not of the terrain.
- Colour is the published display mosaic, not calibrated photometry.
- The previous texture remains in git history for rollback; reverting the PNG
  restores the 2048 authority without code changes.
