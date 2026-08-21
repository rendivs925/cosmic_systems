## 1. Mode selection

- [x] 1.1 Add a `Mode` enum (`Solar`, `Craft`, `Rocket`, `Gyro`) in `application/modes.rs` with a parser that reads the mode token robustly (not substring matching)
- [x] 1.2 Add unit tests for the mode parser covering ordering, unknown values, and substring-safety

## 2. Plugin extraction

- [x] 2.1 Create `infrastructure/plugins/mod.rs` with `SharedSimulationPlugin`
- [x] 2.2 Create `SolarSystemModePlugin` wrapping the body of `setup_solar_system_mode`
- [x] 2.3 Create `CraftModePlugin` wrapping the body of `setup_craft_systems`
- [x] 2.4 Create `GyroModePlugin` wrapping the body of `setup_gyro_mode`
- [x] 2.5 Create `RocketModePlugin` composing the shared world and disabling the solar camera (rocket systems added in later changes)

## 3. Wire main

- [x] 3.1 Replace `args.contains` branching in `src/main.rs` with the `Mode` parser and plugin composition
- [x] 3.2 Preserve window title selection per mode

## 4. Validation

- [x] 4.1 Run `cargo check` and `cargo clippy` and `cargo fmt --check`
- [x] 4.2 Launch `cargo run`, `cargo run -- craft`, `cargo run -- rocket`, and `cargo run -- gyro` and confirm each mode behaves as before
