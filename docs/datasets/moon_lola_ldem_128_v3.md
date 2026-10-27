# Moon LOLA LDEM 128 v3

The Moon terrain authority is NASA PDS's LOLA LDEM_128 global gridded data
product. It is a 128-pixels-per-degree, pixel-registered lunar shape map from
the Lunar Reconnaissance Orbiter's Lunar Orbiter Laser Altimeter (LOLA),
replacing the earlier LDEM_16 authority that shipped 16 samples per degree.

Source product: `LDEM_128`, version `V3.0`, created 2017-09-15 by the LOLA Team
at NASA Goddard Space Flight Center.

Source URL: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/data/lola_gdr/cylindrical/img/ldem_128.img>

PDS label: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/data/lola_gdr/cylindrical/img/ldem_128.lbl>

Dataset description: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/catalog/gdr_ds.cat>

Verified source SHA-256: `0e4fea9b863e3255f58de15ccbd2bb8b1e248cb09217fd55d23ed37489f764b4`

The source is a 46,080 by 23,040 signed-16-bit little-endian, pixel-registered
raster. Each value is multiplied by 0.5 meters to produce height above the
1,737,400-meter lunar reference sphere. It has 236.901-meter horizontal samples
at the equator, north-to-south rows, and 0 through 360 degree east-positive
longitude in a simple cylindrical projection. The geographic bounds describe
pixel edges; the converter maps onto the source pixel centers and clamps the two
poles.

The raster's body-fixed frame is `MEAN EARTH/POLAR AXIS OF DE421`. The active
`pck00011.tpc` orientation authority defines `IAU_MOON` as the same
mean-Earth/polar-axis orientation approximation, so no unreviewed lunar frame
conversion is introduced. The catalog Moon radius is also 1,737,400 meters.

The runtime package is generated offline with:

```sh
cargo run --release --features native --bin lola_ldem_convert -- \
  assets/large_files/terrain/moon_lola_ldem_128_v3.img \
  assets/large_files/terrain/moon_lola_ldem_128_cs4096_v1.csdem 4096
```

The converter infers LDEM_16 versus LDEM_128 from the exact raster byte count, so
a truncated or mislabelled download fails instead of baking a partial Moon. The
generated CSDEM stores signed integer-meter elevations on six cube-sphere faces
in `CubeFace::ALL` order and a precomputed patch-metadata pyramid. Runtime only
validates and deserializes this package. Resampling adds at most 0.25 meters of
rounding error beyond the source's published sampling and interpolation limits.
The PDS product documents possible artifacts at 45-degree latitude-band edges;
it does not provide a global numeric vertical-error bound.

The 4096-sample cube face gives roughly 0.67 km vertex spacing at the equator,
about 2.8 times coarser than the 236.9 m source. A 8192-sample face would match
the source more closely but would make the resident package about 805 MB, so the
current authority keeps the 203 MB (194 MiB) package and a bounded memory
footprint. The source raster remains archived offline for a future re-bake.

Generated runtime SHA-256: `21a87af46d0a6576d7749fe0d270ac94a454f07b64d6c222e1edba7de07d56ee`
(203,423,760 bytes, about 194 MiB).

## Validation

`terrain_metrics --body moon --quick` on the new authority reports
min -8555.6 m, max 10174.1 m, median slope 2.71 degrees, p95 slope 7.89
degrees. The previous LDEM_16 package reports min -8497.3 m, max 10097.9 m,
median slope 2.60 degrees, p95 slope 7.74 degrees. Both harness passes sample a
0.5-degree grid, so these values are deliberately smooth; they confirm the new
package stays in the same physical range rather than exposing its full detail.

The full source raster spans -9127.0 m to 10778.5 m. The baked package spans
-9123 m to 10763 m, so resampling misses the exact extrema by 4 m and 15.5 m
respectively, consistent with bilinear sampling of a 4096-sample cube face.
The source pixel mean is -519.5 m because equirectangular rows over-weight the
poles; area-weighted it is -248.7 m. The cube-sphere package mean is -234.8 m.
The remaining 14 m difference comes from cube-face UV area distortion, not from
a datum or scaling error.

A truncated download cannot be mistaken for a valid product: the converter
checks the exact 46,080 x 23,040 sample count, and a unit test asserts that an
empty or partial LDEM_128 input is rejected before sampling.

Rocket Mode's non-authoritative body-fixed overview is generated offline with:

```sh
cargo run --release --features dem --bin terrain_overview_bake -- moon assets/large_files/terrain/moon_terrain_overview_v1.png
```

Generated overview SHA-256: `0af3afaffe8ad18614cc4fe1a66831c48ca20f55f6f1b6009026d1f7bc80a575`

No simulator path downloads terrain data. If the local CSDEM is absent, the
Moon receives no `PlanetTerrain` component and remains non-landable; it never
falls back to procedural terrain. A present but invalid file is a startup
configuration error.

The previous LDEM_16 CSDEM (`moon_lola_ldem_16_cs2048_v2.csdem`) remains on disk
as a rollback package; reverting the Moon authority only requires pointing
`DEFAULT_MOON_DEM_PATH` back at it.
