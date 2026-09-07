# Mars MOLA MEGR 32 v2

## Status

This document records the accepted source contract and implemented offline
converter for Mars terrain. The catalog selects the Mars terrain authority only
when the `dem` feature is enabled, but no full local CSDEM is available in this
checkout. Startup therefore leaves Mars non-landable rather than substituting
procedural terrain.

## Source

The selected source is the Mars Global Surveyor Mars Orbiter Laser Altimeter
(MOLA) `MEGR90N000FB` global mean-radius map, PDS product version `2.0`,
created 2003-04-11. It is a 32-pixels-per-degree shape map formed from nearly
600 million MOLA observations.

- Source raster: <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg032/megr90n000fb.img>
- PDS4 label: <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg032/megr90n000fb.xml>
- Original PDS3 label: <https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg032/megr90n000fb.lbl>

The `MEGR` radius map is intentionally selected instead of the `MEGT`
topography map. `MEGT` heights are relative to the MOLA areoid, while the
terrain contract requires height above the simulator's catalog mean radius.
`MEGR` supplies direct planet-center radius for each sample, avoiding a second
areoid authority and preserving one physical terrain surface for rendering and
collision.

The raster is 11,520 columns by 5,760 rows of signed 16-bit big-endian values.
Each stored value is added to `3,396,000 m` to obtain mean planetary radius.
Rows run north to south; samples use east-positive longitude and planetocentric
latitude. The stated 0/360 degree longitudes and +/-90 degree latitudes bound
pixel cells, so conversion must sample at pixel centers, wrap longitude, and
clamp the polar rows.

The converter forms a CSDEM elevation as:

```text
height_m = (stored_i16_be + 3,396,000 m) - 3,389,500 m
```

`3,389,500 m` is the existing Mars catalog radius. The converter receives that
value from the catalog authority rather than introducing another radius constant
and rejects converted values outside CSDEM's signed-meter range.

## Frame Requirement

The MOLA label declares `IAU2000_MARS`. The active `pck00011.tpc` uses newer
Mars orientation data and is not compatible with that cartographic frame.
NAIF's `mars_iau2000_v1.tpc` is the reviewed solution: it provides the 2000 IAU
Mars orientation and is explicitly designed to load safely after `pck00011.tpc`.

- Kernel: <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/mars_iau2000_v1.tpc>
- NAIF generic PCK notes: <https://naif.jpl.nasa.gov/pub/naif/generic_kernels/pck/aareadme.txt>

The shared ephemeris manifest loads `pck00011.tpc` first and the reviewed
`mars_iau2000_v1.tpc` override second. The override is applied only to the
Mars-system orientation data; its verified SHA-256 is
`07ba38b939ae92c085882752a523addd749fde0abb7a3468423099ed02bb3949` and its
size is 12,573 bytes. Mars orientation provenance identifies this override,
while every other body retains `pck00011.tpc`. This is a shared ephemeris
authority change, not a terrain-local coordinate conversion.

## Offline Conversion

`mola_megr_convert` validates the exact 132,710,400-byte input size, decodes
big-endian samples, applies the offset and catalog-radius subtraction above,
and resamples through the existing cube-sphere mapping. Runtime terrain sampling
only reads the generated CSDEM; it never reads or downloads the PDS raster.

Provision the ignored source raster and CSDEM with:

```sh
curl --fail --location --continue-at - \
  --output assets/large_files/terrain/mars_mola_megr_32_v2.img \
  https://pds-geosciences.wustl.edu/mgs/urn-nasa-pds-mgs_mola_topography_derived/meg032/megr90n000fb.img
sha256sum assets/large_files/terrain/mars_mola_megr_32_v2.img
cargo run --features dem --bin mola_megr_convert -- \
  assets/large_files/terrain/mars_mola_megr_32_v2.img \
  assets/large_files/terrain/mars_mola_megr_32_cs2048_v1.csdem 2048
sha256sum assets/large_files/terrain/mars_mola_megr_32_cs2048_v1.csdem
```

Record the two resulting SHA-256 values here before treating the local CSDEM as
a reviewed terrain package.

Required tests include source byte order/offset, north-to-south row order,
east-positive antimeridian wrapping, pixel-center behavior, polar clamping,
CSDEM range rejection, and agreement between Mars render and collision samples
through `TerrainSource`.
