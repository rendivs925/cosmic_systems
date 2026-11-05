## 1. Implementation

- [x] 1.1 Pass the detail layer's negative bound into the removal-weight function and make the guard land-only with a continuous underwater fade.
- [x] 1.2 Enable the guard on Earth's detail layer with a window bounded by `1.5 * 36 m`.
- [x] 1.3 Add the worst-case non-crossing, deep-ocean, waterline-continuity and mesh-agreement tests.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, `cargo check` (both feature sets), `cargo clippy -D warnings`, and `cargo test` (both feature sets).
- [x] 2.2 Confirm `ascent`, `ascent-gravity-turn` and `leo-insertion-safe` remain within their committed baselines.
- [x] 2.3 Capture the Papua ground frame with the guard off and on at the same pose, time and frame count; measure the difference.

## 3. Documentation

- [x] 3.1 Record the before/after evidence, the measured ETOPO/sea-datum context and the remaining base-carved water as a separate task.

## 4. Cleanup

- [x] 4.1 Remove the temporary A/B build switch; the guard has no runtime toggle.
- [x] 4.2 Verify the tree is clean and push the change.
