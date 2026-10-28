# Earth Planet Albedo v1

The Earth planet material now uses the reviewed 8192x4096 NASA Blue Marble
global overview instead of the older 2048x1024 texture. The layer is
presentation-only; terrain height, collision, atmosphere, and flight dynamics
are unchanged.

## Source

- Dataset: NASA Blue Marble: Next Generation, August 2004 global composite with
  topography and bathymetry.
- Source file:
  <https://eoimages.gsfc.nasa.gov/images/imagerecords/74000/74117/world.200408.3x21600x10800.jpg>
- Source: 21600x10800 (2 km/pixel), SHA-256
  `926be7253be90a1fd57071a33afbfe7a712a2dfe0d5b5b4602bc5dfbcfdc2c89`.
- Licence: public domain (NASA).
- Attribution: NASA Earth Observatory / Reto Stockli (NASA GSFC).

The repository already validates this source as the
`earth_imagery_blue_marble_global` terrain dataset. Its reviewed runtime asset,
`assets/large_files/terrain/earth_imagery_v1/global_overview.png`, is an
8192x4096 downsample produced by `imagery_convert` with SHA-256
`9b108ac20def521dfa6035d906e4fa33097cfd205689b0f3211d4b05e416373f`.

## Runtime asset

- `assets/textures/planets/earth/albedo.png`
- 8192x4096 RGB PNG, 19,790,949 bytes.
- SHA-256: `9b108ac20def521dfa6035d906e4fa33097cfd205689b0f3211d4b05e416373f`
  (byte-identical to the reviewed terrain overview).
- Loaded by the existing planet material path; no new loader or registry entry.

The planet texture is tracked even though the terrain overview is gitignored.
This keeps `cargo run` functional from a fresh checkout without duplicating the
processing pipeline or requiring the large terrain asset tree. The two copies
are byte-identical and verified by checksum.

## Alignment and validation

- Direct, zero-shift correlation with the superseded 2048 texture scored 0.747,
  the best of direct, longitude-mirrored, latitude-flipped, and shifted
  variants. No rotation, mirroring, or longitude offset was applied.
- Dimensions, mode, and checksum match the reviewed runtime overview.
- The Xvfb startup smoke loaded the texture without errors.
- No physics baseline was regenerated; the texture is not read by terrain,
  collision, altitude, or flight dynamics.

## Limitations

- August 2004 composite: seasonal snow and vegetation are fixed to that month.
- Bathymetry and topographic shading are part of the published display mosaic,
  not a calibrated reflectance product.
- Clouds remain the separate `textures/planets/earth/clouds.png` layer; they are
  not baked into this albedo.
- The previous 2048 texture remains in git history for rollback; reverting the
  PNG restores it without code changes.
