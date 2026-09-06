# Cosmic System - Architecture Documentation

## Overview

The Cosmic System is built using Domain-Driven Design (DDD) combined with Clean Architecture principles. This ensures a scalable, maintainable, and testable codebase that can evolve with the project's ambitious goals of simulating speculative physics and advanced space operations.

## Architectural Principles

### Domain-Driven Design (DDD)
- **Ubiquitous Language**: Terms for bodies, trajectories, rockets, and terrain are used consistently across all layers
- **Bounded Contexts**: The simulation domain is clearly separated from infrastructure concerns
- **Entities & Value Objects**: Core domain concepts are modeled as immutable, testable units

### Clean Architecture
- **Dependency Inversion**: Inner layers (Domain) don't depend on outer layers (Infrastructure)
- **Separation of Concerns**: Each layer has a single responsibility
- **Testability**: Domain logic can be tested in isolation

## Layer Structure

```
cosmic_systems/
├── src/
│   ├── domain/           # Business Logic Layer
│   ├── application/      # Use Cases Layer
│   ├── infrastructure/   # External Concerns Layer
│   ├── presentation/     # UI Layer
│   └── main.rs           # Composition Root
├── docs/                 # Documentation
├── Cargo.toml            # Dependencies
└── Makefile              # Build Scripts
```

### 1. Domain Layer (`src/domain/`)

**Purpose**: Contains all business logic, rules, and concepts. This layer is completely independent of any external frameworks or technologies.

**Contents**:
- `entities/` - Core domain objects with identity and behavior
- `value_objects/` - Immutable data structures
- `services/` - Domain services for complex business logic
  - `physics.rs` - Physics calculations and formulas
- `aggregates/` - Root entities that enforce business invariants
- `repositories/` - Interfaces for data persistence (traits only)
- `events/` - Domain events for decoupling

**Key Characteristics**:
- No dependencies on Bevy, external libraries, or I/O
- Pure Rust functions and data structures
- Easily testable with unit tests
- Can be extracted to a separate crate if needed

### 2. Application Layer (`src/application/`)

**Purpose**: Orchestrates domain objects to fulfill use cases. Contains application services and workflow logic.

**Contents**:
- `simulation_service.rs` - Core simulation orchestration
- `startup.rs` - Bevy startup system setup

**Key Characteristics**:
- Depends only on Domain layer
- Contains no UI or infrastructure code
- Defines the application's use cases

### 3. Infrastructure Layer (`src/infrastructure/`)

**Purpose**: Handles external concerns like databases, web frameworks, file systems, and third-party integrations.

**Contents**:
- `bevy_adapters/` - Bevy-specific implementations
  - `components.rs` - ECS components
  - `systems.rs` - ECS systems
- `persistence/` - Save/load functionality (future)
- `external_services/` - APIs, ML models (future)

**Key Characteristics**:
- Implements interfaces defined in Domain (Dependency Inversion)
- Contains all Bevy-specific code
- Handles I/O operations

### 4. Presentation Layer (`src/presentation/`)

**Purpose**: User interface and output formatting.

**Contents**:
- `ui_components.rs` - Egui UI elements (future)
- `camera_controller.rs` - Camera controls (future)

**Key Characteristics**:
- Depends on Application and Infrastructure
- Contains rendering and input logic

## Technology Stack

### Core Technologies
- **Rust 1.75+**: Systems programming language with memory safety and performance
- **Bevy 0.14**: ECS-based game engine for 3D rendering and real-time simulation
- **Cargo**: Rust package manager and build system

### Development Tools
- **Makefile**: Build automation and common tasks
- **Git**: Version control
- **VS Code + rust-analyzer**: IDE support

### Dependencies (from Cargo.toml)

```toml
[dependencies]
bevy = { version = "0.14.0", features = ["dynamic_linking"] }
rand = "0.8.5"
```

- **Bevy**: Game engine providing ECS, rendering, input, and windowing
- **Rand**: Random number generation for simulations

## Data Flow

1. **User Input** → Presentation Layer (Bevy systems)
2. **System Events** → Application Layer (use case orchestration)
3. **Business Logic** → Domain Layer (pure functions)
4. **Results** → Infrastructure Layer (rendering, persistence)

## Example: Gyroscope Simulation

```
User Input (Presentation)
    ↓
Simulation Service (Application)
    ↓
Gyroscope Entity (Domain)
    ↓
Physics Calculations (Domain)
    ↓
Bevy Components/Systems (Infrastructure)
    ↓
Rendered Output (Presentation)
```

## Benefits of This Architecture

### Maintainability
- Clear separation makes it easy to locate and modify code
- Changes in one layer don't affect others
- Domain logic is protected from framework changes

### Testability
- Domain layer can be tested without Bevy or graphics
- Each layer can be tested in isolation
- Easy to mock external dependencies

### Scalability
- New features can be added without modifying existing code
- Easy to swap implementations (e.g., different physics engines)
- Supports team development with clear boundaries

### Future-Proofing
- Domain logic can be ported to other engines
- Easy to add new output formats or UIs
- Supports advanced features like AI, networking, VR

## Development Guidelines

### Adding New Features
1. Start with Domain entities/value objects
2. Define interfaces in Domain repositories
3. Implement use cases in Application layer
4. Add Infrastructure adapters
5. Update Presentation as needed

### Testing Strategy
- Unit tests for Domain logic
- Integration tests for Application services
- End-to-end tests for full workflows

### Code Organization
- Keep functions small and focused
- Use meaningful names from ubiquitous language
- Document complex business rules
- Avoid circular dependencies

## Future Enhancements

- **Persistence Layer**: Save/load simulation states
- **Networking**: Multiplayer collaborative simulation
- **VR Support**: Immersive 3D experiences
- **ML Integration**: AI-driven optimization
- **Web Deployment**: WASM compilation for browser access

This architecture provides a solid foundation for building complex, speculative physics simulations while maintaining code quality and developer productivity.
