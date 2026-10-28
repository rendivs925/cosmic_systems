# Earth EGM2008 Spherical Harmonics v1

The rocket force model gains an opt-in Earth spherical-harmonic tier backed by
measured EGM2008 coefficients. The default `planet-sun` tier is unchanged, so
no flight baseline, default trajectory, or existing regression moves.

## Source

- Model: Earth Gravitational Model 2008 (EGM2008), tide-free, fully normalized
  (4π) spherical-harmonic coefficients.
- Canonical distribution: National Geospatial-Intelligence Agency,
  <https://earth-info.nga.mil/index.php?dir=wgs84&action=egm2008>, spherical
  harmonics archive `php/download.php?file=egm-08spherical`
  (109,351,360 bytes, degree 2190).
- Distribution actually baked:
  <https://raw.githubusercontent.com/zcyphygeodesy/EGMallelementcalc/main/EGM2008.gfc>,
  a degree-362 EGM2008 subset with the same fully normalized coefficients.
  Verified source SHA-256:
  `4cc2b0e8824deb94407ffe9e396eac5df0cb5dc33d4207878a25eac604c0a701`
  (5,020,830 bytes).
- Model constants in the source header: GM `3.986004415e14 m3/s2`,
  reference radius `6,378,136.3 m`, tide system `tide_free`.
- Licence: public domain (NGA/DoD). Cite Pavlis, Holmes, Kenyon & Factor
  (2012), *The development and evaluation of the Earth Gravitational Model 2008
  (EGM2008)*, JGR 117, B04406.

The canonical NGA archive repeatedly reset this environment's connection after
about 23 MB, and it does not support byte ranges. The mirror was accepted only
after the low-degree coefficients were cross-checked against the established
degree-two extract: `C20 = -4.841651437908e-4` gives
`J2 = -sqrt(5) * C20 = 1.082626173852189e-3`, matching the checked-in
`egm2008_earth_j2.ron` to 3.4e-17. C22, S22, C30, and C40 also match the
published EGM2008 values. Replacing the mirror with the canonical degree-2190
file requires no format or code change.

## Package

`assets/large_files/gravity/egm2008_cs360_v1.csgravc`:

- Degree/order 360, 65,338 `(C̄, S̄)` pairs.
- 1,046,063 bytes: 64-byte header, 591-byte RON provenance, 1,045,408-byte
  payload.
- Package SHA-256:
  `02cf30ad28095e302ec51d53e7cd5d7a9185f1518b558596f26aa95ab46344ef`.
- Payload SHA-256:
  `2c96105b7ef59be485fd30a6878430dbad0ae49f6186fed90b5b9cecfe2383b2`.
- Format: `CSGRAV\0\0`, version `1`, little-endian `f64` pairs ordered
  `n = 2..=360`, `m = 0..=n`.

The header stores the model reference radius. The runtime deliberately keeps
the authoritative DE440 gravitational parameter from `gm_de440.tpc` rather than
the source-file GM, preserving one μ authority.

```sh
cargo run --release --features native --bin gravity_harmonics_bake -- \
  assets/large_files/gravity/raw/egm2008/egm2008_to362_tidefree.gfc \
  assets/large_files/gravity/egm2008_cs360_v1.csgravc 360
```

The converter parses the headerless `n m C S sigmaC sigmaS` distribution with
`D` exponents, verifies the triangular ordering, prints the implied J2, and
writes the versioned package. The simulator never reads the source text.

## Force model

```sh
cargo run -- rocket --site starbase --force-model earth-harmonics
```

`ForceModelTier::EarthHarmonics` selects point mass plus
`ForceModelTerm::EarthSphericalHarmonics`; it intentionally omits the lunar and
solar third-body terms so the tier stays a direct extension of `earth-j2`.
The package is loaded once by `MeasuredGravityHarmonics`. A missing or invalid
package is logged and the tier evaluates point mass only; the default
`planet-sun` tier never reads the package.

The field is evaluated in IAU body-fixed axes through the same kernel-backed
orientation snapshot used by terrain and orbital elements, then rotated back to
planet-centered inertial and solar-inertial axes. Unlike the axisymmetric J2
filter, tesseral terms require this full rotation.

## Validation

- `degree_two_matches_the_authoritative_j2_filter`: a degree-two package with
  only `C20` reproduces `earth_j2_acceleration` to `1e-12` relative at
  equatorial, mid-latitude, and near-polar positions.
- `general_harmonics_match_an_independent_potential_gradient`: a degree-four
  synthetic field with non-zero `C21..S44` matches an independent SciPy
  associated-Legendre potential gradient to `1e-8` (finite-difference limited).
- `earth_harmonics_tier_rotates_the_measured_field_through_the_snapshot`: the
  full ECS adapter path with an identity orientation reproduces the J2 authority
  in planet-centered inertial axes to `1e-12`.
- `earth_harmonics_tier_without_a_package_uses_point_mass`: absence degrades to
  the existing point-mass result.
- Package payload equals the degree-360 source rows exactly (65,338/65,338).
- Measured cost: about `0.33 ms` per degree-360 evaluation in a release build,
  roughly 4% of one core at 120 Hz for a single vehicle.

No physics baseline was regenerated: the default force-model tier and every
existing regression path are untouched.

## Limitations

- Degree 360 resolves features of roughly 100 km; the source extends to 2190 but
  the baked package keeps the reviewed 360-degree slice.
- Tide-free coefficients; no time-variable gravity, atmosphere, ocean tide, or
  solid-Earth tide model is applied.
- The source GM is recorded in the conversion log but the runtime uses DE440 μ.
- The package is local and gitignored, like terrain and climate packages. A
  checkout without it still flies the default tier; only the opt-in tier needs
  the bake step.

## Rollback

Remove `assets/large_files/gravity/egm2008_cs360_v1.csgravc` or run without
`--force-model earth-harmonics`. No code, baseline, or default configuration
change is required.
