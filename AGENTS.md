# AGENTS.md

## Project Overview

This is a Rust-based 3D cosmic simulation and spaceflight project built with the Bevy game engine.

The long-term goal is to evolve the existing project into a:

> World-class 3D real-world solar-system and rocket flight simulator.

The simulator should eventually support:

- realistic solar-system dynamics
- planetary gravity
- planetary rotation
- high-precision orbital mechanics
- 3D rocket flight
- 6-DOF rigid-body dynamics
- propulsion and staging
- atmospheric flight
- aerodynamics
- guidance and control
- orbital insertion and transfers
- atmospheric reentry
- planetary terrain
- procedural and real-world terrain data
- terrain streaming
- terrain LOD
- terrain collision
- realistic landing
- telemetry and debugging
- deterministic simulation
- scalable rendering
- multiple simulation modes

The project already contains a working solar-system simulation and an existing UFO/craft mode.

The codebase must evolve incrementally.

DO NOT build a second simulator beside the existing one.

DO NOT duplicate existing physics, coordinate systems, rendering systems, camera systems, celestial-body systems, or infrastructure.

The existing project is the source of truth.

---

# 1. Core Engineering Philosophy

The most important rule:

> Understand the existing system before changing it.

Before creating any new abstraction, system, component, resource, plugin, service, entity, repository, or module:

1. Search the repository.
2. Find existing functionality.
3. Determine whether it can be reused.
4. Determine whether it can be extended.
5. Determine whether it needs fixing.
6. Determine whether it needs refactoring.
7. Only then consider creating something new.

Preferred order:

    REUSE
      ↓
    EXTEND
      ↓
    FIX
      ↓
    REFACTOR
      ↓
    REPLACE
      ↓
    INVENT

"New" is not automatically "better".

The goal is a coherent simulator, not a large codebase.

---

# 2. Bevy Is the Runtime Architecture

Bevy's ECS is the primary runtime architecture.

Use:

- Entities for simulation objects
- Components for entity state
- Systems for behavior
- Resources for global/shared state
- Events/Messages for communication between systems
- Plugins for feature/module composition
- Schedules/SystemSets for execution ordering
- Assets for render resources
- States for application/game modes where appropriate

Do NOT fight ECS by recreating an object-oriented entity architecture inside it.

Avoid:

    RocketManager
    PlanetManager
    PhysicsManager
    TerrainManager
    CameraManager

when the responsibility can naturally be represented through:

    Components
    Resources
    Systems
    Plugins
    Queries

Prefer data-oriented ECS composition.

For example:

    Rocket
    Transform
    Velocity
    Mass
    Propellant
    Attitude
    Engine

should normally be separate components when those concepts have independent responsibilities or useful reuse.

Do not create a giant:

    Rocket {
        everything...
    }

component merely because it looks convenient.

---

# 3. DDD + ECS Boundary

DDD principles are useful for the simulation domain.

Bevy ECS is responsible for runtime execution.

Do not force DDD concepts into inappropriate Bevy structures.

Preferred conceptual separation:

    domain/
        pure simulation concepts
        mathematical models
        physical laws
        value objects
        domain rules

    infrastructure/
        Bevy ECS integration
        components
        systems
        plugins
        resources
        asset integration
        rendering integration

    application/
        application composition
        simulation configuration
        mode selection
        orchestration

    presentation/
        UI
        telemetry
        visualization
        camera presentation

The domain should not depend on Bevy when that dependency is unnecessary.

Prefer pure Rust domain logic for:

- orbital calculations
- gravity calculations
- atmospheric equations
- propulsion calculations
- aerodynamic equations
- coordinate conversions
- terrain height functions
- numerical algorithms
- guidance mathematics
- control mathematics

Bevy systems should adapt those models into ECS execution.

---

# 4. Bevy Plugin Architecture

Features should be composed through Bevy plugins.

Prefer:

    SolarSystemPlugin
    PhysicsPlugin
    TerrainPlugin
    RocketPlugin
    AtmospherePlugin
    AerodynamicsPlugin
    TelemetryPlugin

over a single enormous application setup.

However:

> Do not create a plugin for every tiny file.

A plugin should represent a meaningful feature or subsystem.

Good:

    RocketPhysicsPlugin

Bad:

    RocketMassPlugin
    RocketVelocityPlugin
    RocketPositionPlugin

unless there is a strong architectural reason.

Plugins should primarily:

- register resources
- register components where required
- register systems
- configure system sets
- register assets
- configure states
- initialize feature-specific infrastructure

Plugins should not become giant service objects.

Bevy plugins are the preferred composition mechanism for application functionality. :contentReference[oaicite:2]{index=2}

---

# 5. Application Modes

The project supports explicit simulation modes.

Current modes:

    cargo run

    -> normal solar-system simulation

    cargo run -- craft

    -> existing UFO/craft simulation

Required rocket mode:

    cargo run -- rocket

    -> rocket flight simulation

Preserve existing behavior.

Do not break:

    cargo run

Do not break:

    cargo run -- craft

The rocket mode should compose shared infrastructure instead of duplicating it.

Conceptually:

    Shared
       │
       ├── Solar System
       ├── Physics
       ├── Rendering
       ├── Camera
       └── Time
              │
        ┌─────┼─────┐
        ▼     ▼     ▼
      Normal Craft Rocket

Use the project's existing mode architecture if one already exists.

Do not introduce a second mode system.

---

# 6. ECS Component Guidelines

Components should represent data/state.

Good components:

    Position
    Velocity
    Mass
    Propellant
    Thrust
    Attitude
    AngularVelocity
    CelestialBody
    TerrainPatch
    Rocket
    Engine
    AtmosphericBody

Avoid components that primarily contain behavior.

Avoid:

    RocketController {
        fn update(...)
    }

Prefer systems:

    rocket_control_system(...)
    rocket_physics_system(...)
    rocket_propulsion_system(...)

Keep components:

- small
- cohesive
- explicit
- serializable where useful
- reusable where appropriate

Do not combine unrelated state into a single component.

---

# 7. Resources

Use Bevy Resources for global state.

Examples:

    SimulationTime
    SimulationSettings
    SolarSystem
    PhysicsSettings
    TerrainSettings
    RocketConfiguration
    InputState
    TelemetryState

Do not use a Resource when the data belongs to individual entities.

Bad:

    RocketStateResource {
        every rocket's state...
    }

Prefer entity components.

Use Resources for data that is genuinely global or shared.

Bevy Resources are intended for globally unique application data. :contentReference[oaicite:3]{index=3}

---

# 8. Systems

Systems should have one clear responsibility.

Prefer:

    calculate_gravity
    integrate_velocity
    integrate_position
    consume_propellant
    calculate_thrust
    calculate_aerodynamic_drag
    update_terrain_lod

over:

    update_everything

A system should ideally:

1. read required state
2. calculate one conceptual operation
3. write the resulting state

Avoid deeply nested systems.

Use guard clauses.

Prefer:

    if !condition {
        return;
    }

over deeply nested:

    if condition {
        if another_condition {
            if ...
        }
    }

---

# 9. System Ordering

Do not rely on accidental system ordering.

If order matters, express it explicitly.

Prefer:

    .chain()

or:

    .before(...)
    .after(...)

or named SystemSets.

For larger systems, define explicit sets.

Example conceptual pipeline:

    Input
       ↓
    Guidance
       ↓
    Control
       ↓
    Actuators
       ↓
    Forces
       ↓
    Physics Integration
       ↓
    Reference Frame Update
       ↓
    Rendering Synchronization

Do not allow unrelated systems to depend on execution order accidentally.

---

# 10. Simulation vs Rendering

Simulation state and rendering state must remain conceptually separate.

The simulation is authoritative.

Rendering visualizes simulation state.

Do not make gameplay physics depend on:

    Transform
    GlobalTransform
    camera position
    rendered mesh
    visual effects

unless the specific visual state is genuinely part of the physical model.

Prefer:

    Simulation State
          ↓
    Presentation Adapter
          ↓
    Bevy Transform
          ↓
    Rendering

Do not use rendered transforms as the source of truth for orbital mechanics.

---

# 11. Fixed-Timestep Physics

Physics should use a fixed simulation timestep where appropriate.

Use Bevy's fixed schedule for deterministic fixed-rate simulation logic.

Conceptually:

    FixedUpdate
        ↓
    gravity
        ↓
    propulsion
        ↓
    aerodynamics
        ↓
    forces
        ↓
    integration

while:

    Update
        ↓
    input
    UI
    camera
    presentation
    telemetry visualization

Bevy provides `FixedUpdate` specifically for logic such as physics and other fixed-rate gameplay logic. :contentReference[oaicite:4]{index=4}

Do not perform authoritative physics using render-frame `delta_seconds()` simply because it is convenient.

Rendering may run at:

    60 FPS
    144 FPS
    240 FPS

while simulation may run at a controlled fixed rate.

Do not assume one render frame equals one physics step.

---

# 12. Time

Separate:

    real time
    simulation time
    physics timestep
    time acceleration

The simulator may eventually support:

    1x
    10x
    100x
    1,000x
    10,000x

Do not multiply physical equations by arbitrary time-scale values inside individual systems.

Centralize simulation time control.

Physics systems should consume the simulation timestep.

---

# 13. Numerical Precision

This is a space simulator.

Floating-point precision is a first-class architectural concern.

Determine whether each subsystem requires:

    f32
    f64
    DVec3
    Vec3

Do not automatically use f32 everywhere.

Do not automatically use f64 everywhere.

Choose precision based on:

- simulation scale
- numerical stability
- performance
- required accuracy
- coordinate system
- rendering requirements

The simulation may need high precision while rendering needs local coordinates.

Potential architecture:

    High precision simulation coordinates
                ↓
        reference-frame conversion
                ↓
        local render coordinates
                ↓
              Bevy

Do not introduce a floating origin if the project already has an equivalent mechanism.

Do not create multiple unrelated coordinate systems.

---

# 14. Coordinate Systems

Coordinate systems must be explicitly documented.

The simulator may eventually require:

    Solar inertial frame
        ↓
    Planet-centered inertial frame
        ↓
    Planet body-fixed frame
        ↓
    Local tangent frame
        ↓
    Rocket body frame

Clearly define:

- axis conventions
- handedness
- units
- origin
- orientation
- frame ownership
- conversion rules

Never silently assume:

    +Y = up

or:

    +Z = forward

unless that is explicitly the project's convention.

Do not create duplicate coordinate conversion utilities.

There must be one authoritative implementation for each conversion.

---

# 15. Units

Physical quantities must have explicit units.

Prefer names such as:

    meters
    meters_per_second
    kilograms
    newtons
    pascals
    kelvin
    radians

Avoid ambiguous variables:

    speed
    force
    distance

when the unit is unclear in a physics-heavy subsystem.

Prefer:

    velocity_mps
    altitude_m
    mass_kg
    thrust_n
    pressure_pa

or strongly typed unit/value objects where the project already uses them.

Never mix:

    meters
    kilometers
    astronomical units

without explicit conversion.

---

# 16. Physics Architecture

Physics should be layered.

Conceptually:

    Celestial Mechanics
          ↓
    Gravity
          ↓
    Vehicle Forces
          ↓
    Aerodynamics
          ↓
    Propulsion
          ↓
    Control Forces/Torques
          ↓
    Integration
          ↓
    State Update

Do not let the rocket system independently calculate planetary gravity if the existing solar-system physics already does it.

Reuse the authoritative gravity implementation.

The rocket should consume gravitational acceleration/force from the existing physics domain.

---

# 17. Rocket Physics

Rocket simulation must be physically authoritative.

Do not fake flight by manipulating:

    Transform.translation
    Transform.rotation

directly.

The physical state should determine the transform.

The rocket should eventually support:

### Translation

    position
    velocity
    acceleration
    mass

### Rotation

    orientation
    angular velocity
    angular acceleration
    inertia tensor

### Propulsion

    thrust
    throttle
    specific impulse
    mass flow
    propellant
    staging
    engine gimbal

### Forces

    gravity
    thrust
    drag
    lift
    other external forces

### Torques

    engine gimbal
    RCS
    aerodynamic torque
    control torque

---

# 18. Guidance / Control / Physics Separation

These are different systems.

Guidance answers:

    Where should the vehicle go?

Control answers:

    What attitude/actuator commands should achieve that?

Physics answers:

    What actually happens?

Preferred architecture:

    Mission
       ↓
    Guidance
       ↓
    Controller
       ↓
    Actuators
       ↓
    Physics
       ↓
    State
       ↓
    Guidance

Never make guidance directly teleport or rotate the rocket.

---

# 19. Atmospheric Simulation

Atmospheric calculations must be centralized.

Do not scatter:

    density = ...

through multiple systems.

Create one authoritative atmospheric model.

It should eventually support:

- temperature
- pressure
- density
- speed of sound
- altitude
- Mach number

Then aerodynamics can calculate:

- drag
- lift
- side force
- dynamic pressure
- aerodynamic torque
- angle of attack

Different planets should be able to provide different atmosphere models.

---

# 20. Terrain Architecture

Terrain is a major subsystem.

The intended architecture should support:

    Planet
       ↓
    Cube Sphere
       ↓
    Terrain Source
       ↓
    Terrain Patches
       ↓
    Quadtree
       ↓
    LOD
       ↓
    Mesh
       ↓
    Material
       ↓
    Collision

Terrain should not be implemented as one giant planet mesh.

Support eventually:

- procedural terrain
- heightmaps
- real planetary DEM data
- deterministic generation
- craters
- mountains
- valleys
- geological features
- terrain streaming
- LOD
- caching
- collision

---

# 21. Terrain Data vs Terrain Rendering

Separate:

    Terrain Data

from:

    Terrain Rendering

and:

    Terrain Collision

Conceptually:

    TerrainSource
         │
         ├── Render Mesh
         │
         ├── Material Data
         │
         └── Collision Data

This allows:

    ProceduralTerrainSource

and eventually:

    DemTerrainSource

without rewriting the entire renderer.

---

# 22. Terrain LOD

Never render maximum terrain resolution everywhere.

Use hierarchical terrain.

Conceptually:

    L0
    Planet

    L1
    Continental

    L2
    Regional

    L3
    Local

    L4
    Landing

    L5
    Micro-detail

Use quadtree subdivision or an equivalent hierarchical approach.

LOD decisions should be based on actual rendering requirements such as:

- camera distance
- screen-space error
- projected geometric error
- visibility

Do not use arbitrary distance thresholds without understanding why they work.

---

# 23. Terrain Streaming

Large terrain must be streamed.

Do not load an entire planetary surface at maximum resolution.

The terrain system should eventually support:

    requested
       ↓
    generating
       ↓
    loading
       ↓
    ready
       ↓
    visible
       ↓
    cached
       ↓
    evicted

Terrain generation should not unnecessarily block the main thread.

Use Bevy's task infrastructure where appropriate.

Do not introduce asynchronous complexity until profiling or requirements justify it.

---

# 24. Terrain Collision

Visual terrain and collision terrain may use different resolutions.

Prefer:

    High-resolution visual terrain

and:

    appropriate-resolution collision terrain

Near the rocket, collision resolution should increase.

Collision must support:

- ground contact
- surface normal
- altitude above terrain
- slope
- landing
- crash detection

Do not create a full-planet high-resolution physics mesh.

---

# 25. Assets

Use Bevy's asset system consistently.

Do not manually load the same asset multiple times from unrelated systems.

Centralize reusable asset handles where appropriate.

Use typed asset resources/collections when the project benefits from them.

Keep:

    asset loading

separate from:

    gameplay logic.

Do not hardcode asset paths throughout systems.

---

# 26. Mesh Generation

Procedural meshes must be deterministic.

Given:

    seed
    terrain coordinates
    resolution
    generation parameters

the same input should produce the same result.

Mesh generation should not depend on:

- frame rate
- entity spawn order
- random global state
- camera movement

unless explicitly intended.

Keep geometry generation separate from mesh spawning.

Prefer:

    terrain data
        ↓
    mesh generation
        ↓
    Bevy asset

rather than mixing all three concerns into one system.

---

# 27. Materials and Rendering

Rendering systems should not own simulation rules.

Material selection can depend on physical/environmental data:

    altitude
    biome
    slope
    temperature
    moisture

but should not modify physical state.

Avoid embedding simulation calculations into shaders unless the calculation is explicitly a visual representation.

---

# 28. Camera Architecture

The camera is presentation.

The camera must not become part of the physics model.

Support different camera modes through reusable camera systems:

    orbital
    chase
    cockpit
    surface
    free camera
    map
    debug

Do not duplicate camera mathematics for every mode.

The rocket's physical orientation should be the source of truth.

The camera follows it.

Not the other way around.

---

# 29. UI and Telemetry

UI should observe simulation state.

UI should not mutate physics directly unless it represents an explicit player control command.

Good:

    UI
      ↓
    Command/Input
      ↓
    Control System

Bad:

    UI
      ↓
    directly modify rocket velocity

Telemetry should be derived from authoritative simulation state.

Useful telemetry may include:

    altitude
    radar altitude
    velocity
    vertical velocity
    horizontal velocity
    acceleration
    G-force
    Mach
    dynamic pressure
    throttle
    thrust
    mass
    propellant
    angle of attack
    pitch
    yaw
    roll
    angular velocity
    apoapsis
    periapsis
    orbital velocity
    atmospheric density
    temperature

---

# 30. States

Use Bevy States for meaningful application lifecycle/mode transitions.

Examples may include:

    Loading
    Simulation
    Paused
    Flight
    Landing
    Debug

Do not create a State for every small gameplay condition.

Prefer components for entity-local state.

Prefer resources for global configuration.

Prefer States for application-wide lifecycle/mode transitions.

---

# 31. Events / Messages

Use events/messages for meaningful decoupled communication.

Examples:

    RocketStageSeparated
    EngineStarted
    EngineShutdown
    LandingDetected
    CrashDetected
    TerrainPatchRequested

Do not use events as a replacement for ordinary direct data access.

If a system simply needs current state:

    Query
    Resource

is usually better.

Use events/messages when something happened and other systems need to react.

---

# 32. Query Design

Queries should request only what the system needs.

Prefer:

    Query<(&Velocity, &Mass)>

over unnecessarily requesting:

    Query<(
        &Transform,
        &Velocity,
        &Mass,
        &Rocket,
        &Engine,
        &Propellant,
        ...
    )>

Smaller queries improve clarity and can improve ECS scheduling/parallelism.

Avoid unnecessary mutable access.

Prefer read-only queries whenever possible.

---

# 33. ECS Parallelism

Bevy ECS is designed to exploit parallelism.

Do not introduce unnecessary shared mutable state.

Prefer:

    many entities
    small components
    independent systems

over:

    global mutable managers
    locks
    giant mutable resources

Do not serialize systems unless there is an actual dependency.

If two systems do not depend on each other, allow Bevy to schedule them independently.

---

# 34. Avoid Giant Resources

Do not create:

    SimulationState

containing every piece of simulation data.

Prefer cohesive resources:

    SimulationTime
    SolarSystemConfig
    PhysicsConfig
    TerrainConfig
    RocketConfig

and ECS components for per-entity state.

---

# 35. Avoid God Plugins

Do not create:

    SimulationPlugin

containing thousands of unrelated systems.

Prefer meaningful feature plugins.

However, shared configuration/composition plugins are acceptable when they represent a real architectural boundary.

---

# 36. Domain Services

DDD services should contain domain logic, not Bevy orchestration.

Good:

    GravityCalculator
    OrbitalMechanics
    AtmosphereModel
    AerodynamicModel
    TerrainHeightFunction

Avoid services that exist only to wrap ECS queries.

Bad:

    RocketService::get_rocket(...)

when the system can simply query the entity.

---

# 37. Value Objects

Use value objects for domain concepts where they improve correctness and clarity.

Examples:

    Mass
    Distance
    Velocity
    Acceleration
    Angle
    OrbitalElements
    PlanetaryConstants
    SimulationTime

Do not create a wrapper type for every primitive merely to satisfy an architectural rule.

Use value objects when they:

- enforce invariants
- clarify units
- encapsulate calculations
- prevent invalid states
- improve domain readability

---

# 38. Error Handling

Do not panic for normal runtime conditions.

Prefer:

    Result
    Option
    explicit validation

Use `unwrap()` only when the invariant is genuinely guaranteed and the failure would represent a programmer error.

Do not silently ignore errors.

Avoid:

    let _ = ...

unless deliberately ignoring the result is correct.

---

# 39. Configuration

Do not scatter physics constants throughout the codebase.

Centralize:

    gravitational constants
    planetary constants
    atmospheric constants
    rocket configuration
    terrain configuration
    simulation settings

Avoid magic numbers.

Bad:

    if altitude > 120.0

Prefer a named configuration/value:

    atmosphere.edge_altitude()

---

# 40. Constants

Constants should have:

- meaningful names
- explicit units
- correct precision
- one authoritative location

Do not duplicate:

    EARTH_RADIUS

in multiple modules.

There must be one source of truth.

---

# 41. Performance

Performance matters, but:

> Measure before optimizing.

Priorities:

1. Correctness
2. Architecture
3. Determinism
4. Profiling
5. Optimization

Do not introduce:

- unsafe code
- custom allocators
- SIMD
- GPU compute
- complicated caching
- task pools
- lock-free structures

without evidence that they solve a real bottleneck.

Use Bevy ECS parallelism first.

Then profile.

Then optimize the actual bottleneck.

---

# 42. Memory Management

Avoid unnecessary allocations in per-frame/per-tick systems.

Be careful with:

- Vec creation
- String creation
- mesh regeneration
- terrain generation
- asset duplication
- cloning large structures

Do not optimize every allocation prematurely.

Measure first.

Prefer reusable buffers and caches when profiling proves they matter.

---

# 43. Terrain Performance

Terrain is expected to become one of the largest performance-sensitive systems.

Pay attention to:

- mesh generation
- patch count
- vertex count
- LOD transitions
- terrain cache size
- GPU memory
- CPU generation cost
- collision mesh size
- streaming bandwidth
- task scheduling

Never generate unnecessary terrain.

Never keep unlimited terrain patches resident.

Use explicit cache/eviction policies.

---

# 44. Determinism

Simulation logic should be deterministic where practical.

Avoid simulation behavior depending on:

- render FPS
- system execution order unless explicitly defined
- unordered iteration when order matters
- random global state
- wall-clock time

Use seeded deterministic generation for procedural terrain.

Determinism is especially important for:

- physics tests
- replays
- debugging
- regression testing
- reproducibility

---

# 45. Testing

Every domain calculation should be testable without launching Bevy where possible.

Test:

### Gravity

- inverse-square behavior
- expected acceleration
- multi-body scenarios

### Orbital mechanics

- circular orbit
- elliptical orbit
- escape
- orbital period
- transfer calculations

### Rocket

- mass consumption
- thrust
- staging
- acceleration
- torque

### Atmosphere

- pressure
- temperature
- density
- speed of sound

### Aerodynamics

- drag
- lift
- dynamic pressure
- Mach
- angle of attack

### Terrain

- deterministic generation
- height continuity
- patch boundaries
- LOD
- quadtree
- collision

### Application

    cargo run
    cargo run -- craft
    cargo run -- rocket

must remain functional.

---

# 46. Physics Regression Tests

Never change a physics implementation without regression tests.

For a change:

    old implementation
        ↓
    test expected behavior
        ↓
    new implementation
        ↓
    compare

When changing numerical algorithms, document:

- expected improvement
- numerical trade-offs
- affected scenarios

Do not rewrite working physics merely because a different algorithm looks more sophisticated.

---

# 47. Bevy Schedule Guidelines

Use the appropriate Bevy schedule.

General guidance:

    Startup
        initialization

    FixedUpdate
        authoritative fixed-rate simulation

    Update
        input
        UI
        variable-rate presentation logic

    PostUpdate
        presentation synchronization where appropriate

Do not place physics in `Update` simply because it is easier.

Do not place UI logic in `FixedUpdate`.

Do not duplicate work across schedules.

Bevy's current main schedule separates fixed simulation from per-frame update and rendering synchronization. :contentReference[oaicite:5]{index=5}

---

# 48. System Sets

For complex subsystems, use explicit system sets.

Example:

    RocketSimulationSet::Input
    RocketSimulationSet::Guidance
    RocketSimulationSet::Control
    RocketSimulationSet::Forces
    RocketSimulationSet::Integration
    RocketSimulationSet::PostPhysics

Then define ordering explicitly.

Do not create hundreds of sets.

Sets should communicate meaningful architectural stages.

---

# 49. Rendering vs Simulation Frequency

Do not assume:

    simulation frequency == rendering frequency

The simulation may run multiple fixed steps between frames.

The renderer may interpolate or otherwise present the latest authoritative state.

Do not introduce visual interpolation into the physical state.

---

# 50. Physics Authority

There must be one authoritative source for each physical quantity.

For example:

Gravity:

    ONE gravity implementation

Planetary rotation:

    ONE authoritative source

Rocket mass:

    ONE authoritative source

Rocket velocity:

    ONE authoritative source

Terrain height:

    ONE authoritative terrain source

Do not have:

    rendering gravity
    rocket gravity
    camera gravity

all calculating slightly different values.

---

# 51. Avoid Duplicate Domain Logic

If existing code already calculates:

    gravity

do not create:

    rocket_gravity.rs

that calculates gravity again.

If existing code already calculates:

    planet transforms

do not create:

    rocket_planet_transform.rs

with another implementation.

Instead:

    reuse
    generalize
    extend
    fix

---

# 52. Refactoring Rules

When modifying existing code:

Prefer small, understandable changes.

Avoid giant rewrites.

Do not rename hundreds of files unless necessary.

Do not migrate architectures simply because another architecture is fashionable.

Refactor only when it improves:

- correctness
- reuse
- maintainability
- performance
- testability
- architectural consistency

Every refactor should have a reason.

---

# 53. File Size

Target:

    200-300 LOC per file

Exceed this only when the file has a clear architectural reason.

Do not blindly split a cohesive module into tiny files merely to satisfy LOC.

Avoid:

    one function per file

Prefer cohesive modules.

---

# 54. Naming

Use clear Rust/Bevy naming.

Types:

    PascalCase

Functions:

    snake_case

Components:

    descriptive nouns

Systems:

    verb-based names

Examples:

    Rocket
    Velocity
    PropellantMass

    calculate_gravity()
    integrate_velocity()
    update_terrain_lod()
    spawn_rocket()

Avoid vague names:

    Manager
    Handler
    Processor
    Thing
    Data
    Utils

unless they have a precise meaning.

---

# 55. Modules

Organize modules around domain/feature boundaries.

Prefer:

    rocket/
        components.rs
        physics.rs
        propulsion.rs
        control.rs
        plugin.rs

over:

    all_components.rs
    all_systems.rs
    all_utils.rs

when the project becomes large.

Feature-local code should remain close together.

---

# 56. Utility Modules

Do not create a giant:

    utils.rs

containing unrelated functions.

Prefer specific modules:

    math/
    coordinates/
    orbital/
    units/
    terrain/

Only create these when there is sufficient shared functionality.

---

# 57. Comments

Code should explain itself.

Comments should explain:

- why
- physical assumptions
- numerical constraints
- non-obvious algorithms
- external references
- invariants

Do not comment obvious code.

Bad:

    // Add velocity to position
    position += velocity * dt;

Good:

    // Integrate in the inertial frame before converting to the
    // planet-fixed frame. This avoids introducing fictitious forces
    // into the inertial integration step.

---

# 58. Documentation

Document important physical models.

For each major model, document:

    equation
    assumptions
    units
    valid range
    numerical method
    limitations

Examples:

- gravity
- atmosphere
- drag
- lift
- orbital propagation
- terrain generation
- reference frames

The simulator should be understandable to another engineer without reverse-engineering every equation.

---

# 59. External Dependencies

Before adding a dependency:

1. Check whether Bevy already provides the capability.
2. Check whether the project already has an equivalent.
3. Check whether a small internal implementation is sufficient.
4. Evaluate maintenance and performance cost.
5. Add the dependency only when justified.

Do not add a crate simply because it is popular.

Avoid dependency duplication.

---

# 60. Bevy API Version

Always inspect:

    Cargo.toml
    Cargo.lock

before using Bevy APIs.

Use the project's actual Bevy version.

Do not blindly use examples from another Bevy version.

If an API has changed, adapt to the version actually used by the project.

Prefer official Bevy documentation and examples.

Bevy maintains migration guides and current API documentation; use them when version-specific behavior matters. :contentReference[oaicite:6]{index=6}

---

# 61. Unsafe Rust

Avoid `unsafe`.

Use unsafe only when:

1. there is a demonstrated performance or interoperability requirement,
2. safe Rust cannot reasonably provide the required behavior,
3. the invariants are explicitly documented,
4. the implementation is tested.

"Performance" alone is not justification.

Do not introduce unsafe code speculatively.

---

# 62. Debugging

Every major simulation subsystem should have useful debug visualization where appropriate.

Examples:

    gravity vectors
    velocity vectors
    thrust vectors
    aerodynamic forces
    center of mass
    center of pressure
    terrain normals
    terrain LOD
    collision geometry
    coordinate frames
    orbital trajectories

Debug visualization must not become part of the simulation itself.

Prefer debug plugins/systems that can be enabled independently.

---

# 63. Telemetry

Simulation telemetry should be derived from authoritative state.

Do not store duplicate values unless required for performance or historical data.

For example:

    velocity

should not have separate authoritative:

    horizontal_velocity
    vertical_velocity
    speed

unless those are explicitly derived representations.

Prefer deriving them from the authoritative vector.

---

# 64. Logging

Use structured logging where appropriate.

Log:

- initialization
- mode selection
- important simulation transitions
- terrain streaming failures
- asset failures
- physics warnings
- invalid configurations

Do not log every physics tick.

Never use logging as a substitute for state management.

---

# 65. Validation

Validate configuration at startup.

Examples:

- invalid planet mass
- negative rocket mass
- invalid engine thrust
- invalid atmospheric parameters
- invalid terrain resolution
- invalid timestep

Fail early for invalid configuration.

Do not allow invalid physical states to silently propagate.

---

# 66. Simulation Modes Must Be Isolated

The three modes:

    cargo run
    cargo run -- craft
    cargo run -- rocket

must not become three copies of the same infrastructure.

Shared:

    physics
    celestial bodies
    rendering
    camera infrastructure
    time
    assets
    coordinates

Mode-specific:

    UFO behavior
    Rocket behavior

The rocket mode should be isolated at the application/plugin level, not by duplicating the underlying engine.

---

# 67. Existing UFO Mode

The UFO mode already exists.

Before modifying it:

1. Find its plugin.
2. Find its components.
3. Find its systems.
4. Find its physics.
5. Find its controls.
6. Determine which parts are generic craft infrastructure.
7. Reuse generic functionality for the rocket.
8. Keep UFO-specific behavior isolated.

Do not copy the UFO implementation into the rocket implementation.

If something is actually generic:

    extract/generalize it.

---

# 68. Solar System

The solar-system simulation is already implemented.

Treat it as existing infrastructure.

Before implementing rocket gravity:

    inspect existing gravity.

Before implementing planet transforms:

    inspect existing planet system.

Before implementing celestial body definitions:

    inspect existing Planet entity/domain model.

Before implementing time:

    inspect existing simulation time.

Before implementing coordinate conversion:

    inspect existing coordinate systems.

Reuse them.

---

# 69. World-Class Simulator Principle

The goal is not:

    maximum number of features.

The goal is:

    physically coherent
    numerically stable
    visually convincing
    deterministic
    scalable
    testable
    maintainable
    extensible

A simulator with fewer correct systems is better than a simulator with many fake systems.

Never fake physics to make a demo look correct.

---

# 70. Anti-Overengineering Rules

Do NOT:

- invent abstractions without a use case
- create speculative interfaces
- create generic factories everywhere
- create managers for ECS data
- create unnecessary traits
- create unnecessary dependency injection
- create duplicate domain services
- create premature plugin hierarchies
- create abstractions around one implementation
- rewrite working code without justification
- add dependencies without need
- optimize without measurement
- build systems before understanding existing systems

Avoid:

    abstraction for abstraction's sake.

---

# 71. AI-Specific Repository Rules

When modifying this repository, AI agents MUST:

1. Search before creating.
2. Read surrounding code before editing.
3. Identify existing patterns.
4. Reuse existing abstractions.
5. Avoid duplicate implementations.
6. Avoid speculative architecture.
7. Avoid giant rewrites.
8. Keep changes scoped.
9. Preserve working behavior.
10. Run validation after changes.

Before implementing a major feature, provide:

    Existing capability
    Missing capability
    Reuse opportunity
    Required changes
    Risks
    Implementation plan

Do not silently invent architecture.

---

# 72. Required Pre-Implementation Audit

Before implementing any major subsystem, answer:

    What already exists?

    Where is it?

    Who uses it?

    Can it be reused?

    Can it be extended?

    Is it incorrect?

    Is it duplicated?

    What is the minimum change required?

Only then implement.

---

# 73. Definition of Done

A feature is NOT complete merely because:

    cargo check

passes.

A feature is complete when:

- architecture is coherent
- existing behavior remains intact
- tests pass
- physics is validated where applicable
- no duplicate implementation was introduced
- no unnecessary abstraction was introduced
- performance is acceptable
- errors are handled
- documentation is updated
- debug/telemetry exists where appropriate
- the feature integrates with existing systems
- the code follows project conventions

---

# 74. Required Validation

Before considering work complete:

    cargo fmt --check
    cargo check
    cargo clippy
    cargo test

For release-oriented changes:

    cargo build --release

For each application mode:

    cargo run
    cargo run -- craft
    cargo run -- rocket

If one mode cannot be run in the current environment, explicitly report why.

Do not claim success without actually running the relevant validation.

---

# 75. Change Discipline

Every change should answer:

    WHY?

    WHAT?

    WHY HERE?

    WHAT EXISTING CODE DOES THIS REUSE?

    WHAT DOES THIS REPLACE?

    WHAT COULD BREAK?

Avoid changing unrelated files.

Avoid opportunistic refactoring.

If unrelated technical debt is discovered:

    document it

rather than silently expanding scope.

---

# 76. Final Architectural Rule

The project should evolve as ONE coherent simulator.

Not:

    Solar System Project
          +
    UFO Project
          +
    Rocket Project
          +
    Terrain Project

Instead:

    ┌─────────────────────────────────────┐
    │           COSMIC SIMULATOR          │
    ├─────────────────────────────────────┤
    │                                     │
    │  Solar System                       │
    │  Celestial Mechanics                │
    │  Reference Frames                   │
    │  Time                               │
    │  Physics                            │
    │  Rendering                          │
    │  Terrain                            │
    │  Atmosphere                         │
    │                                     │
    ├─────────────────────────────────────┤
    │                                     │
    │  Applications / Modes               │
    │                                     │
    │  Normal Simulation                  │
    │  UFO / Craft                        │
    │  Rocket Flight                      │
    │                                     │
    └─────────────────────────────────────┘

Shared systems must remain shared.

Mode-specific behavior must remain isolated.

Physics must remain authoritative.

Rendering must remain presentation.

Domain mathematics must remain testable.

Bevy ECS must remain the runtime composition model.

The architecture should become more coherent as the simulator grows, not more fragmented.
