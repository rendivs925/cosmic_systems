## 1. Pipeline scaffolding

- [ ] 1.1 Add `RocketCommands` resource (target attitude, gimbal, RCS, throttle) and `MissionPhase` resource
- [ ] 1.2 Add `RocketFlightSet` with ordered `Guidance → Control → Actuation → Physics` stages

## 2. Guidance

- [ ] 2.1 Add guidance system producing phase-based targets (launch, gravity-turn ascent, orbit insertion)
- [ ] 2.2 Expose guidance target generation as pure functions for unit testing
- [ ] 2.3 Unit tests: gravity-turn target profile, phase target switching

## 3. Control

- [ ] 3.1 Add PID attitude controller from guidance target and current state to actuator commands
- [ ] 3.2 Add anti-windup and gain configuration
- [ ] 3.3 Unit tests: attitude convergence with bounded overshoot, command clamping

## 4. Actuation

- [ ] 4.1 Add actuation system applying gimbal range clamp and throttle slew limits
- [ ] 4.2 Add RCS maximum torque bounds
- [ ] 4.3 Unit tests: gimbal clamp, throttle slew limiting

## 5. Integration

- [ ] 5.1 Wire guidance/control/actuation into the accumulator pipeline; remove placeholder `update_rocket_controls`
- [ ] 5.2 Verify guidance/control/actuation never write the rocket transform or physical state directly
- [ ] 5.3 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [ ] 5.4 Confirm craft mode unaffected