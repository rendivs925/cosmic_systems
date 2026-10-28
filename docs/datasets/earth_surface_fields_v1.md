# Earth Measured Surface Fields - Water, Canopy Cover, and Canopy Height v2

Status: Ready for the eight priority regions. Presentation-only measured water
occurrence, tree cover, and mean canopy height. No global fallback.

Each `.cssfld` v2 package carries three measured channels on the same
planetocentric terrain-radial grid:

1. `water_occurrence_pct` from JRC Global Surface Water.
2. `tree_cover_pct` from Hansen Global Forest Change.
3. `canopy_height_m` from Meta/WRI Canopy Height Maps, as whole metres.

The package refines vegetation placement, tree height, and close-range wet-ground
materials. It never contributes to height, altitude, collision, erosion, or
physics, and it is not a second terrain height authority.

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
species placement.

### Meta / WRI Canopy Height Maps

- Dataset: Meta/WRI Canopy Height Maps (CHM), global 1 m product downsampled by
  the publisher.
- Downsampled band: `_avg`, average canopy height in centimetres over a
  1/4000-degree cell (about 28 m), EPSG:4326, uint16, `65535` nodata.
- Tiles: 10-degree GeoTIFFs named by their north-west corner, for example
  `meta_chm_lat=30.0_lon=-90.0_avg.tif`.
- License: CC BY 4.0.
- Required attribution: `Meta/WRI Canopy Height Maps (Tolan et al., 2024)`.
- Reference: Tolan et al. (2024), *Very high resolution canopy height maps from
  RGB imagery using self-supervised vision transformer and convolutional decoder
  trained on aerial lidar*, Remote Sensing of Environment 300, 113888,
  doi:10.1016/j.rse.2023.113888.
- Base URL:
  `https://dataforgood-fb-data.s3.amazonaws.com/forests/v1/alsgedi_global_v6_float_epsg4326_v3_10deg/`.

The source 10-degree tiles are 0.4-1.5 GB each. They are provisioned only by the
offline ingestion script and never downloaded at runtime. The published
downsampled set has no `_avg` tile for the Papua window
(`meta_chm_lat=0.0_lon=130.0_avg.tif`: only a cloud mask is published), so the
Papua canopy channel is zero. Zero means "no measurement" and leaves the
reviewed species profile as the fallback; it is not a claim that the canopy is
zero metres tall.

## Transformation

The source tiles contain up to 1.6 billion pixels. `scripts/ingest_water_canopy.py`
opens each GeoTIFF with PIL and crops only the requested geographic window; PIL
reads the covering strips rather than decoding the whole image. It then:

1. Converts terrain-radial output latitudes to WGS 84 geodetic latitudes before
   sampling.
2. Bilinearly samples water occurrence, tree cover, and average canopy height.
3. Treats water/tree values outside `0..=100` (including `255` nodata) as zero.
4. Converts canopy centimetres to metres, treats `65535` nodata as zero, and
   clamps to whole metres `0..=255`.
5. Writes three north-first grayscale PNGs for `surface_fields_bake`.

The normalized PNGs and source tiles stay under the ignored raw tree.
`src/bin/surface_fields_bake.rs` writes the versioned `.cssfld` package with
`conversion_version: 2`.

Reproduction for one region:

```sh
python3 scripts/ingest_water_canopy.py --region starbase_boca_chica
cargo run --release --features native --bin surface_fields_bake -- \
  assets/large_files/terrain/raw/water_canopy/starbase_boca_chica_water.png \
  assets/large_files/terrain/raw/water_canopy/starbase_boca_chica_tree_cover.png \
  assets/large_files/terrain/raw/water_canopy/starbase_boca_chica_canopy_height.png \
  assets/large_files/terrain/surface_fields_v1/starbase_boca_chica.cssfld \
  -97.30 25.699304258 -97.05 25.948267941 \
  <source-aggregate-sha256> <jrc-url> <hansen-url> <meta-url> \
  'JRC Global Surface Water occurrence + Hansen tree cover + Meta/WRI mean canopy height' \
  'CC BY 4.0; CC BY 4.0; CC BY 4.0' \
  'European Commission, Joint Research Centre (JRC) Global Surface Water; Hansen/UMD/Google/USGS/NASA Global Forest Change; Meta/WRI Canopy Height Maps (Tolan et al., 2024)' \
  30.0
```

All regions:

```sh
python3 scripts/ingest_water_canopy.py --region all
```

## Runtime packages

All packages are `.cssfld` v2 with north-first, terrain-radial 1 arc-second
grids. Package metadata and payload checksums are validated at startup.

| Region | Grid | Bytes | Runtime SHA-256 | Canopy source |
| --- | --- | --- | --- | --- |
| Papua | 3601x4293 | 46,378,559 | `dc9290bebe82bcb80a3737e2bf0ffa0f4c905d6613efb312dba86e22b0cddc9e` | none (no published downsampled tile) |
| KSC | 1441x1256 | 5,431,011 | `d92dc1d0abf0a8f7311eb1dbb69d70cda02fda4afaa9bdc81b88082a524e58a3` | Meta/WRI |
| Starbase | 901x897 | 2,425,916 | `fdfa7dca1adecd186b1272eee947cac58c0fd210b9b645f9350859352ce3d680` | Meta/WRI |
| Vandenberg | 1081x899 | 2,916,782 | `514a98559aeefe6a222ec0d3b6f4c269c8ed0a0a7ef8b646edf334fe116b3952` | Meta/WRI |
| Wallops | 1261x900 | 3,406,023 | `f9416f5cd780e87f3d83203a2b915a783c2b548b36c89b23e10fedf073978f21` | Meta/WRI |
| Kourou | 1801x1253 | 6,771,245 | `4d053a517e94f84680a7f42c2a1c81c6af9cf340fb612cfc6bf6adce61b7b665` | none (tile not provisioned) |
| Baikonur | 1801x1261 | 6,814,505 | `305756b146334d611a9819cb82a5de0a6ecc70b563693fa4dbc5ad026f825129` | Meta/WRI |
| Mahia | 1081x900 | 2,919,987 | `fdd5eeb4f53d9e5ca0d813279e71b0dcac11f41cbb2542a52b5782a22f688ac7` | none (tile not provisioned) |

KSC, Starbase, Vandenberg, Wallops, and Baikonur carry measured Meta/WRI canopy height.
Papua has no published downsampled tile. Kourou and Mahia have published
Meta/WRI tiles, but their 1.5 GB and 0.2 GB source tiles were not provisioned in
the current environment; their canopy channel is zero and the reviewed species
profile remains the fallback. Re-running the offline ingest for those regions
upgrades them without changing the package format or runtime code.

The registry declares each package once for the `Water` layer and once for the
`Canopy` layer because the same file carries all three measured fields.
`preprocess --layer water --check` and `--layer canopy --check` therefore
validate the same runtime artifact from both semantic views.

Source SHA-256 values are aggregates over the source tiles actually used:

```text
sha256(jrc.tif)     <jrc tile URL>
sha256(hansen.tif)  <hansen tile URL>
sha256(meta.tif)    <meta tile URL>          (omitted where no tile is published)
```

## Presentation consumers

- Scatter placement multiplies the existing climate/land-cover density by
  measured tree cover, so open grassland keeps grass while closed forest reaches
  its measured density. Persistently wet sites lose both trees and ground cover.
- Measured mean canopy height scales the reviewed tree profile within a bounded
  `0.6..1.6` factor. Samples below 3 m are treated as sparse/unmeasured cover,
  and zero keeps the profile unchanged. A ~28 m pixel average includes gaps, so
  this is a measured constraint, not an individual-tree height.
- Close-range surfaces darken persistently wet ground toward a shallow water
  tint, giving measured lakes and wide river margins a visible cue without
  creating geometry.
- The existing DEM-derived drainage network and river ribbon mesh remain the
  hydrology geometry authority. JRC occurrence is a measured mask and tint, not
  a height or collision input.

## Validation

- Package round-trip, checksum rejection, coverage, bilinear sampling,
  field-name identity, and overlapping-package precedence are covered by
  `surface_fields.rs` tests.
- Scatter and wet-ground material behavior are covered by `surface` tests,
  including a measured-canopy-height profile-scaling test.
- `preprocess --layer water --check` and `--layer canopy --check` verify all
  runtime SHA-256 values.
- No physics baseline was regenerated.

## Rollback

Delete the `.cssfld` packages and remove the corresponding registry entries; the
runtime falls back to the reviewed species profiles and procedural wetness. No
terrain height, collision, or physics value changes.
