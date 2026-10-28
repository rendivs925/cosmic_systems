# Yale Bright Star Catalogue 5 v1

The flight and solar-map starfield uses the 9,110 stars of the Yale Bright Star
Catalogue, 5th Revised Edition, instead of an isotropic procedural
distribution. The layer is presentation-only: no orbital, attitude, terrain,
collision, or physics system reads it.

## Source

- Catalogue: Yale Bright Star Catalogue, 5th Revised Edition (BSC5), compiled by
  Dorrit Hoffleit and Wayne H. Warren Jr., 1991.
- Distribution: SAO Telescope Data Center binary release with J2000
  coordinates, <http://tdc-www.harvard.edu/catalogs/BSC5>.
- Format: the documented SAO TDC binary catalog layout
  (<http://tdc-www.harvard.edu/catalogs/catalogsb.html>); 9,110 entries,
  32 bytes each, V magnitude in hundredths, two-character spectral type.
- Verified source SHA-256:
  `e471d02eaf4eecb61c12f879a1cb6432ba9d7b68a9a8c5654a1eb42a0c8cc340`
  (291,548 bytes).
- Completeness: essentially complete to V = 7; the package spans
  V = -1.46 (Sirius) to V = 7.96, with 8,418 stars at or above the naked-eye
  limit of V = 6.5.

The TDC binary has no B-V column. The ingest step derives B-V from the
two-character spectral type with main-sequence anchors, for example B5 = -0.16,
A0 = 0.00, G0 = 0.58, K5 = 1.15, M5 = 1.64. This is a documented photometric
approximation; it is not measured colour.

No explicit license file is published with the BSC5 binary distribution. It is
a public astronomical catalogue distributed by the SAO Telescope Data Center
and VizieR (V/50). Cite Hoffleit & Warren (1991) when using it.

## Package

`assets/data/stars/yale_bsc5_v1.csstar` is a little-endian, versioned package:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 8 | magic `CSSTAR\0\0` |
| 8 | 4 | format version, `1` |
| 12 | 4 | record count |
| 16 | 16 x count | `f32` RA (rad), `f32` Dec (rad), `f32` V mag, `f32` B-V |

- Size: 145,776 bytes (16-byte header plus 9,110 records).
- SHA-256: `99d6edb998fa38edd43213c20b93dfb358a2905ce7064587854e5f0c4e09ca63`.
- The package is embedded at build time, so startup has no presentation-critical
  file dependency and the same catalog is available on native and wasm targets.
  A damaged package falls back to the retained procedural starfield.

```sh
python3 scripts/ingest_bsc5.py
```

## Frames

Right ascension and declination are ICRS/J2000. The loader converts each
direction with the authoritative
`reference_frames::icrf_j2000_to_solar_inertial`, the same mapping used for
every ephemeris position, so bright stars share the solar system's axes.
The project's solar axes order (+X, +ecliptic north, +ecliptic Y) and therefore
apply a reflection to the whole scene; the catalog is mapped exactly like the
bodies, not corrected separately.

The Milky Way band is generated in IAU galactic coordinates rather than an
arbitrary rotated band. The north pole is ICRS RA 192.85948 deg,
Dec +27.12825 deg and the centre is RA 266.40510 deg, Dec -28.936175 deg, the
J2000 adoption used by Hipparcos and Gaia. Band stars and the central bulge
therefore pass through the measured Cygnus/Sagittarius star clouds.

## Runtime presentation

- Apparent brightness is a monotonic function of V magnitude anchored at the
  naked-eye limit; Sirius and Canopus dominate, and stars fainter than V = 7.2
  fade out.
- Colour follows B-V through the Ballesteros (2012) effective-temperature
  relation and a blackbody-to-sRGB approximation. Presentation only; it is not
  calibrated photometry.
- Stars brighter than V = 1.5 receive a soft halo, replacing the previous
  random bright-star tail.
- The measured catalogue replaces the former 10,000-star isotropic procedural
  layers. The procedural Milky Way density and dust lanes remain, now aligned to
  the real galactic plane.

## Validation

- `cargo test --features dem --lib star_catalog` checks package round-trip,
  truncation, version, range rejection, all 9,110 embedded records, the Sirius
  position, unit directions, and the galactic basis.
- `cargo test --features dem --lib starfield` checks brightness monotonicity,
  B-V colour ordering, one quad per visible catalogued star, finite positions,
  and the procedural fallback.
- An offline 2048 x 1024 equirectangular plot of the baked package shows the
  expected galactic-plane overdensity and the bright-star distribution; the
  package re-bakes to the same SHA-256 from the cached source.
- A bounded Xvfb startup logged `starfield: 9110 measured stars from Yale BSC5`
  and ran without panic; Xvfb does not present the GPU frame, so window-pixel
  inspection was not possible.
- No physics baseline was regenerated; no physics system reads the catalogue.
