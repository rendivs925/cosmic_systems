## 1. Implementation

- [x] 1.1 Add the two bounded fill constants (`k_max`, scale height) to `rendering/sky.rs` and the matching `SkyParams` fields, with a zero fill for vacuum bodies.
- [x] 1.2 Add the luminance-preserving fill to `sky_radiance_direction` in `assets/shaders/sky_radiance.wgsl`, using the observer altitude from `position`.
- [x] 1.3 Update the Rust/WGSL field-order test and add the energy-bound and monotonicity tests.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, `cargo check` (both feature sets), `cargo clippy -D warnings`, and `cargo test` (both feature sets).
- [x] 2.2 Repeat the five `sky2` frames (10/30/70/200/400 km, +2° pitch, 6° FOV) and the two `fov6` limb frames at the same pose, time and frame count.
- [x] 2.3 Measure the +2° horizon saturation and the 24–32 km linear-light limb e-folding; confirm 8 ± 1 km.
- [x] 2.4 Record the before/after evidence, the approximation and its bound in the roadmap, dashboard, README, visual-evidence registry and audit plan.

## 3. Cleanup

- [x] 3.1 Remove any temporary probe or capture switch used only for measurement.
- [x] 3.2 Verify the tree is clean and push the change.
