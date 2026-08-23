# cosmic_systems

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Bevy](https://img.shields.io/badge/Bevy-0.14-blue?logo=bevy)](https://bevyengine.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Cosmic System** – Rust + Bevy simulation platform for speculative physics and advanced space operations. Build autonomous spacecraft for satellite rendezvous and tethering, explore anti-gravity propulsion, zero-point energy, Casimir effects, and AI-driven debris removal. Simulate warp drives and quantum phenomena in immersive 3D.

## Demo recording of the system:

https://github.com/user-attachments/assets/4d5e3e96-c477-4a10-a07a-a5eb68577be9

## Features planned

- Autonomous spacecraft fleets with AI-driven rendezvous, tethering, and servicing of satellites
- Speculative propulsion systems: possible anti-gravity, zero-point energy, Casimir-based thrusters
- Orbital debris management with AI swarms for detection, tracking, and removal
- Warp drive and spacetime metric simulations
- Quantum effects and vacuum phenomena visualization
- High-performance real-time 3D rendering with interactive parameter controls

## Architecture

This project follows Domain-Driven Design (DDD) + Clean Architecture principles for maintainability and scalability. The codebase is organized into layers:

- **Domain**: Pure business logic (entities, services, value objects)
- **Application**: Use cases and orchestration
- **Infrastructure**: External concerns (Bevy integration, persistence)
- **Presentation**: UI and rendering

See [docs/architecture.md](docs/architecture.md) for detailed documentation.

## Installation & Quick Start

1. **Clone the repository**

   ```bash
    git clone https://github.com/rendivs925/cosmic_systems.git
    cd cosmic_systems
   ```

2. **Build and run**

   ```bash
   make space-simulation
   ```

3. **Interactive Controls** (Gyro Mode)
   - **↑/↓ Arrows**: Increase/Decrease gyroscope RPM
   - **W/S**: Increase/Decrease precession frequency
   - **A/D**: Decrease/Increase asymmetry factor
   - Watch the console output for parameter changes and observe the gyroscope's inertia effects in real-time

## Requirements

- Rust 1.75 or later
- Recommended: a GPU with Vulkan/Metal/DirectX 12 support for best performance

## Current Status

This is an early-stage project. Core visualization (gyroscopic propulsion, basic orbital mechanics, and UI controls) is implemented. Advanced AI, warp drives, and quantum simulations are under active development.

### Rocket flight simulation

The `rocket` mode (`cargo run -- rocket --vehicle <falcon9|starship|electron|sls>`) flies data-driven launch vehicles from a Kennedy Space Center pad through ascent, staging, orbit, deorbit, entry, and landing — all on one shared solar-system infrastructure (AGENTS-governed: physics is authoritative, rendering is presentation).

Recent phases, in brief:

- **Ascent guidance hardening** — the gravity turn is gated on real tower
  clearance (altitude AND vertical speed) instead of wall-clock time, and the
  attitude PID is inertia-normalized so the same gains are stable on a 13 t
  Electron and a 142 t Falcon 9. Low-thrust vehicles now reach staging cleanly.
- **Landing gear** — optional per-vehicle `landing_legs` in the RON catalog;
  sized spring-damper struts with penalty-method soft contact inside the single
  GroundContact authority, gear-aware touchdown limits, tip-over criterion,
  and visual struts. Gear-less vehicles keep rigid point contact.
- **Post-landing lifecycle** — sustained leans beyond the critical angle
  topple under gravity torque; every touchdown records a scorecard (descent /
  lateral speed, tilt, slope, distance to target, strut compression) on the
  HUD; `R` relaunches: refuel from RON values, reset upright at the current
  site, clear debris.
- **Orbital operations & telemetry** — Hohmann / bi-elliptic transfer solvers
  and plane-change (+ combined-maneuver) Δv math as pure tested functions,
  with an autopilot Transfer mode composing the existing insertion machinery;
  F11 exports the flight recorder to CSV; `,`/`.`/`0` control time
  acceleration (0.1×–10000×) shown on the HUD, with a burn-rig regression
  proving consumption/staging bookkeeping stays consistent at 100×.
- **Ocean mask & housekeeping** — water inference is driven by an explicit
  per-body `has_ocean` config flag (no name guessing); both previously failing
  legacy tests are fixed, and OpenSpec artifacts were reconciled/archived.

Validation for every phase: `fmt`, `check`, `clippy` (zero new warnings),
`test`, release build, plus panic-free xvfb runs of all modes.

## Roadmap

- Full orbital mechanics with n-body simulation
- AI pathfinding and swarm behavior
- Tethering and satellite servicing physics
- Warp bubble rendering and energy calculations
- Zero-point energy and Casimir force visualization
- VR support

## Contributing

Contributions are welcome!  
Please open an issue first to discuss your idea. PRs for bug fixes, new features, or documentation are appreciated.

## License

MIT License – see [LICENSE](LICENSE) for details.

---

_Inspired by the intersection of fringe physics theories and real-world space technology._
