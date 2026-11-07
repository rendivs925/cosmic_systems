## 1. Audit

- [x] 1.1 Confirm the global ETOPO package is stored ellipsoidal and the regional NASADEM ingest adds the geoid once.
- [x] 1.2 Confirm no runtime consumer adds or subtracts the datum a second time.

## 2. Tests

- [x] 2.1 Add a synthetic known-geoid test across source, mesh, classification, lake depth, and collision.
- [x] 2.2 Add a real Papua control-point test with the measured payload, platform floor, classification, mesh, and collision.

## 3. Validation

- [x] 3.1 Run `cargo fmt --check`, both `cargo check` sets, `cargo clippy -D warnings`, and both test sets.
- [x] 3.2 Confirm the three committed flight baselines are unchanged.

## 4. Documentation

- [x] 4.1 Record the control-point values, the rule, and the imagery exclusion.

## 5. Cleanup

- [x] 5.1 Verify the tree is clean and push the change.
