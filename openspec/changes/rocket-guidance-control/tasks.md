## 1. Pipeline scaffolding

- [x] 1.1 Add `RocketCommands` resource (target attitude, gimbal, RCS, throttle) and `MissionPhase` resource
- [x] 1.2 Add `RocketFlightSet` with ordered `Guidance → Control → Actuation → Physics` stages

## 2. Guidance

- [x] 2.1 Add guidance system producing phase-based targets (launch, gravity-turn ascent, orbit insertion)
- [x] 2.2 Expose guidance target generation as pure functions for unit testing
- [x] 2.3 Unit tests: gravity-turn target profile, phase target switching

## 3. Control

- [x] 3.1 Add PID attitude controller from guidance target and current state to actuator commands
- [x] 3.2 Add anti-windup and gain configuration
- [x] 3.3 Unit tests: attitude convergence with bounded overshoot, command clamping

## 4. Actuation

- [x] 4.1 Add actuation system applying gimbal range clamp and throttle slew limits
- [x] 4.2 Add RCS maximum torque bounds
- [x] 4.3 Unit tests: gimbal clamp, throttle slew limiting

## 5. Integration

- [x] 5.1 Wire guidance/control/actuation into the accumulator pipeline; remove placeholder `update_rocket_controls`
- [x] 5.2 Verify guidance/control/actuation never write the rocket transform or physical state directly
- [x] 5.3 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [x] 5.4 Confirm craft mode unaffected