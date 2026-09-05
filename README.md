# Cosmic Systems

Cosmic Systems is a solar-system simulation project focused on space
exploration. It creates an interactive 3D environment for observing celestial
bodies, their motion, and the environments in which spacecraft operate, while
evolving into a spaceflight simulator and game for launch, flight, orbit,
landing, and exploration.

## Status

The project is under active development. It provides solar-system, craft, and
rocket-flight modes built on a shared Rust and Bevy application.

## Requirements

- A current Rust toolchain with Cargo.
- The repository assets and scientific data files.
- A native desktop environment with a working graphics driver.

## Run

```bash
# Solar-system mode
cargo run

# Craft mode
cargo run -- craft

# Rocket-flight mode
cargo run -- rocket

# Rocket-flight mode with a configured vehicle
cargo run -- rocket --vehicle falcon9
```

| Command | Description |
| --- | --- |
| `cargo run` | Starts the solar-system simulation. |
| `cargo run -- craft` | Starts craft mode in the shared solar-system world. |
| `cargo run -- rocket` | Starts rocket-flight mode. |

`--vehicle <key>` is available only in rocket mode. Unknown vehicle keys are
reported before the application opens a window.

The equivalent Make targets are `make run`, `make run-craft`, and
`make run-rocket`.

## Project Structure

- `src/domain/`: simulation rules and calculations.
- `src/application/`: startup and configuration.
- `src/infrastructure/`: Bevy ECS integration, assets, streaming, and rendering.
- `src/presentation/`: user interface and visual presentation.
- `assets/`: vehicle configuration, textures, and scientific data configuration.

## Validation

Run these checks before submitting changes:

```bash
cargo fmt --check
cargo check --features dem
cargo clippy --features dem
cargo test --features dem
cargo build --release --features dem
```

For startup or mode changes, also run:

```bash
cargo run
cargo run -- craft
cargo run -- rocket
```

Use `make build-wasm` to build the library for `wasm32-unknown-unknown`.

## Contributing

Keep changes focused and reuse existing simulation and presentation systems
where possible. See [AGENTS.md](AGENTS.md) for project architecture and
engineering rules.

## License

Licensed under the [MIT License](LICENSE).
