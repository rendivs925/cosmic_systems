# cosmic_frontier_simulator

[![Rust](https://img.shields.io/badge/Rust-1.75+-orange?logo=rust)](https://www.rust-lang.org/)
[![Bevy](https://img.shields.io/badge/Bevy-0.14-blue?logo=bevy)](https://bevyengine.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

**Cosmic Frontier Simulator (CFSim)** – Rust + Bevy simulation platform for speculative physics and advanced space operations. Build autonomous spacecraft for satellite rendezvous and tethering, explore anti-gravity propulsion, zero-point energy, Casimir effects, and AI-driven debris removal. Simulate warp drives and quantum phenomena in immersive 3D.

## Features
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
   git clone https://github.com/rendivs925/cosmic_frontier_simulator.git
   cd cosmic_frontier_simulator
   ```

2. **Build and run**
    ```bash
    make space-simulation
    ```

## Requirements
- Rust 1.75 or later
- Recommended: a GPU with Vulkan/Metal/DirectX 12 support for best performance

## Current Status
This is an early-stage project. Core visualization (gyroscopic propulsion, basic orbital mechanics, and UI controls) is implemented. Advanced AI, warp drives, and quantum simulations are under active development.

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

*Inspired by the intersection of fringe physics theories and real-world space technology.*
