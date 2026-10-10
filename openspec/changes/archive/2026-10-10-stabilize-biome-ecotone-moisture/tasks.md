## 1. Wetness signal

- [x] 1.1 Add `MOISTURE_NOISE_SCALE` and make `ProceduralDetailSource::moisture`
      return a unit-sphere fBm field instead of the drainage statistic.
- [x] 1.2 Remove the now-unused `drainage_strength` wrapper; keep
      `drainage_strength_for_direction` for geometry.
- [x] 1.3 Document the scale, the reason, and that geometric drainage is unchanged.

## 2. Regression coverage

- [x] 2.1 Add `earth_launch_site_weight_map_has_no_high_frequency_alias` for both
      launch sites, bounding coarse texel steps and the moisture span.
- [x] 2.2 Add `earth_launch_site_weight_maps_stay_continuous_across_lods`,
      comparing each parent/child pair against the bilinearly reconstructed coarse
      map at both sites.
- [x] 2.3 Point `real_earth_adjacent_patch_weight_maps_agree_on_shared_edges` at
      the Papua launch site and move test imports to the module top.
- [x] 2.4 Mutation-check: raising `MOISTURE_NOISE_SCALE` to the drainage frequency
      makes the continuity test fail.

## 3. Evidence

- [x] 3.1 Capture a matched before/after A/B at the fixed prelaunch camera from two
      release binaries differing only in the moisture signal.
- [x] 3.2 Capture a wider orbital view of the fixed binary to check additional LODs.
- [x] 3.3 Store the evidence and record the method in
      `docs/ground_tower_vegetation_repair.md`.

## 4. Validation

- [x] 4.1 `cargo fmt --check`, `cargo clippy --features dem --all-targets`.
- [x] 4.2 `cargo test --features dem`, `cargo test --no-default-features --lib`.
- [x] 4.3 Release build and smoke-run solar, craft, and rocket modes.
- [x] 4.4 `openspec validate --all --strict`.
