## Context

See `proposal.md` for motivation. The live primary-body authority is the
analytic, heliocentric ecliptic evaluator in `physics_orbital.rs`; it supplies
solar-map state, Sun direction, rocket proxy placement, and solar differential
gravity. Moon states and planet orientation use separate analytic paths.

`SimulationTime` controls rocket fixed-step time, while solar-map presentation
currently derives an independent epoch from `Time<Fixed>` and
`SolarSystemParameters`. `reference_frames.rs` owns flight/render conversion,
but currently documents a Sun-centered display-axis physical frame.

## Goals / Non-Goals

**Goals:**

- Establish one local, reproducible body-state authority usable without a
  network in native and WASM builds.
- Preserve f64 state through physical calculations and make ICRF/J2000/TDB,
  NAIF IDs, centers, units, and coverage explicit.
- Migrate every current primary-body consumer in one coherent sequence.
- Replace the old runtime authority instead of adding a feature flag or fallback.

**Non-Goals:**

- Numerically integrate small bodies or spacecraft with a new N-body solver.
- Add an online Horizons client, automatic runtime kernel downloads, or a second
  floating-origin system.
- Claim high-precision Earth orientation outside the selected binary-PCK
  coverage, or supply unselected satellite kernels implicitly.

## Decisions

### Use ANISE with curated local SPICE assets

Use the Rust-native `anise` crate rather than CSPICE FFI. ANISE loads SPK, BPC,
and planetary-constant PCK data, supports TDB, validates translations against
SPICE, and avoids native ABI/toolchain risk for the project’s native and WASM
targets. CSPICE would be authoritative but adds build/link/asset-loading
complexity; writing a DAF/SPK reader would create unnecessary scientific risk.

The initial manifest pins a reviewed DE440 short SPK (`de440s.bsp`), planetary
constants (`pck00011.tpc` plus `gm_de440.tpc`), a leap-seconds kernel, and an
explicitly dated Earth binary PCK when high-precision Earth orientation is
needed. Satellite SPKs are listed as explicit later manifest extensions; they
are never substituted by the former Kepler tables.

Large kernel bytes remain outside Git under `assets/large_files/kernels/`. A
tracked manifest, checksums, source URLs, and provisioning command make the
offline runtime set reproducible. Application startup validates the local
manifest once and fails before evaluation if it is unavailable or incompatible.

### Separate pure domain evaluation from Bevy cache/update scheduling

The domain exposes typed NAIF target/center identifiers, a TDB epoch, immutable
kernel provenance, and a state result of `DVec3` meters/m/s. It has no Bevy
dependency beyond existing math types. A shared plugin owns loaded immutable
kernels and one evaluated-state snapshot per authoritative epoch. Systems update
the snapshot before all gravity, orbit, light, camera, and presentation
consumers; consumers do not open kernels or calculate their own state.

This avoids an ECS manager object while preventing per-mode kernel loading or
epoch drift. The snapshot includes state metadata so test failures identify the
epoch, center, frame, and manifest used.

### Make TDB epoch an explicit simulation-time conversion

The baseline is J2000 TDB plus completed simulated seconds. The conversion is
centralized and takes no wall-clock input. The normal/craft presentation clock
migrates onto this shared epoch, while time-warp controls become one
`SimulationTime` responsibility. Presentation interpolation may query a derived
fractional epoch but cannot change the authoritative fixed snapshot.

### Derive legacy frames at the conversion boundary

SPICE state is barycentric ICRF/J2000. The reference-frame service derives
heliocentric and planet-centered position and velocity by subtracting two
same-epoch states. It owns the documented ICRF-to-existing visual-axis
projection until presentation fully adopts an ICRF convention. Render-space
rebasing remains `PhysicalScale`/`RenderOrigin` responsibility.

Planetary orientation is a separate state contract: text PCK provides declared
IAU orientation where adequate, while an explicitly covered BPC supplies Earth
orientation. The old axial-tilt/spin formula is removed per body only when the
selected kernel provides its replacement.

### Migrate state consumers before deleting analytic implementation

First introduce pure loading/evaluation/provenance tests without rewiring
consumers. Then move the solar-map, craft target, rocket proxies, sunlight, and
solar gravity in one release path to the shared snapshot. Orbit ribbons become
sampled presentation geometry from the authority. Finally delete the hard-coded
primary table, numerical derivative, and migrated moon/orientation paths.

## Risks / Trade-offs

- [Kernel assets are large and coverage-bounded] → Keep bytes ignored, pin a
  manifest, validate at startup, and report coverage errors rather than fallback.
- [DE440 SPK and current visual axes differ] → Add recorded frame-conversion
  vectors and migrate presentation through one explicit adapter.
- [Earth orientation BPC has finite temporal coverage] → Pin a dated BPC and
  make its coverage visible; use declared PCK fidelity for other bodies.
- [ANISE API or supported kernel subset evolves] → Pin the crate version and
  validate selected kernels in CI with known states before accepting upgrades.
- [Existing modes use separate clocks] → Migrate shared epoch before consumers;
  retain no mode-local ephemeris cache.

## Migration Plan

1. Add the manifest/provisioner, ANISE dependency, typed domain contract, and
   deterministic state/coverage/provenance tests.
2. Register the shared kernel and evaluated-state resources in common plugin
   composition; map `SimulationTime` to TDB.
3. Migrate primary solar-map, craft, rocket proxy, Sun-light, camera, orbit, and
   gravity consumers to the shared state snapshot and validate all modes.
4. Add selected satellite SPKs and BPC/PCK orientation migration, then remove
   the matching analytic moon/spin implementations.
5. Remove the primary secular table and numerical velocity derivative. A failed
   rollout restores the prior application revision and its kernel manifest; no
   runtime fallback exists within a revision.

## Open Questions

- The initial satellite-kernel subset and supported historical/future coverage
  window will be selected from the catalog before satellite migration; this does
  not affect the initial primary DE440 state contract.
