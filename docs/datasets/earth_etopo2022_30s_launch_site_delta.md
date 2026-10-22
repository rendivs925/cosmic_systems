# Earth ETOPO 2022 Launch-Site Height Delta

This report records the signed height change at the priority launch sites when
the Earth authority moved from the ETOPO1 Ice Surface mean-sea-level package
(`earth_etopo1_ice_surface_cs2048_v2.csdem`) to the ETOPO 2022 30 arc-second WGS
84 ellipsoid package
(`earth_etopo2022_30s_ice_surface_cs2048_v3.csdem`).

The delta mixes two reviewed changes:

1. the vertical datum change from mean sea level to the WGS 84 ellipsoid, which
   contributes the local EGM2008 geoid undulation; and
2. the relief upgrade from 1 arc-minute ETOPO1 to 30 arc-second ETOPO 2022.

It is therefore not a pure source-elevation difference. The physics regression
baselines were regenerated only after this table was reviewed.

Generated with:

```sh
cargo run --features dem --bin terrain_site_delta -- \
    assets/large_files/terrain/earth_etopo1_ice_surface_cs2048_v2.csdem \
    assets/large_files/terrain/earth_etopo2022_30s_ice_surface_cs2048_v3.csdem
```

| Site | Latitude | Longitude | ETOPO1 m | ETOPO 2022 m | Delta m |
| --- | ---: | ---: | ---: | ---: | ---: |
| Kennedy Space Center LC-39A | 28.6084 | -80.6043 | -5.1 | -33.3 | -28.2 |
| Starbase / Boca Chica | 25.9967 | -97.1560 | -0.1 | -22.0 | -21.9 |
| Vandenberg SLC-4E | 34.6321 | -120.6108 | 112.8 | 85.1 | -27.6 |
| Wallops Pad 0A | 37.8340 | -75.4870 | -3.3 | -40.7 | -37.4 |
| Kourou ELA-3 | 5.2360 | -52.7686 | 9.4 | -28.0 | -37.4 |
| Baikonur Site 1/5 | 45.9200 | 63.3422 | 107.7 | 75.1 | -32.7 |
| Mahia LC-1A | -39.2608 | 177.8647 | -9.2 | 2.7 | 11.9 |
| Papua coastal lowland | -8.0000 | 139.5000 | 9.2 | 77.0 | 67.8 |

Interpreting the deltas:

- Northern-hemisphere sites fall by about 22 to 37 m, consistent with the local
  EGM2008 geoid being below the ellipsoid.
- Papua rises by about 68 m, consistent with the local geoid being above the
  ellipsoid.
- The residual after removing the geoid is the 30 arc-second relief upgrade.

`terrain_site_delta` samples the cube-sphere packages directly; it does not apply
erosion or the bounded procedural detail layer, so the runtime launch surface can
differ by the documented sub-merit-scale detail contribution.
