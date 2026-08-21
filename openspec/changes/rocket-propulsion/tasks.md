## 1. Vehicle definition

- [ ] 1.1 Extend `Rocket` entity with `RocketStage` and `RocketEngine` (position, thrust vector, ISP, gimbal range, state)
- [ ] 1.2 Update `falcon9()` to a two-stage Falcon 9 default; keep existing Falcon-9 parameters
- [ ] 1.3 Keep `mass_flow_rate_kg_s` as the single mass-flow authority

## 2. Propulsion domain logic

- [ ] 2.1 Add thrust calculation `T = m_dot * Isp * g0` with sea-level/vacuum ISP selection
- [ ] 2.2 Add propellant consumption and mass update logic
- [ ] 2.3 Add staging logic (shed spent stage mass, activate next stage)
- [ ] 2.4 Add gimbal torque calculation from thrust-line offset and center of mass, with gimbal range clamping
- [ ] 2.5 Unit tests: rocket equation Δv, mass loss = m_dot·t, staging mass shed, thrust from ISP, gimbal torque direction and clamping

## 3. Propulsion systems

- [ ] 3.1 Add `propulsion_thrust` system feeding the translational accumulator
- [ ] 3.2 Add `propulsion_consumption` system depleting propellant and updating mass/inertia
- [ ] 3.3 Add `propulsion_staging` system handling separation
- [ ] 3.4 Add `propulsion_gimbal` system feeding the rotational accumulator
- [ ] 3.5 Remove hardcoded thrust/fuel logic from `rocket_systems.rs`
- [ ] 3.6 Verify no propulsion system writes the rocket transform directly

## 4. Validation

- [ ] 4.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [ ] 4.2 Confirm craft mode unaffected