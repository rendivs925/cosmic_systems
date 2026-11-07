## 1. Implementation

- [x] 1.1 Classify ocean-cap vertices at the sea datum, matching `surface_class`.
- [x] 1.2 Reduce the depth-stability surface offset from 0.5 m to 0.05 m.
- [x] 1.3 Record the Papua composition (measured, sea, eroded, composed, class) in a data-backed test.
- [x] 1.4 Add a render regression: land above the datum gets no cap; a bed at or below it does.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, `cargo check` (both feature sets), `cargo clippy -D warnings`, and `cargo test` (both feature sets).
- [x] 2.2 Confirm `ascent`, `ascent-gravity-turn` and `leo-insertion-safe` remain within their committed baselines.
- [x] 2.3 Repeat the TD-06 Papua pose and measure the pale near-neutral pixel share.
- [x] 2.4 Check a 300 km ocean/coast frame for far-field globe z-fighting after the offset change.

## 3. Documentation

- [x] 3.1 Record the determination, the classification fix, the measurement, and the residual data-limited strip.

## 4. Cleanup

- [x] 4.1 Verify the tree is clean and push the change.
