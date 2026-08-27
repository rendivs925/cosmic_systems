## 1. Coherence Baseline

- [ ] 1.1 Complete and regression-test camera pose transitions, fixed-epoch ordering, pause gating, live guidance dynamic pressure, and pressure-based propulsion.
- [ ] 1.2 Add schedule and pure-domain regressions for the corrected fixed-flight behavior.

## 2. Typed Domain Values

- [ ] 2.1 Add and validate `CelestialBodyId`; migrate rocket body bindings and bound-body lookup interfaces.
- [ ] 2.2 Add a pure `EngineOperatingPoint` and migrate thrust, propellant flow, gimbal torque, telemetry, liftoff, and debug rendering.
- [ ] 2.3 Add a pure `FlightConditions` value and migrate atmosphere-relative velocity, dynamic pressure, Mach, guidance, aerodynamics, telemetry, and entry consumers.

## 3. ECS Boundaries

- [ ] 3.1 Split `rocket_systems.rs` into cohesive gravity/orbit, flight conditions, guidance/control, propulsion, dynamics/contact, and presentation adapters.
- [ ] 3.2 Narrow feature-local query data and remove duplicated environment and body lookups.
- [ ] 3.3 Preserve explicit fixed-stage ordering and gate all flight stages on unpaused simulation time.

## 4. Presentation

- [ ] 4.1 Move camera transition state changes behind `RocketCameraController` methods and add transition-state tests.
- [ ] 4.2 Verify the camera consumes only interpolated presentation state.

## 5. Validation

- [ ] 5.1 Run `cargo fmt --check`, `cargo check`, `cargo clippy`, and `cargo test`.
- [ ] 5.2 Run normal, craft, and rocket modes under the available graphical test environment; manually verify camera switching in rocket mode.
- [ ] 5.3 Validate the OpenSpec change strictly and document any remaining known limitations.
