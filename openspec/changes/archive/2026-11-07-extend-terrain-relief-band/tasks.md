## 1. Implementation

- [x] 1.1 Add the deterministic coarse relief source with launch-site attenuation.
- [x] 1.2 Add the band-pass mesh-only layer and wire it into Earth.
- [x] 1.3 Weight it to land above the sea-datum guard.

## 2. Tests

- [x] 2.1 Band weight monotonic into and out of the centre; planet-scale and fine LODs zero.
- [x] 2.2 Physical height unchanged; sea guard and high-land cases.
- [x] 2.3 Earth composition: fine LOD matches collision, coarse LOD differs.

## 3. Validation

- [x] 3.1 Run `cargo fmt --check`, both `cargo check` sets, `cargo clippy -D warnings`, and both test sets.
- [x] 3.2 Confirm the three committed flight baselines are unchanged.
- [x] 3.3 Capture 3 km, 40 km (normal and height diagnostic) and 300 km frames.
- [x] 3.4 Record resident tiles, memory, main-thread and batch cost.

## 4. Documentation

- [x] 4.1 Record the band, its limits, and the performance numbers.

## 5. Cleanup

- [x] 5.1 Verify the tree is clean and push the change.
