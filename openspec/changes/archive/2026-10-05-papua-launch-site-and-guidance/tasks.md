## 1. Drop KSC-scoped work

- [x] 1.1 Delete `openspec/changes/ksc-data-backed-terrain/` and remove
  `assets/configs/terrain/earth_ksc_3dep_v1.ron`.
- [x] 1.2 Remove the local-elevation machinery: `local_elevation.rs`,
  `local_elevation_convert.rs`, the `local_elevation_convert` bin in
  `Cargo.toml`, and `LocalElevationOverlayTerrainSource` /
  `with_dem_and_local_elevation_paths` in `terrain_source/catalog.rs`, with
  their tests.
- [x] 1.3 Confirm no remaining references to the removed module, bin, or KSC
  config compile or are loaded.

## 2. Launch-site-derived inclination

- [x] 2.1 Add `RocketAutopilot::for_launch_site(latitude_deg)` setting
  `target_orbit.target_inclination_rad` to `|latitude|`, and remove the
  redundant, never-read `AscentGuidanceProfile::target_inclination_rad`.
- [x] 2.2 Use it in the rocket spawn path from `setup.launch_site` instead of
  `RocketAutopilot::default()`.
- [x] 2.3 Correct the KSC-latitude comment on `LowEarthOrbitTarget` and update
  tests that reference the old value.
- [x] 2.4 Add regression tests for the launch-site-derived target inclination
  and the near-due-east ascending heading for the 8-degree-south Papua site.

## 3. Procedural terrain confirmation

- [x] 3.1 Confirm the launch site continues to sample the global/procedural
  Earth elevation source and that removing the overlay does not change default
  terrain samples.

## 4. Launch-site presentation

- [x] 4.1 Extend the anchored procedural pad with a higher-quality service
  tower and supporting structures, keeping all children presentation-only.
- [x] 4.2 Bound structure detail, draw distance, and light count behind the
  existing proximity/quality gating.
- [x] 4.3 Add tests for geodetic anchoring, absence of collision/physics
  contribution, and quality-bounded visibility.

## 5. Validation

- [x] 5.1 Run `cargo fmt --check`, `cargo check/test --features dem`,
  `cargo check/test --no-default-features`, `cargo clippy --all-targets
  --features dem -- -D warnings`, and a release build.
- [x] 5.2 Run bounded smoke tests for `cargo run`, `cargo run -- craft`, and
  `cargo run -- rocket`, and capture a native Papua-pad view.
- [x] 5.3 Run `openspec validate --all --strict` and `openspec doctor`.
