# Moon LOLA LDEM 16 v3

The Moon terrain authority is NASA PDS's LOLA LDEM_16 global gridded data
product. It is a 16-pixels-per-degree, pixel-registered lunar shape map from
the Lunar Reconnaissance Orbiter's Lunar Orbiter Laser Altimeter (LOLA).

Source product: `LDEM_16`, version `V3.1`, created 2019-03-15 by the LOLA Team
at NASA Goddard Space Flight Center.

Source URL: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/data/lola_gdr/cylindrical/img/ldem_16.img>

PDS label: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/data/lola_gdr/cylindrical/img/ldem_16.lbl>

Dataset description: <https://pds-geosciences.wustl.edu/lro/lro-l-lola-3-rdr-v1/lrolol_1xxx/catalog/gdr_ds.cat>

Verified source SHA-256: `a511e40d7a3ea3275945b4da2a1df377133264fab0be94b7434b1cf8907254cb`

The source is a 5,760 by 2,880 signed-16-bit little-endian, pixel-registered
raster. Each value is multiplied by 0.5 meters to produce height above the
1,737,400-meter lunar reference sphere. It has 1,895.21-meter horizontal
samples, north-to-south rows, and 0 through 360 degree east-positive longitude
in a simple cylindrical projection. The geographic bounds describe pixel edges;
the converter maps onto the source pixel centers and clamps the two poles.

The raster's body-fixed frame is `MEAN EARTH/POLAR AXIS OF DE421`. The active
`pck00011.tpc` orientation authority defines `IAU_MOON` as the same DE421
mean-Earth/polar-axis orientation approximation, so no unreviewed lunar frame
conversion is introduced. The catalog Moon radius is also 1,737,400 meters.

The checked-in configuration references an ignored runtime file generated
offline:

```sh
cargo run --features dem --bin lola_ldem_convert -- \
  assets/large_files/terrain/moon_lola_ldem_16_v3.img \
  assets/large_files/terrain/moon_lola_ldem_16_cs2048_v1.csdem 2048
```

The generated CSDEM stores signed integer-meter elevations on six cube-sphere
faces in `CubeFace::ALL` order. Its resampling adds at most 0.25 meters of
rounding error beyond the source's published sampling and interpolation limits.
The PDS product documents possible artifacts at 45-degree latitude-band edges;
it does not provide a global numeric vertical-error bound.

Generated runtime SHA-256: `c804b5b3cbe7509803cc676a613e9ea38dab2454e97e0cc6b026d1b55b2573e4`

No simulator path downloads terrain data. If the local CSDEM is absent, the
Moon receives no `PlanetTerrain` component and remains non-landable; it never
falls back to procedural terrain. A present but invalid file is a startup
configuration error.
