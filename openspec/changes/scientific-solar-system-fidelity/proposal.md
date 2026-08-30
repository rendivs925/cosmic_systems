## Why

DE440-backed primary translations have established a sound starting point, but
the simulator cannot yet make professional scientific-fidelity claims. Time,
body orientation, gravity, propagation, and external reference validation have
independent approximations that can produce mutually inconsistent states.

## What Changes

- Establish one selectable simulation epoch and explicit UTC, TAI, TT, TDB, and
  UT1 responsibilities for all modes.
- Complete kernel-backed scientific inputs: body states, orientation, and GM
  provenance for every body represented as high fidelity.
- Replace uniform catalog spin with validated body orientation models and add
  WGS-84 geodesy plus Earth orientation data where Earth flight requires it.
- Introduce explicit force-model tiers, beginning with validated GM, Earth J2,
  and ephemeris-driven lunar and solar third-body gravity.
- Separate high-rate contact/ascent integration from accurate long-arc orbit
  propagation, with documented error budgets.
- Add machine-readable external-reference cases and quantitative acceptance
  thresholds for ephemeris, frame, and two-body reference behavior; retain
  deterministic baselines as regression controls only.
- Keep visual scaling, Sun enlargement, static orbit markers, cloud motion, and
  other artistic effects explicitly presentation-only.

## Capabilities

### New Capabilities
- `simulation-epoch`: Selectable civil and dynamical epochs with one
  authoritative conversion path for all simulation consumers.
- `scientific-validation`: Versioned external reference cases, residual
  reporting, and acceptance thresholds for scientific models.

### Modified Capabilities
- `celestial-ephemerides`: Expand kernel-backed body coverage and provenance
  into the explicit ephemeris authority contract.
- `reference-frames`: Define validated body-orientation and Earth-geodesy
  transformations alongside existing inertial and render boundaries.
- `gravity`: Add declared fidelity tiers and ephemeris-derived gravitational
  parameters and third-body accelerations.
- `rocket-dynamics`: Define the propagation/integration boundary and published
  numerical error expectations for long-arc orbital motion.
- `determinism-regression`: Require external-truth validation to complement
  deterministic internal replay baselines.

## Impact

- Domain: `ephemeris`, `simulation_time`, `reference_frames`, `gravity`,
  `rocket_dynamics`, and orbital presentation services.
- Infrastructure: shared ephemeris plugin, fixed schedules, solar-map and
  rocket presentation adapters, replay/regression tooling.
- Assets and tooling: kernel manifest/provisioner, leap-second and Earth
  orientation data, recorded Horizons/SPICE reference datasets.
- Modes: normal solar, craft, and rocket retain shared scientific authority;
  existing presentation behavior remains non-authoritative.
- Deferred: independent external J2 and Earth-Moon-Sun long-arc propagator
  comparisons are not required by this change and may be proposed separately.
