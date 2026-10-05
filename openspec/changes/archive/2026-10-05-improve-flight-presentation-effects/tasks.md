## 1. Audit And Quality Configuration

- [x] 1.1 Audit existing rocket presentation, interpolation, camera, lifecycle,
  effect hooks, assets, and mode composition; document the reusable authority
  path and the minimum presentation-only extension.

  Recorded in `design.md` under "Existing Capability Audit". The archived
  `premium-rocket-flight-presentation` change already provides engine plumes,
  ground interaction, pad lighting, audio controls, render-origin-stable
  children, and a distance-only quality resource. The uncovered gaps were
  quality levels, atmospheric shock/heating visuals, lifecycle transient
  feedback, and isolation tests.
- [x] 1.2 Add explicit effect-quality levels (full, reduced, disabled) to the
  existing Rocket presentation-quality resource, with a configurable startup
  override, without duplicating simulation state.

  `RocketEffectQualityLevel { Full, Reduced, Disabled }` added to
  `RocketPresentationQuality.level` (`components/mod.rs`); `from_env()` reads
  `COSMIC_SYSTEMS_ROCKET_EFFECT_QUALITY` and `RocketModePlugin` inserts it. The
  level gates only presentation work: `Reduced` hides secondary engine/ground/
  lifecycle layers, `Disabled` hides all effect children and skips spawn work.
  Pure parse/gate tests added.

## 2. Atmospheric And Lifecycle Feedback

- [x] 2.1 Add bounded atmospheric shock and heating visual feedback derived from
  the existing smoothed presentation parameters and authoritative flight
  conditions.

  New `atmospheric_effects.rs` reconciles two bounded rocket children from the
  already-smoothed `shock_intensity_unit`/`heating_intensity_unit`. It resamples
  no atmosphere/heating physics, gates visibility on finite intensity and camera
  distance, and uses a shared low-poly shell mesh with shock/heating materials.
- [x] 2.2 Add bounded transient lifecycle feedback for staging, fairing
  separation, touchdown, splashdown, and crash, driven by existing authoritative
  events.

  New `lifecycle_effects.rs` spawns short-lived puff/flash children from
  `StageSeparatedEvent`, `FairingSeparatedEvent`, `TouchdownEvent`,
  `SplashdownDetectedEvent`, and `CrashEvent`. A presentation-only timer grows
  and retires them; no visual timer is stored in simulation components.
- [x] 2.3 Reconcile new effect children with existing lifecycle/entity cleanup,
  render-origin-stable placement, and quality gating.

  Atmospheric shells self-heal when their owner is removed; transient effects
  are children of the affected vehicle and despawn with it or on expiry; both
  use rocket-local transforms so render-origin rebasing is inherited. Quality
  gating is applied at spawn and update.

## 3. Isolation And Validation

- [x] 3.1 Add pure tests proving presentation state cannot mutate authoritative
  rocket state, simulation time, or force inputs.

  `presentation_updates_leave_authoritative_state_untouched` runs the engine and
  atmospheric presentation systems for several frames and asserts throttle,
  active stage, propellant inventory, and `SimulationTime` are unchanged.
- [x] 3.2 Add lifecycle, entity-lifetime, render-origin, and quality-isolation
  regression coverage.

  Lifecycle: spawn+retire and disabled/reduced suppression tests. Render-origin:
  rocket-local shell transform test plus the existing plume transform test.
  Quality isolation: level parse/gate tests. Entity lifetime: owner-removal
  self-heal and despawn-with-owner behavior.
- [x] 3.3 Run formatting, compile, strict Clippy, tests, scientific validation,
  and bounded normal, craft, and rocket mode startup checks.

  `cargo fmt --check`, `cargo check --features dem`, `cargo clippy --all-targets
  --features dem -- -D warnings`, `cargo test --features dem` (827 passed, 2
  ignored), `cargo test --no-default-features` (797 passed, 2 ignored), release
  build, and bounded `default`/`craft`/`rocket` startup smoke runs (all exit 0,
  no panic) all pass.
