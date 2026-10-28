# Mars GMM-3 Gravity Field v1

The repository now carries a validated, fully normalized Mars gravity package
in the same `.csgravc` format as the opt-in Earth EGM2008 field. The package is
**data readiness**: no current force-model tier selects Mars harmonics because
the simulator has no Mars flight mode, and no default behavior or physics
baseline changes.

## Source

- Model: Goddard Mars Model 3 (GMM-3), a static spherical-harmonic solution to
  degree and order 120 from MGS, Mars Odyssey, and MRO radio tracking.
- PDS product: `MRO-M-RSS-5-SDP-V1.0`, product `GMM3_120_SHA.TAB`, release
  2016-01-21.
- Source file:
  <https://pds-geosciences.wustl.edu/mro/mro-m-rss-5-sdp-v1/mrors_1xxx/data/shadr/gmm3_120_sha.tab>
- Source SHA-256:
  `eb4913b1afb6682406e6a9dad5be7918a162fa8462473c9a2e7aae258d4c2c9c`
  (900,604 bytes).
- Header: reference radius `3,396.0 km`, GM `42,828.372854 km3/s2`, degree and
  order `120`, normalization state `1` (normalized), reference longitude and
  latitude zero.
- Frame: the PDS label states that the reference system is Konopliv et al.
  (2006), explicitly **not** the IAU Mars system. The package records that
  caveat in its provenance; it must not be rotated with `pck00011.tpc` until a
  reviewed Mars gravity-frame conversion exists.
- Licence: public domain (NASA).
- Attribution: Genova et al. (2016), *A global, high-resolution gravity field
  of Mars from MGS, Mars Odyssey, and MRO radio tracking data*, Icarus 272,
  228-235; NASA/GSFC.

The 4π-normalized convention is confirmed by the degree-two term:
`C20 = -8.750211323545289e-4`, so `J2 = -sqrt(5) * C20 = 1.956606733693567e-3`,
consistent with the published Mars oblateness. The PDS label does not state a
tide system, so the package records the static field as published rather than
claiming tide-free or zero-tide.

## Package

- Path: `assets/large_files/gravity/gmm3_cs120_v1.csgravc`.
- Degree/order 120, 7,378 `(C̄, S̄)` pairs.
- 118,689 bytes: 64-byte header, 577-byte RON provenance, 118,048-byte payload.
- SHA-256:
  `1471af565e989c96fdd8615bfbdab9ef2785b890ac07fef0acc1cd26892e17f7`.
- Payload SHA-256 is validated by the shared `.csgravc` loader.
- The package payload equals the source rows for degrees 2 through 120 exactly.

```sh
python3 scripts/ingest_gmm3.py
cargo run --release --features native --bin shadr_gravity_bake -- \
  assets/large_files/gravity/raw/gmm3/gmm3_120_sha.tab \
  assets/large_files/gravity/gmm3_cs120_v1.csgravc gmm3 120
```

The model reference radius is the gravity field's `3,396 km`, not the
simulator's Mars catalog radius (`3,389.5 km`) used by terrain. The expansion
radius is internal to the force model and never becomes a terrain or render
authority.

## Validation

- Header parsing enforces the SHADR column order, normalization state, square
  degree/order, and exact triangular coefficient ordering.
- Payload comparison against the PDS rows: 7,378/7,378 coefficients identical.
- Degree-two term implies `J2 = 1.956606733693567e-3`; with the DE440 Mars GM
  and the model radius, the equatorial point-plus-J2 acceleration is about
  `3.703 m/s2`.
- The shared synthesizer already has degree-two/J2 and independent SciPy
  gradient tests; this package exercises its data path without introducing a
  new gravity implementation.
- The package is static: the source's seasonal C20/C30/C40/C50 variations are
  not stored.

No physics baseline was regenerated. No runtime system loads this package yet.

## Runtime status

`MeasuredGravityHarmonics` currently loads only the Earth package, and
`ForceModelTier::EarthHarmonics` is Earth-specific. Mars harmonics will be wired
when a Mars flight or propagation mode and a reviewed frame conversion exist;
until then this package is a reviewed data asset, not a hidden runtime input.

## Rollback

Delete `assets/large_files/gravity/gmm3_cs120_v1.csgravc`; nothing in the
runtime references it.
