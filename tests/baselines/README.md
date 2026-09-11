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
