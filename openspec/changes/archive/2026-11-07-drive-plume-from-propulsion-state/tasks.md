## 1. Implementation

- [x] 1.1 Record burned propellant mass flow in `AppliedPropulsionState` from the existing force computation.
- [x] 1.2 Drive plume intensity from the measured mass-flow fraction instead of a running flag.
- [x] 1.3 Grow plume length with the pressure-derived expansion ratio.
- [x] 1.4 Point each plume along the gimbaled thrust axis using the force path's own helpers.
- [x] 1.5 Give recovering stages the diagnostic sample so their own burn drives their own plume.
- [x] 1.6 Add the capture-only effect log and the side tracking camera.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, `cargo check` (both feature sets), `cargo clippy -D warnings`, and `cargo test` (both feature sets).
- [x] 2.2 Confirm `ascent`, `ascent-gravity-turn` and `leo-insertion-safe` remain within their committed baselines.
- [x] 2.3 Capture sea-level and altitude burn frames and record the driving state.
- [x] 2.4 Measure effect count and update cost with `COSMIC_SYSTEMS_PERFORMANCE_METRICS=1`.

## 3. Documentation

- [x] 3.1 Record the mapping, the approximation, the capture controls and the remaining limits.

## 4. Cleanup

- [x] 4.1 Verify the tree is clean and push the change.
