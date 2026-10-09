## 1. Terrain material frame

- [x] 1.1 Compare the full derived material frame (anchors + planet center + rotation) before skipping a refresh in `terrain/render.rs`
- [x] 1.2 Add a test that an anchor change forces a refresh and an unchanged frame does not churn
- [x] 1.3 Add a test proving body-fixed phase invariance under origin rebase and planetary rotation
- [x] 1.4 Verify layer/imagery readiness keeps the geographic base continuous in the shader

## 2. Coarse vegetation

- [x] 2.1 Remove `coarse_vegetation_scale` and its call sites
- [x] 2.2 Keep coarse tree-only patches physically sized within the coarse budget
- [x] 2.3 Add tests: coarse trees are not enlarged; coarse geometry does not exceed finer; refinement keeps cover
- [x] 2.4 Update the existing vegetation threshold/tests for the new contract

## 3. Residency accounting

- [x] 3.1 Include coarse-vegetation bytes in `estimated_patch_bytes` using the same budget the scatter path uses
- [x] 3.2 Update budget/viewport tests so the estimate matches generation

## 4. Camera, shadows, tower

- [x] 4.1 Add tower integration tests over spawned meshes (attachment, thickness, orientation)
- [x] 4.2 Exercise camera clearance across patch replacement, origin rebase, and mode transitions; capture stationary and moving shadow sequences (automated tests added and mutation-checked; rendered stationary + camera-transition sequences captured and inspected)
- [x] 4.3 Fix only shadow causes identified by the captures; otherwise record the result (no shadow defect identified; residual frame difference is streaming/animated-cloud motion, recorded in the repair notes)

## 5. Measurement and optimization

- [x] 5.1 Re-baseline release with fixed camera/epoch/resolution/quality against a recoverable pre-repair state, separate startup from settled residency
- [x] 5.2 Profile CPU extraction/preparation, worker contention, all GPU passes, and resident asset growth
- [x] 5.3 Apply the smallest profile-supported fix; re-measure the same scenario (disabling 90% of worker CPU did not change frame time; all instrumented GPU passes total ~2.8 ms; frame cost is resolution-dependent. No terrain/erosion/shader fix was justified — the remaining cost is an un-instrumented present/copy path, out of scope.)
- [x] 5.4 Record measured before/after and remaining bottlenecks in `docs/ground_tower_vegetation_repair.md`

## 6. Validation

- [x] 6.1 `cargo fmt --check`, `cargo check --features dem`, `cargo clippy --features dem --all-targets`
- [x] 6.2 `cargo test --features dem` and `cargo test --no-default-features --lib`
- [x] 6.3 `cargo build --release --features dem` and start normal/craft/rocket modes
- [x] 6.4 `openspec validate --all --strict`
