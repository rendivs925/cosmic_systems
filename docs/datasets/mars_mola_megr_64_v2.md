# Mars MOLA MEGR 64 v2

The Mars terrain authority is the Mars Global Surveyor Mars Orbiter Laser
Altimeter (MOLA) `MEGR` global mean-radius map at 64 pixels per degree,
replacing the earlier 32-pixel-per-degree authority. The product is distributed
by the PDS as four 180 x 90 degree tiles; this repository stitches them into one
north-to-south, east-positive global raster before baking.

## Source

- Product: MOLA MEGR90N000GB / MEGR90N180GB / MEGR00N000GB / MEGR00N180GB,
  PDS product version `2.0`, created 2003-03-21.
- Resolution: 64 pixels per degree, 926 m per sample.
- Grid: 23,040 columns by 11,520 rows after stitching, signed 16-bit big-endian.
- Stored value plus `3,396,000 m` is the mean planetary radius from the Mars
  centre.
- Rows run north to south; longitude is east-positive 0-360 degrees;
  latitude is planetocentric. Pixel edges bound the poles and the longitude
  seam, so conversion samples pixel centers, wraps longitude, and clamps the
  polar rows.
- Tiles:
  - <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg064/megr90n000gb.img>
  - <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg064/megr90n180gb.img>
  - <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg064/megr00n000gb.img>
  - <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg064/megr00n180gb.img>

Each tile is 132,710,400 bytes. Verified SHA-256 values:

| Tile | SHA-256 |
| --- | --- |
| `megr90n000gb` | `a23d70033211335092ba6c02f51d24e74207323009aadacf04fcd4a84b02497b` |
| `megr90n180gb` | `ba8c17534342b2bf84eee8754652aa896ce98416b839a81cc1ecc296ab7ad927` |
| `megr00n000gb` | `e7efab659ceecb5cfe6a03dd4c9ff910dc814004ec0cff2bbe81d03df00f3c71` |
| `megr00n180gb` | `47869677db4215c4e97b056f2177fe5d7a28018ba3f3abe60f550edeb9d0058b` |

Aggregate source SHA-256 (sorted `sha256  url` lines):
`10def4f245623c67032c2df929dcf5f52c1178c3d3643e9440e0d1e28e8caac9`.
The stitched raster is 530,841,600 bytes.

The `MEGR` radius map is still selected instead of `MEGT` topography: `MEGT`
heights are relative to the MOLA areoid, while the terrain contract requires
height above the simulator's catalog mean radius. `MEGR` supplies direct
planet-centre radius, avoiding a second areoid authority.

## Pipeline

```sh
python3 scripts/ingest_mola_megr_64.py
cargo run --release --features native --bin mola_megr_convert -- \
  assets/large_files/terrain/mars_mola_megr_64_v2.img \
  assets/large_files/terrain/mars_mola_megr_64_cs4096_v1.csdem 4096
cargo run --release --features dem --bin terrain_overview_bake -- mars \
  assets/large_files/terrain/mars_terrain_overview_v1.png
```

The ingest script downloads each tile with parallel range requests, verifies
the exact tile size, stitches the four quadrants, and writes a deterministic
aggregate SHA-256 sidecar over the tile files. The converter infers MEGR 32
versus MEGR 64 from the exact stitched raster size, so a truncated download or a
single unstitched tile is rejected before sampling.

The converter forms each CSDEM elevation as:

```text
height_m = (stored_i16_be + 3,396,000 m) - 3,389,500 m
```

`3,389,500 m` is the existing Mars catalog radius passed by the converter from
`PlanetFactory`, not a new constant.

A 4096-sample cube face gives roughly 1.3 km vertex spacing at the Mars equator,
compared with 926 m source samples. The earlier 2048-sample authority gave about
2.6 km spacing. An 8192-sample face would resolve the source more closely but
would make the resident package about 805 MB; that remains deferred.

Generated runtime SHA-256:
`0ec97e6affcb0c41e530b5b69cf903cd626bf045315c4d405c9cba7dc6e1afb4`
(203,423,760 bytes, about 194 MiB).

## Frame requirement

The MOLA label declares `IAU2000_MARS`. The active `pck00011.tpc` uses newer Mars
orientation data and is not compatible with that cartographic frame. NAIF's
`mars_iau2000_v1.tpc` remains the reviewed override and is loaded after
`pck00011.tpc`, exactly as before. No frame, datum, or radius authority changed
in this upgrade.

## Validation

The stitched source spans -16,675 m to 27,841 m above the catalog mean radius,
with an area-weighted mean of 0.15 m. The baked package spans -16,666 m to
27,834 m with a cube-face-weighted mean of -49.2 m. The extrema agree within
10 m; the mean shift comes from cube-face UV area distortion, not from a datum
or scaling error.

`terrain_metrics --body mars --quick` reports:

| Metric | MEGR 32 (2048) | MEGR 64 (4096) |
| --- | --- | --- |
| min | -16,230.9 m | -16,233.7 m |
| max | 27,652.5 m | 27,660.4 m |
| median slope | 0.41 deg | 0.42 deg |
| p95 slope | 1.93 deg | 1.95 deg |
| drainage density | 0.0034 km/km2 | 0.0031 km/km2 |

The 0.5-degree harness grid deliberately smooths both packages; the resolution
gain is visible at close range and in the regenerated overview.

A truncated download or an unstitched single tile cannot be baked: the converter
accepts only the exact 132,710,400-byte MEGR 32 or 530,841,600-byte stitched
MEGR 64 raster, and a unit test asserts that empty input is rejected.

No physics baseline was regenerated.

Rocket Mode's non-authoritative body-fixed overview was regenerated from the new
authority. Generated overview SHA-256:
`6fb8e8a2ba2d17961b209be59898a05fbaf897504a7694262d455eee9b1617bc`.

## Rollback

`assets/large_files/terrain/mars_mola_megr_32_cs2048_v2.csdem` and its source
raster remain on disk. Reverting the authority only requires pointing
`DEFAULT_MARS_DEM_PATH` back at the 32 ppd package.
