---
name: solar-ephemeris-authority
description: Use when adding or changing SPICE, DE440, DE441, NAIF kernels, JPL Horizons validation, planetary or lunar ephemerides, barycentric body states, or scientific solar-system time in Cosmic Systems.
---

# Solar Ephemeris Authority

Use this skill for scientific solar-system state. Known bodies have one runtime
authority: a curated local NAIF SPICE kernel set backed by a JPL DE ephemeris.
Do not add a second analytic, N-body, rendering, or network-derived body-state
path.

## Target Authority

The authoritative state contract is:

```text
input:  NAIF target ID, NAIF center ID, TDB epoch
output: position_m and velocity_mps as f64/DVec3
frame:  ICRF/J2000 barycentric unless an explicit derived frame is requested
data:   versioned local SPK + PCK + required time kernels
```

The kernel-backed service owns state evaluation. `reference_frames.rs` owns
conversion to heliocentric, planet-centered, body-fixed, local tangent, and
camera-relative frames. Rendering, gravity, telemetry, orbit ribbons, and
camera systems consume evaluated state; they never load kernels or reconstruct
orbits independently.

## Required Audit

Before implementation, inspect:

- `Cargo.toml` and `Cargo.lock` for existing SPICE support and dependency policy;
- `src/domain/services/physics_orbital.rs` for the analytic migration seam;
- `src/domain/services/reference_frames.rs` for frame/precision conversion;
- `src/domain/services/simulation_time.rs` for the simulation epoch;
- `src/infrastructure/plugins/mod.rs` for shared startup and system ordering;
- current solar-map, rocket-proxy, lighting, orbit, and camera consumers.

State the current authority, every consumer, the proposed kernel files and
versions, NAIF IDs, frame, center, time conversion, migration order, disk-size
and licensing implications, and the exact deletion plan for superseded runtime
models.

## Kernel Policy

- Ship or provision one reviewed, versioned kernel manifest. Record file names,
  source URLs, checksums, NAIF IDs, coverage range, frame, center, units, and
  license/provenance.
- Runtime must be offline and deterministic. JPL Horizons is a validation source,
  never a runtime API or fallback.
- Validate coverage and checksums at startup. Fail clearly when the selected
  epoch/body/frame is unsupported; never silently substitute analytic data.
- Load kernels once through shared composition. Kernel loading, metadata, and
  evaluated body state must not be duplicated per application mode or consumer.
- Keep binary assets out of commits unless project policy explicitly permits
  them. Prefer a documented manifest and reproducible provisioning command.

## Time And Frame Discipline

- `SimulationTime` supplies elapsed simulation seconds. Convert it once to the
  documented TDB epoch; do not use wall-clock time.
- Preserve SPICE state positions and velocities as f64 SI values. Convert SPK
  kilometres/kilometres-per-second explicitly at the kernel boundary.
- State target, observer/center, frame, aberration policy, epoch scale, and
  units in every public ephemeris API name or documentation.
- Derive Sun-centered or planet-centered values by subtracting two states at
  the same epoch. Convert both position and velocity.
- Planet orientation comes from the approved PCK path. Do not combine a DE
  position with the old axial-tilt/spin approximation after migration.

## Migration Rules

1. Add the kernel-backed domain service and pure state tests without changing
   consumers.
2. Migrate primary-body state consumers together: solar-map positions, Sun
   direction, flight proxies, lighting, camera targets, and orbit paths.
3. Migrate lunar and major-moon consumers using the same authority.
4. Migrate gravity and other physical consumers only after state/frame tests
   establish the intended center and differential-force formulation.
5. Remove the analytic runtime ephemeris path and its call sites. Retain only
   explicit migration/reference tests if they add scientific value.

Do not leave feature flags, fallbacks, or per-mode ephemeris choices that make
two body-state authorities possible at runtime.

## Validation

Add deterministic tests for:

- startup validation of manifest, checksum, kernel coverage, and NAIF IDs;
- Sun, Earth-Moon barycenter, and at least one outer planet at J2000 and
  separated epochs within declared Horizons-derived tolerances;
- position and velocity units, frame, center, and finite values;
- barycentric-to-heliocentric and barycentric-to-planet-centered derived states;
- body orientation at recorded epochs;
- identical evaluated states across repeated calls and all application modes.

Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`, then
bounded startup validation for normal, craft, and rocket modes. Report any
headless graphics limitation separately from ephemeris validation.

Reject online runtime queries, f32 ephemeris authority, render transforms as
state, undocumented kernel provenance, silent coverage fallback, duplicate body
caches, and mixing epochs across a single presentation or physics tick.
