## 1. Audit

- [x] 1.1 Find the land/sea, species and treeline inputs.
- [x] 1.2 Confirm the stable lattice and far-field density change is complete.

## 2. Implementation

- [x] 2.1 Use height above the local sea datum in the tree gate and species call.
- [x] 2.2 Use the same datum for the treeline.
- [x] 2.3 Add `VEGETATION_GENERATOR_VERSION`.

## 3. Tests

- [x] 3.1 Real Papua coastal patch: every accepted tree is at least 0.5 m above the datum.
- [x] 3.2 Synthetic treeline fades density to zero.
- [x] 3.3 Identity pins the version, seed, salt and lattice level.
- [x] 3.4 Lattice nesting and determinism (already present from the stable-lattice change).

## 4. Validation

- [x] 4.1 Run `cargo fmt --check`, both `cargo check` sets, `cargo clippy -D warnings`, and both test sets.
- [x] 4.2 Confirm the three committed flight baselines are unchanged.
- [x] 4.3 Capture ground, 3 km and 40 km frames.
- [x] 4.4 Archive `stabilize-far-field-tree-placement`.

## 5. Documentation

- [x] 5.1 Record the datum defect, the fix, the identity and the sparse-look limit.

## 6. Cleanup

- [x] 6.1 Verify the tree is clean and push the change.
