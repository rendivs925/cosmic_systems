## 1. Implementation

- [x] 1.1 Add `TerrainSource::appearance_height_m` with a geometry-only boundary doc.
- [x] 1.2 Override it in `LayeredTerrainSource` with the ungraded composition.
- [x] 1.3 Sample the shared slope helper and the per-vertex surface map from it.
- [x] 1.4 Add gentle and steep grade tests, including the flat-classification control.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, both `cargo check` sets, `cargo clippy -D warnings`, and both test sets.
- [x] 2.2 Confirm the three committed flight baselines are unchanged.
- [x] 2.3 Capture a ground frame at the most rugged predefined site.

## 3. Documentation

- [x] 3.1 Record the rule, the elevation-band exception, and the measured result.

## 4. Cleanup

- [x] 4.1 Verify the tree is clean and push the change.
