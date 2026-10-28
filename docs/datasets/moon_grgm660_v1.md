# Moon GRGM660PRIM Gravity Field v1

The repository carries a validated, fully normalized lunar gravity package in
the shared `.csgravc` format. Like the Mars GMM-3 package, it is **data
readiness**: no current force-model tier loads it because there is no Moon
flight mode, and no default behavior or physics baseline changes.

## Source

- Model: GRGM660PRIM, a degree-and-order-660 static spherical-harmonic solution
  from GRAIL primary-mission Ka-band range-rate and DSN tracking data.
- PDS product: `GRAIL-L-LGRS-5-RDR-V1.0`, product `GGGRX_0660PM_SHA.TAB`,
  release 2013-09-04, data span 2012-03-01 to 2012-05-29.
- Source file:
  <https://pds-geosciences.wustl.edu/grail/grail-l-lgrs-5-rdr-v1/grail_1001/shadr/gggrx_0660pm_sha.tab>
- Source SHA-256:
  `a77ee9996aff7341ef89f69aa401f39ba0c2949ed80411505fb4826b771dd63e`
  (26,692,624 bytes).
- Header: reference radius `1,738.0 km`, GM `4,902.79980693169 km3/s2`, degree
  and order `660`, normalization state `1` (normalized), reference longitude
  and latitude zero.
- Licence: public domain (NASA).
- Attribution: Lemoine et al. (2013), *High-degree gravity models from GRAIL
  primary mission data*, JGR Planets 118, 1676-1698,
  doi:10.1002/jgre.20118; NASA/GSFC GRAIL.

The coefficients are fully normalized. `C20 = -9.088292365077100e-5` gives
`J2 = -sqrt(5) * C20 = 2.032203952770473e-4`. With the DE440 lunar GM and the
model radius, the equatorial point-plus-J2 acceleration is about
`1.6226 m/s2`. A Kaula-type constraint was applied above degree 330 in the
source solution.

## Frame caveat

The PDS label states that the planetary ephemeris is **DE421**, which defines
the lunar body-fixed coordinate system in the **principal-axes frame**. This is
not the IAU/pck frame used by the simulator's Moon terrain and orientation.
The package records the DE421 principal-axes provenance and must not be rotated
with `pck00011.tpc` for force evaluation until a reviewed frame conversion is
implemented. It is therefore not loaded by `MeasuredGravityHarmonics`.

The label does not state a tide system; the package records the static field as
published rather than claiming tide-free or zero-tide.

## Package

- Path: `assets/large_files/gravity/grgm660_cs660_v1.csgravc`.
- Degree/order 660, 218,788 `(C̄, S̄)` pairs.
- 3,501,337 bytes: 64-byte header, 665-byte RON provenance, 3,500,608-byte
  payload.
- SHA-256:
  `2cfda21344426bd17b4665cbf7e05c4084b693637f5ad6afca0b339c7483b2f3`.
- Payload SHA-256 is validated by the shared `.csgravc` loader.
- Payload equals the source rows for degrees 2 through 660 exactly.

```sh
python3 scripts/ingest_grgm.py
cargo run --release --features native --bin shadr_gravity_bake -- \
  assets/large_files/gravity/raw/grgm/gggrx_0660pm_sha.tab \
  assets/large_files/gravity/grgm660_cs660_v1.csgravc grgm660 660
```

The model radius (`1,738 km`) is the expansion radius, not a terrain radius.
The simulator's Moon terrain authority remains LOLA LDEM_128, and the two
radii are not interchangeable.

## Validation

- SHADR header parsing enforces column order, normalization state, square
  degree/order, and exact triangular coefficient ordering.
- Payload comparison against the PDS rows: 218,788/218,788 coefficients
  identical.
- Degree-two term implies `J2 = 2.032203952770473e-4`; equatorial point-plus-J2
  acceleration is about `1.6226 m/s2`.
- The shared synthesizer already has degree-two/J2 and independent SciPy
  gradient tests; this package adds data, not a second gravity implementation.

No physics baseline was regenerated. No runtime system loads this package yet.

## Rollback

Delete `assets/large_files/gravity/grgm660_cs660_v1.csgravc`; nothing in the
runtime references it.
