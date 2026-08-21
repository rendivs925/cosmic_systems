## 1. Domain gravity function

- [ ] 1.1 Add `domain/services/gravity.rs` with `gravitational_acceleration(body_mass_kg, position_m, body_position_m) -> DVec3` using Newton's law
- [ ] 1.2 Add a gravitational parameter constant with explicit units and a single authoritative location
- [ ] 1.3 Unit tests: Earth surface acceleration ≈ 9.8 m/s², inverse-square behavior, circular-orbit period consistency

## 2. Bevy gravity system

- [ ] 2.1 Add a system that reads `PlanetComponent` + reference frame and computes gravitational acceleration for the rocket
- [ ] 2.2 Wire the gravity output into the rocket acceleration accumulator (before 6-DOF integration lands, store it for later consumption)
- [ ] 2.3 Ensure rendering does not compute a separate gravity value

## 3. Validation

- [ ] 3.1 Run `cargo check`, `cargo clippy`, `cargo fmt --check`, `cargo test`
- [ ] 3.2 Verify craft mode behavior is unchanged (ZPE gravity constant untouched)