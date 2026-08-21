## 1. Mode selection

- [ ] 1.1 Add a `Mode` enum (`Solar`, `Craft`, `Rocket`, `Gyro`) in `application/modes.rs` with a parser that reads the mode token robustly (not substring matching)
- [ ] 1.2 Add unit tests for the mode parser covering ordering, unknown values, and substring-safety

## 2. Plugin extraction

- [ ] 2.1 Create `infrastructure/plugins/mod.rs` with `SharedSimulationPlugin`
- [ ] 2.2 Create `SolarSystemModePlugin` wrapping the body of `setup_solar_system_mode`
- [ ] 2.3 Create `CraftModePlugin` wrapping the body of `setup_craft_systems`
- [ ] 2.4 Create `GyroModePlugin` wrapping the body of `setup_gyro_mode`
- [ ] 2.5 Create `RocketModePlugin` composing the shared world and disabling the solar camera (rocket systems added in later changes)

## 3. Wire main

- [ ] 3.1 Replace `args.contains` branching in `src/main.rs` with the `Mode` parser and plugin composition
- [ ] 3.2 Preserve window title selection per mode

## 4. Validation

- [ ] 4.1 Run `cargo check` and `cargo clippy` and `cargo fmt --check`
- [ ] 4.2 Launch `cargo run`, `cargo run -- craft`, `cargo run -- rocket`, and `cargo run -- gyro` and confirm each mode behaves as before
