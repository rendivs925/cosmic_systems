# Deterministic Flight Baselines

These RON fixtures record authoritative f64 rocket state after fixed physics
updates. They are compared with the tolerances in `RegressionConfig`: 1 mm
position, 1 micrometer/s velocity, 1 microradian attitude, and 1 mg mass.

| Scenario | Initial conditions | Checkpoints | Purpose |
| --- | --- | --- | --- |
| `ascent` | Electron-like vehicle at the KSC terrain launch site, upright, zero velocity, launch guidance | Initial pad state through 256 fixed ticks (about 4 s) | Pins liftoff and throttle slew. |
| `ascent-gravity-turn` | Same canonical launch state as `ascent` | Initial state through 1,280 fixed ticks (20 s), including vertical-gate exit and pitch-over | Pins the full existing guidance, control, force, integration, and terrain-contact path through gravity-turn entry. |
| `leo-insertion-safe` | Existing Electron-like ascent harness seeded with the validated inclined circular `LowEarthOrbitTarget` state | Initial state, fixed-clock initialization, then the fixed tick that changes mission state to `Orbit` and cuts thrust | Pins the authoritative safe-insertion completion predicate. |

Record a reviewed fixture deliberately with:

```text
REGRESSION_TEST_FILTER=determinism_regression_tests::<test-name> \
  scripts/regression/save_baseline.sh
```

Review the fixture audit metadata, state samples, and hash chain before commit.

## 2026-11-10 launch-roll and RCS-authority update

`ascent-gravity-turn` and `leo-insertion-safe` were re-recorded after ascent
guidance was changed to preserve the vehicle's roll instead of commanding an
arbitrary shortest-arc roll at liftoff. The old target demanded a ~4.07 rad roll
slew while the engine was still off, which is why the RCS clamp had been set to
5e7 N·m. The default is now 5e3 N·m, a representative physical cluster scale.
`ascent` (the first 4 s, before the turn) is unchanged. No force, propulsion, or
integration equation changed; the attitude target and the actuator limit did.
The headless Starbase ascent still reaches insertion (200.85 x 199.24 km,
e = 0.00012).

## 2026-11-09 inclination-reference update

`leo-insertion-safe` was re-recorded after orbital inclination was corrected to
be measured about the physical rotation axis instead of the geometric pole. The
reflected solar frame makes those antiparallel for Earth, so the previous
recording described a circular orbit that was *retrograde* relative to the
planet's rotation while its inclination metadata said 28.5° prograde. The
initial position is unchanged; the velocity direction flips to the true
prograde sense, and the fixture still ends on the tick that changes mission state
to `Orbit` and cuts thrust. `ascent` and `ascent-gravity-turn` are unchanged.
No guidance, propulsion, force, or integration equation changed.

## 2026-10-21 terrain-alignment update

The three flight baselines were re-recorded after regional DEMs were aligned to
WGS 84 geodetic latitude. The legacy KSC reference surface moved by about 11.3 m
because the previous regional package was displaced by the planetocentric /
geodetic latitude mismatch. The launch-site delta and the re-recorded fixtures
were reviewed together; no guidance, propulsion, or integration equation changed.
