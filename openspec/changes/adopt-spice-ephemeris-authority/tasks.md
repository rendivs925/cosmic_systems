## 1. Kernel Contract And Provisioning

- [x] 1.1 Add the pinned Rust-native SPICE dependency and typed NAIF target,
  center, TDB epoch, provenance, coverage, and state-vector domain contract.
- [x] 1.2 Add the tracked DE440 kernel manifest, checksum validation, and
  reproducible offline provisioning command while keeping kernel bytes ignored.
- [x] 1.3 Add deterministic manifest, missing-kernel, coverage, units, and
  recorded Horizons-vector domain regressions.

## 2. Shared Scientific Authority

- [x] 2.1 Implement a pure kernel-backed body-state service returning
  barycentric ICRF/J2000 f64 SI position and velocity.
- [x] 2.2 Centralize `SimulationTime` to TDB epoch conversion and derive
  same-epoch heliocentric and planet-centered states in reference frames.
- [x] 2.3 Register immutable kernel provenance and evaluated state once in shared
  Bevy composition with defined fixed/update ordering for all modes.

## 3. Consumer Migration

- [ ] 3.1 Migrate solar-map transforms, render rebasing, and sampled orbit
  presentation to the shared evaluated state.
- [ ] 3.2 Migrate craft targets, rocket planet/moon proxies, and Sun lighting to
  the shared evaluated state and shared TDB epoch.
- [ ] 3.3 Migrate solar differential gravity to same-epoch kernel-derived states
  and replace migrated planet orientation with approved PCK/BPC rotation data.

## 4. Authority Cleanup And Validation

- [ ] 4.1 Remove the analytic primary JPL table, numerical velocity derivative,
  and superseded primary/moon/orientation runtime paths without a fallback.
- [ ] 4.2 Add cross-mode deterministic state regressions and recorded
  frame/position/velocity/orientation reference cases.
- [ ] 4.3 Run OpenSpec validation, formatting, checks, clippy, tests, and bounded
  startup validation for normal, craft, and rocket modes.
