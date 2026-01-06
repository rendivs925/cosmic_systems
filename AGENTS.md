# AGENTS.md

## Project Structure

This is a Rust-based cosmic simulation project using the Bevy game engine to model and visualize a solar system. The codebase follows Domain-Driven Design (DDD) principles with a clean architecture separation:

### Core Modules (src/)
- **application/**: Orchestrates the simulation startup and service integration
- **domain/**: Contains business logic with aggregates, entities, events, repositories, services, and value objects
  - Entities: Gyroscope, Planet
  - Services: Physics calculations
  - Value Objects: Simulation and Solar System parameters
- **infrastructure/**: Bevy adapters for components, systems, and rendering
- **presentation/**: User interface and presentation logic

### Assets
- **assets/textures/**: Planet textures (albedo, clouds, rings) and background stars

### Documentation
- **docs/**: Architecture overview and simulation plans

### Configuration
- **Cargo.toml**: Rust dependencies (including Bevy)
- **Makefile**: Build scripts
- **download_textures.sh**: Asset fetching script

## Coding Guidelines

### Principles
- **Clean Code**: Write readable, maintainable code with clear intent
- **DRY (Don't Repeat Yourself)**: Eliminate duplication through abstraction
- **SOLID**: Single responsibility, Open-closed, Liskov substitution, Interface segregation, Dependency inversion
- **YAGNI (You Aren't Gonna Need It)**: Implement only what's necessary
- **KISS (Keep It Simple, Stupid)**: Prefer simple solutions over complex ones
- **Self-Explanatory Code**: Write code that explains itself without excessive comments
- **Balanced Conciseness**: Code should be neither too verbose nor too abbreviated
- **Safety First**: Always write safe code that prevents common errors and vulnerabilities
- **Ultra High Performance**: Optimize for extreme performance using advanced techniques
- **Idiomatic Code**: Follow Rust conventions and best practices for the language

### Code Structure
- Limit modules/files to 200-300 lines of code (LOC)
- Exceed this limit only with clear architectural purpose
- Use guard clauses to avoid deeply nested conditions
- Follow existing patterns and conventions in the codebase

### Commands
- Lint: `cargo clippy`
- Typecheck/Build: `cargo check` / `cargo build`
- Test: `cargo test`