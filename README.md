# Cosmic Systems

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Bevy](https://img.shields.io/badge/Bevy-0.17.3-blue?logo=bevy)](https://bevyengine.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Cosmic Systems is a Rust and Bevy 3D solar-system and rocket-flight simulator.
Its simulation state is authoritative; rendering, terrain streaming, cameras,
and UI present that state without owning physical rules.

## Run

```bash
cargo run
cargo run -- craft
cargo run -- rocket
cargo run -- rocket --vehicle falcon9
cargo run -- gyro
```

Equivalent Make targets are `make run`, `make run-craft`, `make run-rocket`,

## Current Capabilities

- Kernel-backed primary-body ephemerides and planetary orientation.
- Fixed-step f64 rocket dynamics, gravity, propulsion, staging, guidance,
  atmospheric entry, landing contact, replay, and telemetry.
- Data-driven Falcon 9, Starship, Electron, and SLS vehicle configurations.
- Deterministic procedural Earth terrain shared by collision and rendering,
  with cube-sphere LOD, bounded streaming, erosion, hydrology, and local launch
  site calibration.
- Solar-system, craft, rocket, and gyro modes composed from shared Bevy
  infrastructure.

## Architecture

- `domain/`: pure physical models, coordinate conversions, terrain, and
  deterministic simulation services.
- `application/`: startup composition and validated configuration.
- `infrastructure/`: Bevy ECS adapters, plugins, asset integration, streaming,
  and presentation synchronization.
- `presentation/`: UI and visual presentation.

## Validation

```bash
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
```

Use `make build-wasm` to build the library for `wasm32-unknown-unknown`.

## License

MIT License. See [LICENSE](LICENSE).
