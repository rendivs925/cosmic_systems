## 1. Vehicle definition

- [x] 1.1 Extend `Rocket` entity with `RocketStage` and `RocketEngine` (position, thrust vector, ISP, gimbal range, state)
- [x] 1.2 Update `falcon9()` to a two-stage Falcon 9 default; keep existing Falcon-9 parameters
- [x] 1.3 Keep `mass_flow_rate_kg_s` as the single mass-flow authority

## 2. Propulsion domain logic

- [x] 2.1 Add thrust calculation `T = m_dot * Isp * g0` with sea-level/vacuum ISP selection
- [x] 2.2 Add propellant consumption and mass update logic
- [x] 2.3 Add staging logic (shed spent stage mass, activate next stage)
- [x] 2.4 Add gimbal torque calculation from thrust-line offset and center of mass, with gimbal range clamping
- [x] 2.5 Unit tests: rocket equation Δv, mass loss = m_dot·t, staging mass shed, thrust from ISP, gimbal torque direction and clamping

## 3. Propulsion systems

- [x] 3.1 Add `propulsion_thrust` system feeding the translational accumulator
- [x] 3.2 Add `propulsion_consumption` system depleting propellant and updating mass/inertia
- [x] 3.3 Add `propulsion_staging` system handling separation
- [x] 3.4 Add `propulsion_gimbal` system feeding the rotational accumulator
- [x] 3.5 Remove hardcoded thrust/fuel logic from `rocket_systems.rs`
- [x] 3.6 Verify no propulsion system writes the rocket transform directly

## 4. Validation

- [x] 4.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [x] 4.2 Confirm craft mode unaffected