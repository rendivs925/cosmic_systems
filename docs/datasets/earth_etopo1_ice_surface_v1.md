# Earth ETOPO1 Ice Surface v1

The Earth terrain authority is NOAA/NCEI ETOPO1 Ice Surface, grid-registered,
one arc-minute global relief. The source raster uses WGS 84 geographic
coordinates (EPSG:4326) and mean sea-level heights (EPSG:5715).

Source archive: `etopo1_ice_g_i2.zip`

Verified source SHA-256: `877cbe01350b009583fd3b9c5ea4231e269484fbdaeb513b17b3a1bfdccce1ce`

Source URL: <https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO1/data/ice_surface/grid_registered/binary/etopo1_ice_g_i2.zip>

The checked-in runtime file is generated offline:

```sh
cargo run --features dem --bin etopo1_convert -- etopo1_ice_g_i2.bin assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v1.csdem 2048
```

The runtime file stores signed 16-bit metre elevations on six cube-sphere faces
in `CubeFace::ALL` order. No simulator path downloads data at runtime. If the
file is intentionally omitted, native startup uses the deterministic procedural
Earth fallback; a present but invalid file is a startup configuration error.

Generated runtime SHA-256: `cc20e13cf65d10b856e9697960922951f6943585b9be74d5b6bd28d75f88e141`
