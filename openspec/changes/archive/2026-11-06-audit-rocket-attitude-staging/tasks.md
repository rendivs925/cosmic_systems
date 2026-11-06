## 1. Implementation

- [x] 1.1 Make the mission state optional in the render snapshot and keep the surface-constrained pin for actual mission vehicles.
- [x] 1.2 Add a body-origin shift to the mesh builder and build the serial-stage mesh in the stage's own frame.
- [x] 1.3 Add attitude-preservation assertions for serial separation and parallel booster jettison.
- [x] 1.4 Add the measurement-only capture controls for body axes and a deterministic warm start.

## 2. Validation

- [x] 2.1 Run `cargo fmt --check`, `cargo check` (both feature sets), `cargo clippy -D warnings`, and `cargo test` (both feature sets).
- [x] 2.2 Confirm `ascent`, `ascent-gravity-turn` and `leo-insertion-safe` remain within their committed baselines.
- [x] 2.3 Capture prelaunch and ascent/staging frames with the body-axis overlay and record the sim time reached.

## 3. Documentation

- [x] 3.1 Record the audit findings, the two fixes, the capture controls and the remaining limits.

## 4. Cleanup

- [x] 4.1 Verify the tree is clean and push the change.
