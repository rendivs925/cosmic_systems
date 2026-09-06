---
name: rocket-flight-simulation
description: Use when changing rocket physics, propulsion, staging, guidance, control, aerodynamics, entry, recovery, landing, or the fixed RocketSet pipeline in Cosmic Systems.
---

# Rocket Flight Simulation

Apply this skill before changing flight-authoritative code. The rocket is a
physical simulation, not a transform animation.

## Existing Authorities

Read and extend these modules before adding any rocket behaviour:

- `src/infrastructure/plugins/mod.rs`: `RocketSet` registration and fixed-pipeline order.
- `src/domain/services/rocket_dynamics.rs`: pure 6-DOF state and integration maths.
- `src/infrastructure/bevy_adapters/rocket_dynamics.rs`: ECS force/torque adapters.
- `src/domain/services/rocket_propulsion.rs` and `rocket_propulsion.rs`: engines, mass flow, gimbal, and staging.
- `rocket_guidance.rs`, `rocket_control.rs`, and `rocket_contact.rs`: command path and terrain contact authority.
- `rocket_flight_conditions.rs`, `rocket_gravity_orbit.rs`, `rocket_entry.rs`, and `rocket_recovery.rs`: environment, gravity, entry, and recovery.
- `src/infrastructure/bevy_adapters/rocket_pipeline_tests.rs`: complete fixed-pipeline integration tests.

Do not add a second gravity calculation, vehicle integrator, flight clock,
contact solver, rocket manager, or alternate control pipeline.

## Authority And Data Flow

```text
mission/guidance target
  -> controller command
  -> actuator command
  -> gravity, atmosphere, aero, propulsion, entry, contact forces
  -> force and torque accumulation
  -> fixed 6-DOF integration
  -> authoritative state
  -> render interpolation, telemetry, camera, UI
```

- `RocketPhysicsState` and domain dynamics state are authoritative.
- `Transform`, camera state, UI, and rendered meshes are presentation only.
- Guidance selects a desired direction/trajectory; it never teleports or rotates a rocket.
- Control produces bounded commands; it never directly rewrites dynamics.
- Actuation maps commands to engines, gimbal, RCS, grid fins, and other hardware.
- Forces and torques determine the physical outcome through integration.

## Encapsulated Fixed-Tick State

- `RocketPropulsion::active_core_stage` and `running_core_stage` are the sole
  capability views for synchronized active-stage configuration, inventory,
  reserve, throttle, and engine eligibility. Do not independently index stage
  configuration and propellant vectors.
- `RocketPhysicsState::refresh_attached_mass_properties` atomically refreshes
  mass, inertia, and center of mass from one propulsion snapshot. Call it after
  consumption, separation, fairing changes, or ablation changes; do not update
  individual rigid-body fields in separate systems.
- `ForceAccumulator` and `TorqueAccumulator` accept only unit-named additions.
  `integrate_6dof` is their sole production consumer through `take_force_n` and
  `take_torque_nm`; it clears the completed tick budget as it reads it.
- `RocketFlightConditions` stores one complete private atmosphere sample.
  `refresh_flight_conditions` is its sole production writer through
  `replace_sample`; consumers read the shared snapshot and never overwrite
  density, Mach, pressure, or air-relative velocity independently.
- Test fixtures may use the explicitly test-only constructors. Do not make
  private state public merely to simplify a production system or test.

## Fixed Pipeline

Rocket authority belongs in `FixedUpdate` and consumes `SimulationTime`'s fixed
timestep. Preserve explicit `RocketSet` ordering. The current conceptual order is:

```text
atmosphere/recovery -> guidance -> control -> actuation -> gravity
-> terrain interaction/spent stage/entry -> aero forces and torque
-> propulsion thrust/gimbal/consumption/staging -> accumulate -> integrate
-> advance simulation time -> orbital elements -> ground contact
-> render state -> telemetry -> replay
```

- Put input/UI commands in `Update`; apply authoritative mutations in the fixed pipeline.
- Declare every new ordering requirement with sets, `.before`, `.after`, or `.chain()`.
- Do not use render-frame delta time for rocket physics.
- Do not add sequential dependencies between unrelated systems merely for convenience.

## Physics Rules

- State all physical units in names and documentation: meters, m/s, radians/s,
  kilograms, newtons, pascals, seconds.
- Use `f64` and `DVec3`/`DQuat` for flight-scale domain state.
- Forces and torques are accumulated once per fixed step; integration consumes
  and clears the accumulated value.
- Mass, propellant, center of mass, and inertia derive from the active stages and
  must remain mutually consistent after consumption/separation.
- Engine thrust depends on throttle, engine state, gimbal, and ambient pressure
  through existing propulsion authority. Do not approximate it in presentation.
- Aerodynamics consumes the shared atmosphere/flight-condition sample. Do not
  reimplement density, Mach, dynamic pressure, drag, or lift locally.
- Use the existing gravity/orbital service. A rocket never owns a special planetary-gravity model.
- Ground contact consumes `TerrainSource` through `terrain_collision.rs`; visual
  terrain or camera-relative coordinates must never determine landing physics.

## Guidance, Control, And Lifecycle

- Guidance: target orbit/trajectory/landing state only.
- Control: attitude/throttle/actuator demand with bounds, rates, and anti-windup.
- Actuation: physically available engine/fin/RCS output only.
- Lifecycle transitions such as launch, staging, fairing separation, reentry,
  landing, crash, and relaunch are explicit components/events, not implicit UI state.
- Keep spent stages as separate entities with their own authoritative dynamics.
- Recovery/deck contact belongs to the existing recovery systems; do not apply
  static terrain constraints to a deck-relative landing.

## Required Validation

For changed domain maths, add focused pure tests. For pipeline changes, extend
the nearest `rocket_pipeline_tests.rs` module and test deterministic fixed-step
results, not only visual output.

Run:

```text
cargo fmt --check
cargo check --features dem
cargo clippy --features dem -- -D warnings
cargo test --features dem
cargo build --release --features dem
timeout 10s cargo run --features dem --quiet -- rocket
```

Also bounded-start `cargo run` and `cargo run -- craft` with the same feature
set. Record display limitations instead of claiming visual flight validation
where a graphical window is unavailable.

Reject transform-driven flight, duplicate force calculations, variable-rate
physics, force application after integration, presentation-controlled physics,
unbounded actuator commands, and changes without physics regression tests.
