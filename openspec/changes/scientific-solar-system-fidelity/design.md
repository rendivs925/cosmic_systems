## Context

See proposal.md for motivation. The project already has an offline DE440s
manifest, shared f64 SSB/ICRF snapshots, a fixed simulation clock, and a
single reference-frame module. However, solar-map rotation still consumes a
separate display clock; body-fixed rotation is uniform spin plus catalog tilt;
gravity uses catalog mass with a point-mass Earth and only solar differential
gravity; and existing deterministic replays are internal regressions rather
than external validation.

## Goals / Non-Goals

**Goals:**
- Make epoch, body translation, orientation, GM, gravity tier, and validation
  provenance explicit and shared by normal, craft, and rocket modes.
- Preserve f64 SI authority through dynamics and restrict f32 to rebased
  presentation boundaries.
- Provide incremental fidelity tiers with quantitative validation rather than a
  one-time rewrite of the current flight pipeline.

**Non-Goals:**
- Real-time assimilation of live EOP, weather, tracking, or telemetry feeds.
- Immediate full-degree gravity fields, general relativity, tides, SRP, drag
  climatology, or every solar-system moon.
- Replacing presentation orbit ribbons, visual Sun enlargement, clouds, or
  terrain art with physics authority.
- Claiming a safety-critical or certified flight-dynamics product.

## Decisions

### 1. Epochs are a first-class domain value

Introduce an immutable epoch value that represents one physical instant and
offers explicit UTC, TAI, TT, TDB, and optional UT1 forms. `SimulationTime`
retains cadence, pause, and warp responsibilities, but advances this epoch only
after completed authoritative fixed ticks. Solar-map translation, rotation,
terrain/body-fixed conversion, rocket forces, and UI telemetry read that epoch.

The leap-second kernel/data source is local and versioned. Earth UT1 is derived
only when local EOP data covers the requested instant; no UTC-as-UT1 fallback is
allowed in reference-grade mode.

Alternatives considered:
- Keep separate visual and physical clocks: rejected because it invalidates
  translation/orientation/lighting comparisons.
- Derive TDB directly from elapsed seconds forever: retained only as a legacy
  J2000-start convenience; it cannot represent selectable civil dates.

### 2. Scientific datasets have explicit roles

The manifest evolves into a typed dataset contract: SPK provides translation,
LSK provides UTC/TAI conversion, PCK/BPC provides orientation, GM data provides
gravitational parameters, and EOP provides Earth UT1/polar-motion data. Runtime
loading and validation report which roles are active. A declared but unloaded
dataset is an error for the fidelity tier that requires it.

The current DE440s coverage remains bounded. The UI and APIs expose coverage
failure rather than extrapolating. Catalog bodies are classified as
kernel-backed, orientation-backed, or approximate; unsupported moons keep their
current parent-relative presentation model with explicit labels.

### 3. Orientation is separate from translation and presentation

Add a pure `BodyOrientation` authority that returns body-fixed/inertial rotation
and angular velocity at the shared epoch. It owns IAU pole/prime-meridian or
PCK/BPC evaluation. The reference-frame module remains the only coordinate
conversion boundary and consumes this authority instead of catalog tilt and
uniform period for high-fidelity bodies.

Earth additionally gains WGS-84 ECEF/geodetic conversion and Earth-orientation
data. Other bodies remain spherical until a named ellipsoid is approved. This
prevents silently treating catalog mean radii as an Earth geodetic datum.

### 4. Force models are named immutable configurations

Define a pure `ForceModelTier` selected at startup or through explicit scenario
configuration. Initial tiers are: `TwoBody`, `EarthJ2`, and `EarthMoonSun`.
Each combines common point-mass gravity with only its declared terms. Third-body
forces use the same-epoch differential formulation in the active
planet-centered inertial frame. GM and J2 values are dataset-provenanced.

Force accumulation stays in the existing fixed rocket pipeline. No system reads
render transforms or independently queries ephemerides. Future tides, higher
harmonics, SRP, and atmospheric models extend the tier rather than creating
parallel gravity implementations.

### 5. Long-arc propagation is an independent read-only service

Keep the existing high-rate fixed pipeline authoritative for powered flight,
contact, propellant, and control. Add a pure long-arc propagator for trajectory
prediction and coast/orbital scenarios, initially using deterministic
error-controlled DOP853 with a bounded maximum step and fixed tolerance policy.
It receives an immutable initial state and force-model configuration and never
mutates ECS state directly.

This is preferred over changing all integration to a symplectic method: the
rocket's non-conservative thrust, drag, staging, and contacts do not share a
single symplectic formulation. Its adaptive decisions must be deterministic for
the same configuration and state sequence; replay regression remains scoped to
the authoritative fixed pipeline.

### 6. External validation is a first-class release gate

Store recorded Horizons/SPICE reference cases with exact provenance, frames,
centers, and units. The release suite covers body states, orientation,
launch-site state, Sun direction, point-mass acceleration, and two-body
long-arc checkpoints. Acceptance budgets are scenario-specific and published
with each case. Independent J2 and Earth-Moon-Sun propagator comparisons are
explicitly deferred rather than simulated by internal test data.

Internal replay baselines continue to detect unintended changes; they do not
substitute for external truth. Kernel-dependent reference tests run in the
provisioned CI profile and are reported as unavailable, not passed, locally
when data is absent.

## Risks / Trade-offs

- [Data-source licensing, size, and coverage differ by kernel] → Pin source URL,
  checksum, coverage, and role in the manifest; provision only needed datasets.
- [Earth orientation data ages] → Version an EOP snapshot, state its validity
  range, and fail reference-grade requests outside that range.
- [Adaptive propagation threatens replay reproducibility] → Keep it read-only,
  deterministic for fixed inputs/configuration, and separate from fixed flight
  authority.
- [PCK/BPC support may differ in the existing Rust SPICE library] → Add a
  capability spike and only adopt a dependency or pure adapter after validating
  reference cases and local offline operation.
- [Fidelity work can regress interactive performance] → Evaluate snapshots once
  per epoch, select force tiers explicitly, profile each tier, and retain
  presentation decoupling.
- [External residuals reveal current baselines are inaccurate] → Preserve old
  replay fixtures, document intentional divergence, and never overwrite a
  baseline to conceal it.

## Migration Plan

1. Add epoch and provenance data in parallel with existing clocks; verify normal,
   craft, and rocket consumer agreement before removing legacy reads.
2. Load and validate dataset roles, then migrate body orientation and Earth
   geodesy behind explicit capability/tier configuration.
3. Introduce GM and force tiers with pure reference tests before scheduling them
   in the fixed rocket pipeline.
4. Add the read-only long-arc propagator and external checkpoint suite; publish
   error budgets before exposing high-fidelity scenario claims.
5. Remove superseded analytic primary, uniform-orientation, and catalog-GM
   runtime paths only after every active consumer has migrated.

Rollback is configuration-level: retain the prior validated tier and legacy
presentation markers while a new tier is disabled. Scientific authority data is
append-only and versioned; no user state migration is required.
