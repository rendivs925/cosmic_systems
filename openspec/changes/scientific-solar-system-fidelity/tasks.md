## 1. Scientific Dataset Contract

- [x] 1.1 Inventory every runtime celestial, orientation, GM, and time-data
  consumer; classify it as kernel-backed, approved approximation, or
  presentation-only.
- [x] 1.2 Extend the local scientific-data manifest with typed translation,
  leap-second, orientation, GM, and Earth-orientation roles, including source,
  checksum, coverage, frame, and time-scale provenance.
- [x] 1.3 Add offline provisioning and startup validation for every declared
  dataset role; report unavailable, invalid, and out-of-coverage roles without
  silent fallback.
- [x] 1.4 Add pure manifest and provenance regressions for missing, mismatched,
  and coverage-invalid scientific datasets.

## 2. Unified Epoch Authority

- [x] 2.1 Define pure epoch values and conversions for UTC, TAI, TT, TDB, and
  optional UT1 using the versioned local data sources.
- [x] 2.2 Extend `SimulationTime` to advance one authoritative epoch only after
  completed fixed ticks while preserving pause, warp, and bounded backlog
  behavior.
- [x] 2.3 Synchronize normal-mode, craft-mode, and rocket-mode scheduling with
  the unified epoch; remove independent physical-clock reads from celestial
  translation, rotation, lighting, and telemetry.
- [x] 2.4 Add cross-mode fixed-tick regressions proving identical shared TDB
  epochs and epoch-consistent translation, rotation, and Sun direction.
- [x] 2.5 Add civil-time and leap-second reference cases, plus explicit EOP
  coverage failure behavior for UT1 requests.

## 3. Body Orientation And Earth Geodesy

- [x] 3.1 Add a pure, versioned body-orientation authority returning
  inertial/body-fixed rotation and angular velocity at the shared epoch.
- [x] 3.2 Integrate approved PCK/BPC or equivalent IAU orientation data through
  the scientific dataset contract; keep unsupported body orientation explicitly
  approximate.
- [x] 3.3 Replace high-fidelity consumers of uniform catalog spin and axial tilt
  with the orientation authority while preserving presentation-only effects.
- [x] 3.4 Implement WGS-84 Earth geodetic/ECEF conversion and retain explicit
  spherical geodesy for bodies without an approved ellipsoid.
- [x] 3.5 Add orientation, Earth rotation angle, launch-site, Sun azimuth, and
  body-fixed round-trip reference regressions.

## 4. Ephemeris Coverage And Consumer Cleanup

- [x] 4.1 Expand shared snapshot coverage and catalog mappings for every body
  declared high fidelity, including required moon states and same-epoch relative
  state access.
- [x] 4.2 Migrate all remaining high-fidelity translation consumers to shared
  physical snapshots; preserve analytic paths only as labelled presentation
  approximations.
- [x] 4.3 Remove superseded analytic primary tables and numerical velocity
  derivatives after each active consumer and reference regression has migrated.
- [x] 4.4 Add multi-date, multi-body SSB and relative-state residual tests
  against recorded Horizons/SPICE values within documented DE440 budgets.

## 5. Gravity Fidelity Tiers

- [x] 5.1 Introduce a pure immutable force-model tier configuration and expose
  its active terms through telemetry and validation output.
- [x] 5.2 Replace high-fidelity catalog mass-times-G calculations with validated
  GM constants from the active scientific dataset while retaining labelled
  catalog fallback only where necessary.
- [x] 5.3 Implement and validate Earth J2 acceleration in the planet-centered
  inertial frame using the shared orientation authority.
- [x] 5.4 Generalize the existing solar tidal term to same-epoch, configured
  Moon and Sun differential third-body accelerations without duplicate gravity
  implementations.
- [x] 5.5 Add pure force, frame, and orbit regressions for point-mass, J2,
  Earth-Moon-Sun, and tier-selection behavior.

## 6. Long-Arc Propagation

- [x] 6.1 Define a pure read-only long-arc propagation API with initial state,
  epoch, force tier, integration settings, checkpoints, and result provenance.
- [x] 6.2 Implement deterministic bounded error-controlled long-arc integration
  without mutating the authoritative powered-flight/contact ECS state.
- [x] 6.3 Integrate the propagator into trajectory prediction and display only
  after its state, frame, and force-model boundaries are validated.
- [x] 6.4 Add scenario-specific LEO, J2-precessing, lunar-transfer, and escape
  checkpoint regressions with published position and velocity error budgets.

## 7. External Scientific Validation

- [x] 7.1 Define the versioned machine-readable reference-case format with
  source, command, kernel/data versions, time scale, frame, center, units, and
  tolerance metadata.
- [ ] 7.2 Record reproducible external cases for body state, orientation,
  launch-site state, Sun direction, gravity, and long-arc propagation.
- [x] 7.3 Implement an offline scientific-validation runner that reports each
  residual and fails against the case's published budget.
- [x] 7.4 Integrate provisioned scientific validation into CI separately from
  deterministic replay baselines and report unavailable reference datasets as
  unverified rather than passing.
- [x] 7.5 Extend physics-change audit records with affected external cases,
  intentional baseline divergence, numerical trade-offs, and acceptance output.

## 8. Release And Migration Validation

- [x] 8.1 Document fidelity tiers, active datasets, coordinate conventions,
  supported epochs, validity coverage, known approximations, and published
  scenario budgets.
- [x] 8.2 Run formatting, compile, strict clippy, deterministic replay, external
  reference, and full test validation after each completed phase.
- [x] 8.3 Validate bounded startup and scientific dataset reporting for normal,
  craft, and rocket modes; manually inspect presentation only after scientific
  checks pass.
