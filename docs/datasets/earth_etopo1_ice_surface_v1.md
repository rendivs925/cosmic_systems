# Earth ETOPO1 Ice Surface v1

The Earth terrain authority is NOAA/NCEI ETOPO1 Ice Surface, grid-registered,
one arc-minute global relief. The source raster uses WGS 84 geographic
coordinates (EPSG:4326) and mean sea-level heights (EPSG:5715).

Source archive: `etopo1_ice_g_i2.zip`

Verified source SHA-256: `877cbe01350b009583fd3b9c5ea4231e269484fbdaeb513b17b3a1bfdccce1ce`

Source URL: <https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO1/data/ice_surface/grid_registered/binary/etopo1_ice_g_i2.zip>

The checked-in runtime file is generated offline:

```sh
cargo run --features dem --bin etopo1_convert -- etopo1_ice_g_i2.bin assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem 2048
```

The runtime file stores signed 16-bit metre elevations on six cube-sphere faces
in `CubeFace::ALL` order and its complete precomputed patch-metadata pyramid.
Native Earth startup only validates and deserializes this measured package; it
does not rebuild terrain metadata. No simulator path downloads data at runtime;
a missing or invalid package is a native startup configuration error.

Generated runtime SHA-256: `5402d634be8c4ac3897eab53c3f60ee213000752f26bb426ba9c22fdafe60c83`

Rocket Mode's non-authoritative body-fixed overview is generated offline from
this package and the shared terrain appearance law:

```sh
cargo run --features dem --bin terrain_overview_bake -- earth assets/large_files/terrain/earth_terrain_overview_v1.png
```

Generated overview SHA-256: `7152bc0e46aa8e1df85cef48c2ccb546cabce6b6f27813bab005809d206dea1f`
