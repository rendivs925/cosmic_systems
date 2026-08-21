## 1. Domain integration math

- [x] 1.1 Add `domain/services/rocket_dynamics.rs` with translational integration (semi-implicit Euler) in f64
- [x] 1.2 Add rotational integration: torque → angular acceleration → angular velocity → normalized quaternion via `Quat::from_scaled_axis`
- [x] 1.3 Add inertia tensor computation from stage geometry and center-of-mass update on mass change
- [x] 1.4 Unit tests: zero-torque stability, quaternion validity/normalization, torque about principal axis, mass-change behavior

## 2. Physical state component

- [x] 2.1 Extend `RocketComponent` with f64 physical state, inertia tensor, center of mass, and angular acceleration (keep existing fields as compatible facade)
- [x] 2.2 Update inertia tensor and center of mass when fuel mass changes

## 3. System pipeline

- [x] 3.1 Add `accumulate_forces`, `accumulate_torques`, `integrate_6dof`, and `sync_render_transform` systems
- [x] 3.2 Chain them explicitly (ordered systems) in the rocket mode plugin
- [x] 3.3 Replace the old `update_rocket_physics` invalid integration and remove the `Quat::from_vec4` step
- [x] 3.4 Ensure no other system writes the rocket `Transform` directly

## 4. Validation

- [x] 4.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [x] 4.2 Confirm craft mode is unaffected