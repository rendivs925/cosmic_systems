---
name: simulation-determinism-regression
description: Use when changing fixed simulation behaviour, SPICE/DE ephemerides, physics algorithms, replay, baselines, numerical integration, time acceleration, random generation, or simulation tests in Cosmic Systems.
---

# Simulation Determinism And Regression

Apply this skill whenever a change can affect authoritative simulation results.
Correct compilation is insufficient for physics changes.

## Existing Authorities

Start with these modules:

- `src/domain/services/simulation_time.rs`: fixed timestep, pause, and time acceleration.
- `src/domain/services/regression.rs`: sample comparison, baseline and tolerance policy.
- `src/infrastructure/bevy_adapters/rocket_replay.rs`: replay capture/restore and seek rules.
- `src/infrastructure/bevy_adapters/rocket_pipeline_tests.rs`: full fixed-pipeline tests.
- `src/domain/services/physics_orbital.rs`: current analytic ephemeris migration
  point until the kernel-backed authority replaces it.
- Pure domain services for gravity, orbital mechanics, rocket dynamics, propulsion,
  atmosphere, aerodynamics, terrain, and reference frames.

Do not create an unrelated replay format, simulation clock, random source, or
regression comparator.

## Determinism Contract

For identical initial state, configuration, inputs, seed, fixed timestep, and
ordered events, authoritative simulation must produce the same state sequence.

It must not depend on:

- render FPS, real-frame delta, wall-clock time, or camera state;
- accidental Bevy system order;
- unordered iteration where accumulation order matters;
- global/unseeded random state;
- cache hit/miss state or background task completion timing;
- GPU rendering state or asset load timing.

Presentation may interpolate or shed visual work. It must never change the
authoritative fixed state.

For kernel-backed ephemerides, identical results also require the same kernel
set, kernel hashes, NAIF body IDs, frame, center, and TDB conversion policy.
Treat that provenance as simulation input, not asset metadata.

## Time And Scheduling

- Authoritative physics runs in `FixedUpdate` using `SimulationTime`.
- Real time, simulation epoch, fixed timestep, and time acceleration remain
  distinct. Do not multiply individual equations by arbitrary warp settings.
- Explicitly order physically dependent systems with `RocketSet`/system sets.
- Make input a command captured in `Update` and consumed deterministically by the
  fixed pipeline.
- Background terrain/data work may alter readiness or presentation only; it may
  not make fixed physical results depend on completion order.
- A fixed-tick atmosphere sample is replaced as one complete value before its
  consumers run. Force and torque budgets are consumed exactly once by
  integration, so they cannot leak into a later tick.

## Regression Workflow

Before changing a physics algorithm:

1. State the expected physical improvement and numerical trade-off.
2. Identify the existing authoritative calculation and its users.
3. Add or update focused pure domain tests with known analytic/reference values.
4. Exercise an integration scenario through the existing fixed pipeline.
5. For ephemerides, compare recorded states with JPL Horizons at multiple
   epochs, including position and velocity tolerances in SI units.
6. Compare against the committed replay/baseline with explicit tolerances.
7. Document any intentional baseline divergence, kernel/version change, tick,
   variable, and reason.

Never rewrite working physical mathematics merely for sophistication. Do not
replace a baseline to hide an unexpected difference.

## Tests To Preserve

Choose the nearest relevant level:

- Domain: inverse-square gravity, energy/orbit properties, mass flow, torque,
  atmospheric density, aerodynamic force direction, terrain continuity.
- Frame/precision: round trips and large-scale precision boundaries.
- ECS pipeline: staged force accumulation, integration, ground contact, launch,
  recovery, and event ordering.
- Component authority: reserve-aware active-stage eligibility, atomic mass /
  inertia / center-of-mass refresh, complete flight-condition replacement, and
  cleared force/torque budgets after integration.
- Replay: capture/restore exactness, chronological capacity, lifecycle/seek
  restrictions, and bitwise-identical fresh runs.
- Regression: baseline signing, tolerances, exact divergence reporting, and
  expected trajectory hashes.
- Ephemeris: kernel provenance, TDB boundary cases, known body-center/frame
  states, and Horizons reference vectors across the supported date range.

Add a test for every bug fixed. Favor deterministic tests independent of a GPU,
window, render rate, and real time.

## Numerical Discipline

- Keep high-precision simulation in `f64`; isolate lossy presentation conversion.
- Check finite values and zero/singularity cases at domain boundaries.
- State tolerance units and rationale. Do not use broad tolerances without
  explaining the expected numerical error.
- Preserve conservation or monotonicity properties where the model requires
  them: mass budget, energy/momentum in appropriate scenarios, bounded controls,
  non-negative propellant, and normalized quaternions.

## Validation

Run at least:

```text
cargo fmt --check
cargo check --features dem
cargo clippy --features dem -- -D warnings
cargo test --features dem
```

For changes affecting mode/plugin setup, additionally bounded-start `cargo run`,
`cargo run -- craft`, and `cargo run -- rocket`. Report graphical-environment
limitations honestly. Reject a physics change with only visual/manual evidence.
