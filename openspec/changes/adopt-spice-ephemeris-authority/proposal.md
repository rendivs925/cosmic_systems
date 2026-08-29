## Why

The simulator currently derives primary-body positions from a limited analytic
secular-element table and derives velocity numerically. That model is not a
single scientific authority for planets, moons, orientation, gravity, lighting,
and flight frames. A local SPICE/DE kernel authority is required before the
solar-system simulation can support trustworthy observation and navigation.

## What Changes

- Add an offline, versioned NAIF kernel manifest and deterministic provisioning
  path for the selected JPL DE SPK, planetary constants/orientation kernels, and
  required time data.
- Add one shared Rust-native SPICE state service that evaluates NAIF body states
  in barycentric ICRF/J2000 at a TDB epoch and returns f64 SI position and
  velocity.
- Define the conversion from the existing simulation epoch to TDB and unify all
  solar-system consumers on that evaluated epoch.
- Migrate solar-map transforms, flight proxies, sunlight, camera targets, orbit
  presentation, and solar differential gravity to the shared state authority.
- Replace the current primary analytic JPL-table runtime path and numerical
  velocity derivative. Replace moon and orientation approximations where the
  approved kernel coverage supplies those bodies/rotations.
- Add kernel provenance, coverage, state-vector, frame, and cross-mode
  deterministic regressions using recorded JPL Horizons references.
- **BREAKING**: Public ephemeris and reference-frame contracts move from
  heliocentric ecliptic AU/AU-day presentation state to barycentric ICRF/J2000
  f64 SI state. Presentation-only display coordinates become derived values.

## Capabilities

### New Capabilities
- `spice-ephemeris-authority`: Offline kernel provisioning, provenance
  validation, TDB state evaluation, and shared scientific body-state access.

### Modified Capabilities
- `celestial-ephemerides`: Replace the analytic secular primary-body authority
  with a kernel-backed barycentric SPICE/DE authority.
- `reference-frames`: Define barycentric ICRF/J2000 as the physical solar-system
  authority and constrain all derived flight and render-frame conversions.
- `gravity`: Require solar differential gravity to consume the shared
  kernel-backed state rather than analytic heliocentric state.

## Impact

- Domain services: `physics_orbital.rs`, `reference_frames.rs`,
  `simulation_time.rs`, celestial identifiers, and new ephemeris data types.
- Application/infrastructure: shared plugin composition, solar-map systems,
  rocket proxies, lighting, camera/orbit presentation, and gravity consumers.
- Dependencies and assets: add the Rust-native `anise` toolkit plus a reviewed,
  non-committed local kernel set and checksum manifest.
- Validation: recorded JPL Horizons vectors, kernel/hash/coverage checks, full
  Rust validation, and bounded startup checks for normal, craft, and rocket
  modes.
