# Earth Measured Surface Fields - Water and Canopy v1

Status: Ready for the eight priority regions. Presentation-only measured water
occurrence and tree cover. No global fallback yet.

Each `.cssfld` package carries two measured channels on the same
planetocentric terrain-radial grid:

1. `water_occurrence_pct` from JRC Global Surface Water.
2. `tree_cover_pct` from Hansen Global Forest Change.

The package refines vegetation placement and close-range wet-ground materials.
It never contributes to height, altitude, collision, erosion, or physics, and it
is not a second terrain height authority.

## Sources

### JRC Global Surface Water

- Dataset: Global Surface Water 2021, occurrence layer, version 1.4.
- Resolution: 30 m.
- Tiles: 10-degree GeoTIFFs, named by their west and north edges, for example
  `occurrence_90W_30Nv1_4_2021.tif`.
- License: CC BY 4.0.
- Required attribution: `European Commission, Joint Research Centre (JRC)
  Global Surface Water`.
- Base URL:
  `https://storage.googleapis.com/global-surface-water/downloads2021/occurrence/`.

The stored values are occurrence percentages `0..=100`; `255` is nodata and is
written as zero occurrence.

### Hansen Global Forest Change

- Dataset: Global Forest Change 2023 v1.11, `treecover2000`.
- Resolution: 30 m.
- Tiles: 10-degree GeoTIFFs, named by their west and north edges, for example
  `Hansen_GFC-2023-v1.11_treecover2000_30N_090W.tif`.
- License: CC BY 4.0.
- Required attribution: `Hansen/UMD/Google/USGS/NASA Global Forest Change`.
- Base URL:
  `https://storage.googleapis.com/earthenginepartners-hansen/GFC-2023-v1.11/`.

`treecover2000` is measured percentage tree cover, not canopy height. The
runtime uses it as a measured density constraint on the existing deterministic
species placement; per-tree height still comes from the reviewed species
profiles. A measured height source such as GEDI remains future work.

## Transformation

Both source tiles contain 1.6 billion pixels. `scripts/ingest_water_canopy.py`
opens each GeoTIFF with PIL and crops only the requested geographic window;
PIL reads the covering tiles rather than decoding the whole image. It then:

1. Converts terrain-radial output latitudes to WGS 84 geodetic latitudes before
   sampling.
2. Bilinearly samples water occurrence and tree cover.
3. Treats values outside `0..=100` (including `255` nodata) as zero.
4. Writes two north-first grayscale PNGs for `surface_fields_bake`.

The normalized PNGs and source tiles stay under the ignored raw tree.
`src/bin/surface_fields_bake.rs` writes the versioned `.cssfld` package.

Reproduction for one region:

```sh
python3 scripts/ingest_water_canopy.py --region starbase_boca_chica
cargo run --release --features native --bin surface_fields_bake -- \
  assets/large_files/terrain/raw/water_canopy/starbase_boca_chica_water.png \
  assets/large_files/terrain/raw/water_canopy/starbase_boca_chica_tree_cover.png \
  assets/large_files/terrain/surface_fields_v1/starbase_boca_chica.cssfld \
  -97.30 25.699304258 -97.05 25.948267941 \
  <source-aggregate-sha256> <jrc-url> <hansen-url> \
  'JRC Global Surface Water occurrence + Hansen tree cover' \
  'CC BY 4.0; CC BY 4.0' \
  'European Commission, Joint Research Centre (JRC) Global Surface Water; Hansen/UMD/Google/USGS/NASA Global Forest Change' \
  30.0
```

All regions:

```sh
python3 scripts/ingest_water_canopy.py --region all
```

## Runtime packages

All packages are `.cssfld` v1 with north-first, terrain-radial 1 arc-second
grids. Package metadata and payload checksums are validated at startup.

| Region | Grid | Bytes | Runtime SHA-256 |
| --- | --- | --- | --- |
| Papua | 3601x4293 | 30,919,222 | `0e204c9605e3bc045e9c5b02e1139a340af20f823fd1f744cae17f71cdb5b8e8` |
| KSC | 1441x1256 | 3,620,828 | `067cc520a624e457a21b030b38cf6c9dc62b99262d3b9108b55a3737b81cf01e` |
| Starbase | 901x897 | 1,617,431 | `c045c752fc0825e64c7ea908669896442af52722f04e72177c61104189ca0a36` |
| Vandenberg | 1081x899 | 1,944,675 | `a9bd7c7d4b893c0fb11e851c15cc9e77d50ee0ac17a8d96056e30d776dd693ce` |
| Wallops | 1261x900 | 2,270,836 | `c828d11ecf1ada7eb31552a426788c23db9bb36ad0ab31cc5576d6410781a708` |
| Kourou | 1801x1253 | 4,514,342 | `4bc45528f7f4a8d94c2faa72ee38a55158297f901f260ce79a4a826d354e47e6` |
| Baikonur | 1801x1261 | 4,543,158 | `8c6c5a253d632e7a13f26ecbb5888a677b389f6dc7109153006d83dd202aa607` |
| Mahia | 1081x900 | 1,946,837 | `4123c8f4a6b9eabdecfec732445464ea17a11cdde7057aa353ea932b590298e4` |

The registry declares each package once for the `Water` layer and once for the
`Canopy` layer because the same file carries both measured fields. `preprocess
--layer water --check` and `--layer canopy --check` therefore validate the same
runtime artifact from both semantic views.

Source SHA-256 values are aggregates over the two source tiles:

```text
sha256(jrc.tif)     <jrc tile URL>
sha256(hansen.tif)  <hansen tile URL>
```

## Presentation consumers

- Scatter placement multiplies the existing climate/land-cover density by
  measured tree cover, so open grassland keeps grass while closed forest reaches
  its measured density. Persistently wet sites lose both trees and ground cover.
- Close-range surfaces darken persistently wet ground toward a shallow water
  tint, giving measured lakes and wide river margins a visible cue without
  creating geometry.
- The existing DEM-derived drainage network and river ribbon mesh remain the
  hydrology geometry authority. JRC occurrence is a measured mask and tint, not
  a height or collision input.

## Validation

- Package round-trip, checksum rejection, coverage, bilinear sampling, field-name
  identity, and overlapping-package precedence are covered by
  `surface_fields.rs` tests.
- Scatter and wet-ground material behavior are covered by `surface` tests.
- `preprocess --layer water --check` and `--layer canopy --check` verify all
  runtime SHA-256 values.
- No physics baseline was regenerated.
