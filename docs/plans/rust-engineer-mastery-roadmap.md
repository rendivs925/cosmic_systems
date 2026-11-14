# Rust Engineer Mastery Roadmap: Beginner to Space-Grade

Status: active learning plan. Owner: repository author.
Scope: personal engineering development. This document does not change simulation
behavior and introduces no runtime code.

## 1. Purpose

Build the knowledge, judgment, and evidence needed to work as a
space-company-grade software engineer, with Rust as a primary language. The
reference implementation and laboratory is this repository: a deterministic
solar-system and rocket-flight simulator built on Rust and Bevy.

The plan is designed for:

- a Rust programmer who is comfortable with the language but wants complete
  syntax fluency and production-grade judgment;
- a beginner in mathematics and physics who needs derivation and visual
  intuition, not formula memorization;
- a self-paced learner motivated by curiosity, with no fixed weekly target;
- someone who wants to write code manually, explain every line, and defend
  design decisions in a review.

The outcome is not "finished reading the codebase". The outcome is a portfolio
of working systems, verification evidence, and the ability to design the next
one without supervision.

## 2. Target caliber

"NASA/SpaceX-grade" is a mission-software standard, not a language standard.
It combines:

- domain physics and astrodynamics;
- deterministic, resource-bounded software;
- verification and validation;
- real-time and embedded behavior;
- fault tolerance and failure analysis;
- systems engineering and configuration control;
- clear technical communication;
- safety culture.

Rust is a differentiator and a strong primary language, but the industry also
uses C, C++, Python, MATLAB/Simulink, and Ada/SPARK. This plan keeps Rust central
while requiring enough interop and domain literacy to work in that environment.

Hiring constraints vary by country and employer, including citizenship or
export-control requirements (ITAR/EAR), security clearance, and degree
expectations. Check the actual postings for the roles and locations you target;
those constraints shape the path more than any tutorial.

## 3. Learner profile and operating constraints

| Dimension | Current state | Consequence for the plan |
| --- | --- | --- |
| Rust | Comfortable | Skip beginner syntax; drill advanced syntax and idioms |
| Mathematics | Beginner | Make math the main long-term thread; use visual, code-first cards |
| Physics | Beginner | Derive before implementing; validate against known values |
| Schedule | Fun-driven, no target hours | Measure in sessions and artifacts, not calendar deadlines |
| Learning style | Wants manual coding and full understanding | No copy-paste; implement-then-compare; rebuild-from-blank |
| Persistence | Conversation and this document | Keep the plan durable; keep the code in a separate lab workspace |

## 4. Definition of mastery

A topic is learned when all five levels are demonstrated:

| Level | Evidence |
| --- | --- |
| Explain | Teach the mechanism without notes; state the invariant it protects |
| Trace | Follow a real value through the actual call chain |
| Modify | Change behavior with tests, without breaking other modes |
| Diagnose | Locate a failure to the responsible layer and prove it |
| Design | Propose the right abstraction and justify rejected alternatives |

Reading produces only "Explain". The other four require building, testing, and
review.

## 5. Learning loop

Use this loop for every topic and every unit.

1. Predict: before reading code or a formula, write down what you think happens.
2. Implement small: build the minimal version from a blank file.
3. Test against reality: known values, unit tests, a plot, a measurement.
4. Compare: only now read the production implementation; diff decisions, not lines.
5. Explain: write five to ten lines or teach it aloud without notes.
6. Review: check the diff against invariants, idioms, and correctness.
7. Reuse: later units must reuse what was built, not rewrite it.

Rules:

- Type the code; no copy-paste in learning labs.
- AI is for explanation, hints, and review, not for code that cannot be
  reproduced from memory.
- Every claim gets a test or a measurement.
- Version discipline: read `Cargo.toml` and `Cargo.lock` before following any
  tutorial. This repository uses Bevy 0.17.3, glam 0.30.10, and anise 0.10.6.
- Keep experiments outside the repository, in a separate lab workspace, so the
  simulator is never polluted by learning code.

## 6. Session format

A session is 45 to 90 minutes.

1. Prediction question.
2. Minimal concept plus a math card.
3. Implementation by the learner.
4. Test against a known value.
5. Comparison with production code, when an equivalent exists.
6. Syntax spotlight: tokens and idioms in the files touched.
7. Review and ledger update: what can now be done that could not before.

## 7. Confidence loop

At the end of every unit:

1. Rebuild from a blank project within a timebox, without looking.
2. Explain the design and the mathematics without notes.
3. Break it intentionally and diagnose the failure.
4. Extend it with one feature that was not requested.

If only re-reading is possible, the unit is not finished.

## 8. Evidence standard

Every project produces:

- the artifact (crate, binary, or feature);
- a README with purpose, design, and usage;
- tests, including at least one failure-path test;
- one measurement (time, memory, accuracy, or determinism);
- one known limitation stated honestly;
- a design note and a requirements-to-test table when the project is
  flight-relevant.

This mirrors the repository's own task model: `PLANNED`, `IMPLEMENTED`,
`VERIFIED`, and the rule that code merging is not completion.

## 9. Curriculum overview

| Level | Theme | Sessions | Main artifacts | Exit gate |
| --- | --- | ---: | --- | --- |
| 0 | Orientation and diagnostics | 2-4 | System map, diagnostic, lab workspace | All modes run; plan understood |
| 1 | Math you can see | 12-18 | 2D orbital sandbox | Derive gravity; fixed-step golden test |
| 2 | Simulation core | 25-35 | Typed units, N-body, 3D solar system, rigid body, contact | Deterministic replay; stable landing |
| 3 | Space systems | 40-60 | Propagator, ascent, GNC, entry, terrain, EKF | Headless autonomous flight and validation report |
| 4 | Flight-grade software | 40-60 | `no_std` real-time loop, FDIR, CCSDS, verification | Embedded real-time demo with fault injection |
| 5 | Mission-grade portfolio and career | Ongoing | Deep projects, open source, write-ups | Competitive application package |

Pacing is open-ended. At two sessions per week the path is roughly a year to
Level 3 and longer to Level 5. At four to five sessions per week it is faster.
Progress is measured by artifacts, not elapsed time.

## 10. Level 0 - Orientation and diagnostics

### Sessions

1. Toolchain and environment
   - install or verify `rustup`, `cargo`, `rustfmt`, `clippy`, `rust-analyzer`;
   - confirm the compiler version and edition;
   - create the lab workspace outside the repository;
   - learn `cargo run`, `cargo test`, `cargo doc --open`, `cargo tree`.
2. Repository orientation
   - run all three modes: `cargo run`, `cargo run -- craft`,
     `cargo run -- rocket`;
   - read `README.md`, `AGENTS.md` sections 1-15, and
     `docs/progress/README.md`;
   - locate the five authorities: time, ephemeris, frames, gravity, terrain;
   - write a one-page map of simulation state versus presentation state.
3. Diagnostics
   - Rust syntax diagnostic (twelve constructs: read, explain, write);
   - mathematics diagnostic (algebra, trigonometry, vectors, calculus);
   - physics diagnostic (force, energy, momentum, rotation, orbits);
   - record the results in the portfolio log.
4. Planning
   - choose the first unit;
   - set up the syntax card template and the portfolio log;
   - decide the weekly rhythm that is enjoyable and sustainable.

### Gate

- All three modes start.
- The five authorities can be named and located.
- The fixed timestep, start epoch, and mode composition are understood.
- Diagnostics are recorded and the first unit is chosen.

## 11. Level 1 - Math you can see

Goal: build a 2D orbital sandbox in Bevy from scratch and understand every line.

### Unit 1.1 - Vectors and trigonometry in Rust

- Write `Vec2` without external math crates: add, subtract, scale, dot, cross,
  length, normalize, angle.
- Learn radians, sine, cosine, and the unit circle.
- Tests with known values and edge cases (zero vector, opposite vectors).
- Math cards: M0, M1, M2.

### Unit 1.2 - Gravity and fixed-step motion

- Point mass, inverse-square gravity, semi-implicit Euler.
- Terminal program: circular orbit, energy over time, step-size comparison.
- Prove the relation between force, acceleration, velocity, and position.
- Math cards: M3, M4, M6.

### Unit 1.3 - Bevy basics

- App, plugin, `Startup`, `Update`, fixed step, camera, circle mesh, gizmos.
- Draw the orbit, the body, and the force vector.
- One plugin per concern; no physics in `Update`.
- Bevy rungs B0 and B1.

### Unit 1.4 - Interaction and presentation

- Pause, time scale, body selection, camera focus.
- A small HUD showing speed, radius, and energy.
- Input abstraction so the simulation remains headless-testable.

### Unit 1.5 - Confidence loop

- Rebuild the sandbox from blank in one session.
- Break it deliberately: wrong sign, wrong frame, wrong timestep, wrong energy.
- Diagnose each failure and write the fix.
- Extend it: add a third body or a moon.

### Gate

- Derive gravity from Newton's law on paper.
- Explain why the timestep matters and what changes at 10 Hz versus 1000 Hz.
- Add a third body without help.
- A fixed-step golden test and a requirements-to-test note exist.
- Syntax: S0-S2 and Bevy B0-B1 are usable from memory.

## 12. Level 2 - Simulation core

Goal: master deterministic simulation, frames, precision, and rigid-body
mechanics.

### Unit 2.1 - Typed units, configuration, validation

- Newtypes for meters, seconds, kilograms, radians.
- RON or JSON configuration with typed errors and validation.
- No `unwrap` on user input; fail fast on invalid configuration.
- Rust: S2-S5.

### Unit 2.2 - Deterministic N-body

- Fixed-step point-mass simulation with a seeded scenario.
- Golden-file regression and a determinism test.
- Telemetry output and a small analysis tool.
- Explain floating-point ordering and why it matters.

### Unit 2.3 - 3D solar system in Bevy

- Quaternions, orbital inclination, camera modes, selection, fixed clock.
- One plugin per concern; explicit system ordering.
- Compare the design with `src/application/plugins.rs`.

### Unit 2.4 - Frames, precision, and floating origin

- Solar-inertial, planet-centered inertial, planet body-fixed, local tangent.
- Round-trip property tests and known-coordinate tests.
- A floating-origin demo that keeps positions small near a planet.
- Read `src/domain/services/reference_frames.rs` and compare.

### Unit 2.5 - 6-DOF rigid body

- Center of mass, inertia tensor, torque, angular velocity.
- Quaternion integration and normalization.
- Analytical checks: free rotation, spin stability, forced precession.

### Unit 2.6 - Heightfield contact

- Sphere/plane and heightfield collision.
- Penetration resolution, normal impulse, friction, restitution, resting contact.
- Toppling and angular impulse; avoid the spinning-contact energy pump.
- Gate: a body lands and stays landed across timesteps.

### Unit 2.7 - `no_std` sensor hello

- `core` versus `std`, panic handler, `embedded-hal`, `defmt`.
- Read a simulated sensor and run a bounded control loop.
- Optional: run on an MCU or in QEMU.
- Early exposure prevents a std-only blind spot.

### Gate

- Deterministic replay of a recorded run.
- Stable landing with documented energy behavior.
- Frame conversions have round-trip and known-value tests.
- A design note explains every integration and contact choice.
- Syntax: S3-S7 and Bevy B1-B2 are usable from memory.

## 13. Level 3 - Space systems

Goal: build the spaceflight domain, from orbit to landing, with validation.

### Unit 3.1 - Orbital propagator

- Two-body problem, Kepler's laws, orbital elements, vis-viva.
- Circular, elliptical, parabolic, hyperbolic cases.
- J2 and third-body terms behind selectable force-model tiers.
- Maneuver planning and execution.
- Compare against published states or SPICE output.

### Unit 3.2 - Time and ephemeris

- TAI, UTC, TDB, leap seconds, Julian date, J2000.
- ICRF and IAU body orientation.
- Read a real ephemeris and validate body states.
- Read `src/domain/services/simulation_epoch.rs` and
  `src/domain/services/ephemeris/`.

### Unit 3.3 - Atmosphere and aerodynamics

- ISA layers, pressure, temperature, density, speed of sound, Mach.
- Drag, lift, side force, angle of attack, dynamic pressure, stability.
- Grid fins and control surfaces.

### Unit 3.4 - Propulsion and staging

- Rocket equation, specific impulse, mass flow, throttle.
- Staging, fairing separation, mass properties.
- Propellant budgets and residuals.

### Unit 3.5 - Ascent simulation

- Gravity turn, drag loss, gravity loss, staging, orbit insertion.
- Compare against a reference trajectory and a regression baseline.
- Sensitivity analysis: density, thrust, mass, guidance.

### Unit 3.6 - Guidance and control

- Open and closed loop, proportional/derivative control, damping.
- Actuator limits, gimbal and RCS allocation.
- Gravity turn, terminal descent, suicide burn, boostback, entry guidance.
- Guidance never writes a render transform.

### Unit 3.7 - Entry and recovery

- Ballistic coefficient, heating rate, ablation, plasma blackout.
- Parachutes, retro-propulsion, landing legs, touchdown.
- Crash detection and post-impact behavior.

### Unit 3.8 - Terrain

- Cube-sphere parameterization and quadtree LOD.
- Mesh generation, normals, seams, caching, streaming.
- Terrain collision, altitude queries, slope.
- Water and surface materials.

### Unit 3.9 - Presentation

- Camera modes, HUD, telemetry, replay, effects, sky.
- Simulation remains authoritative; presentation observes.

### Unit 3.10 - Navigation and dispersion

- Extended Kalman filter for orbit determination.
- Covariance, consistency checks, measurement models.
- Monte Carlo dispersion campaign with statistics and worst cases.

### Gate

- A headless autonomous flight succeeds across sites and seeds.
- A validation report compares results with an independent reference.
- Propellant, control effort, and timing budgets are documented.
- A subspecialty is chosen: GNC, flight software, simulation and test,
  autonomy, or ground software.
- Syntax: S8-S10 and Bevy B3-B4 are usable from memory.

## 14. Level 4 - Flight-grade software

Goal: demonstrate the practices that make software trustworthy on a mission.

### Unit 4.1 - `no_std` Rust

- `core` versus `std`, panic strategies, `alloc`, `embedded-hal`, `defmt`,
  `probe-rs`.
- Build for a target without an operating system.
- Measure code size, stack use, and static allocation.

### Unit 4.2 - Real-time execution

- RTIC or Embassy: tasks, priorities, interrupts, timers.
- Bounded execution, no dynamic allocation in the control path.
- Watchdog, safe mode, startup and shutdown sequences.
- Timing analysis and WCET reasoning.

### Unit 4.3 - Determinism and reproducibility

- Fixed-point versus floating-point trade-offs.
- Reproducible builds and pinned toolchains.
- Seeded randomness and replay.
- Determinism tests across machines and optimization levels.

### Unit 4.4 - Fault detection, isolation, and recovery

- Health monitoring, thresholds, voting, redundancy.
- FDIR state machine: nominal, degraded, safe, recovery.
- Fault injection harness and deterministic recovery tests.

### Unit 4.5 - Communications

- CCSDS space packets: primary header, sequence counters, time codes,
  checksums.
- Telemetry and telecommand framing.
- Encode/decode property tests and malformed-input handling.

### Unit 4.6 - Verification

- Unit, integration, and system tests.
- Property tests with `proptest`; fuzzing with `cargo-fuzz`.
- Coverage and mutation testing.
- `Miri` for undefined behavior; `Kani` for bounded formal checks on critical
  functions.
- State the property, the bound, and the residual risk.

### Unit 4.7 - Safety toolchain and supply chain

- Static analysis, `clippy`, `cargo-deny`, SBOM generation.
- Awareness of Ferrocene as a qualified Rust toolchain and its current
  qualification scope.
- Awareness of Ada/SPARK as the aerospace safety benchmark.
- Dependency policy, license review, and reproducible dependency resolution.

### Unit 4.8 - Interoperability

- C FFI with `bindgen` and `cbindgen`.
- C++ interop with `cxx`.
- Python interop with `PyO3`.
- Read and modify a small legacy C or C++ component.

### Unit 4.9 - Systems engineering

- Requirements, interfaces, and interface control documents.
- V-model, verification matrix, configuration management.
- Design reviews, trade studies, hazard analysis, FMEA, root-cause analysis.
- Change control and baseline discipline.

### Unit 4.10 - Flight algorithms

- Attitude determination from IMU, star tracker, and sun sensor models.
- EKF/UKF estimation and consistency.
- LQR and MPC for attitude and trajectory control.
- Trajectory optimization with direct collocation as an advanced topic.

### Gate

- A `no_std` real-time loop ingests simulated sensor data, runs guidance and
  control, emits CCSDS telemetry, survives injected faults, and has a
  verification report.
- Every critical function has a property, test, or formal check.
- Resource budgets and failure modes are documented.

## 15. Level 5 - Mission-grade portfolio and career

Goal: convert skill into credible evidence and a competitive application.

### Depth over breadth

Choose two or three deep projects. Each has:

- a design document and an interface description;
- tests, including failure paths;
- a measurement and a validation result;
- a limitations section;
- a review record.

### Open source

Contribute to at least one relevant ecosystem: cFS, F Prime, RTIC, Embassy,
Rust embedded, Rust aerospace or robotics crates, SPICE bindings, or a
simulation project. Start with documentation, tests, or small fixes; then one
substantive change.

### Portfolio

Maintain a public or private portfolio repository containing:

- the projects and their READMEs;
- the verification evidence;
- the write-ups;
- the career narrative.

### Subspecialty

By the end of Level 3, choose one:

| Subspecialty | Core skills | Signature artifact |
| --- | --- | --- |
| GNC | Astrodynamics, control, estimation, Python/C++ | Validated GNC simulation, EKF, Monte Carlo |
| Flight software | C/C++, Rust, RTOS, embedded, FDIR, CCSDS | `no_std` real-time app with fault injection |
| Simulation and test | Physics, numerics, HIL/SIL, Python | 6-DOF simulation and test rig |
| Autonomy | Planning, estimation, Rust/C++, ML basics | Autonomous landing or avoidance demo |
| Ground software | Rust/C++/Python, networking, telemetry, data | Telemetry pipeline and analysis tools |

### Interview preparation

- C++ and Python fluency, in addition to Rust.
- Operating systems, real-time concepts, memory, concurrency.
- Data structures and algorithms.
- GNC mathematics and physics for the chosen subspecialty.
- Debugging and failure analysis.
- Hardware bring-up and test experience.
- Behavioral stories about ownership, failures, and reviews.

### Professional habits

- Write design documents and test plans.
- Review code for correctness, not style alone.
- Run postmortems and root-cause analyses.
- Practice configuration control and change discipline.
- Communicate trade-offs to non-specialists.

## 16. Rust syntax track

Syntax fluency is a separate skill from architecture. It requires short, frequent
recall practice and immediate application.

### 16.1 Syntax ladder

| Rung | Inventory | Drill |
| --- | --- | --- |
| S0 Lexical | Comments, attributes, `let`, `mut`, shadowing, type annotations, literals and suffixes, operators, expression blocks, `const`, `static`, `_` | Predict types of twenty one-liners; write ten from memory |
| S1 Control flow | `if`, `else`, `loop`, `while`, `for`, ranges, labels, `break`, `continue`, `match`, guards, `if let`, `while let`, `let else` | Rewrite an `if` chain as `match`; exhaustive enum match |
| S2 Data and ownership | Structs, enums, `impl`, `self`, `&self`, `&mut self`, `Self`, references, dereference, slices, arrays, tuples, `String`/`&str`, `Vec`, `Option`, `Result`, `?` | Model a domain with enums; fix ten borrow errors |
| S3 Traits and generics | Trait definitions, default methods, bounds, `where`, associated types and constants, `dyn Trait`, `impl Trait`, `Box<dyn Trait>`, conversions, `Default`, `Display`, `Debug`, derives | Implement one trait for two types; convert with `From` |
| S4 Lifetimes | `'a`, `&'a T`, elision rules, structs holding references, `PhantomData`, `'static` | Annotate five functions; explain each annotation |
| S5 Modules and Cargo | `mod`, `pub`, visibility levels, `use`, `as`, crate paths, `lib.rs`/`main.rs`, `Cargo.toml`, features, `cfg`, `cfg_attr`, test modules, integration tests | Split a file into modules; add a feature flag |
| S6 Closures and iterators | Closure syntax, `move`, `Fn`/`FnMut`/`FnOnce`, adapters, `IntoIterator`, `impl Iterator`, `collect`, `HashMap` entry API | Rewrite ten loops as iterator chains |
| S7 Smart pointers | `Box`, `Rc`, `Arc`, `RefCell`, `Cell`, `Mutex`, `RwLock`, `Weak`, `Cow`, `Deref`, `Drop` | `Rc<RefCell<_>>` and `Arc<Mutex<_>>` demos |
| S8 Errors and tests | Error enums, `Display`, `Error`, `?`, `Box<dyn Error>`, test modules, assertions, `should_panic`, `Result` in tests, doc tests | Write an error enum with context; test failure paths |
| S9 Macros | `println!`, `format!`, `vec!`, `matches!`, `write!`, derives, basic `macro_rules!`, attribute and procedural macro concepts | Write a small `macro_rules!` constructor; read a derive |
| S10 Concurrency and async | Threads, `move`, channels, `join`, scoped threads, `Send`/`Sync`, `async`/`await`, `Future`, `tokio::main`, `spawn`, `JoinHandle` | Producer/consumer; a small async task |
| S11 Unsafe | `unsafe`, raw pointers, `extern "C"`, `#[no_mangle]`, `#[repr(C)]`, safety comments | Recognize and explain only; write only with justification |

### 16.2 Fourteen-day syntax bootcamp

Fifteen to twenty minutes per day, in parallel with Level 1.

| Day | Focus | Drill |
| ---: | --- | --- |
| 1 | Lexical | One-liners, shadowing, expression blocks |
| 2 | Control flow | Labels, `loop` with `break` value, ranges |
| 3 | Patterns | Destructure tuples, structs, enums; guards; `let else` |
| 4 | Data | Structs, enums, `impl` from memory |
| 5 | Ownership | Deliberately create and fix ten borrow errors |
| 6 | `Option`/`Result` | Five functions with `?` and a custom error |
| 7 | Review | Rebuild one file from blank |
| 8 | Traits | Trait, two impls, default method |
| 9 | Generics | Bounds, `where`, `impl Trait`, `dyn Trait` |
| 10 | Lifetimes | Annotate five functions; explain elision |
| 11 | Modules and Cargo | Split a file, add a feature, run `cargo doc` |
| 12 | Closures and iterators | Rewrite ten loops; `HashMap` entry API |
| 13 | Pointers, errors, tests | `Rc<RefCell>`, error enum, tests |
| 14 | Macros and async | `macro_rules!`, a small async task |

### 16.3 Must-write-from-memory checklist

Functions, structs, enums, `impl` blocks, `match`, trait and implementation,
generic bounds and `where`, one lifetime annotation, closures, iterator chains,
`Result` with `?`, error enum, `mod`/`use`, test module, Bevy plugin skeleton,
Bevy system signature, `Query`, `Res`, `ResMut`, `Commands`, derives, and
`add_systems` with sets and run conditions.

### 16.4 Compiler-error drills

Learn to read and repair these by name:

- `E0382` use of moved value;
- `E0499` more than one mutable borrow;
- `E0502` immutable borrow while mutable borrow exists;
- `E0596` cannot borrow as mutable;
- `E0106` missing lifetime specifier;
- `E0277` trait bound not satisfied;
- `E0308` mismatched types.

### 16.5 Practice methods

1. Syntax cards: construct, minimal example, mental model, common error, repo
   example.
2. Blank-file drills: no autocomplete, no copy.
3. Compiler-error drills: break, read, repair.
4. Repository corpus: find a construct in this codebase, explain every token,
   then write a minimal version.

## 17. Bevy track

| Rung | Constructs | Drill |
| --- | --- | --- |
| B0 App and plugins | `App`, `Plugin`, `build`, `add_systems`, `add_plugins`, resources, ordering, run conditions | Plugin with two ordered systems |
| B1 ECS signatures | `Commands`, queries, filters, `Res`, `ResMut`, `Local`, `Entity`, derives | Five system signatures, explain every parameter |
| B2 Assets, transforms, time | `Handle`, `Assets`, asset server, `Transform`, `GlobalTransform`, `Time`, `Camera3d`, `Projection`, gizmos | Spawn a mesh and material; draw a gizmo |
| B3 States, messages, UI | `NextState`, `OnEnter`, `in_state`, message and event writers/readers, UI nodes, text | Pause state and a message |
| B4 Advanced | Parallel iteration, tasks, custom materials, shaders | After terrain; not in Level 1 |

Rule: if a system signature can be written and every access explained, the ECS
borrow model is understood.

## 18. Math thread

Mathematics is learned just in time, visually, and code-first. Every concept
gets a math card with six parts:

1. plain-language meaning;
2. geometric picture;
3. formula;
4. Rust function;
5. known test value;
6. common mistake.

| Stage | Topic | First visible use |
| --- | --- | --- |
| M0 | Units, scientific notation, algebra, functions, graphs | Typed units, telemetry |
| M1 | Trigonometry: sine, cosine, radians, identities | Orbits, rotations, sky |
| M2 | 2D and 3D vectors: dot, cross, projection, normalization | Gravity, forces, camera |
| M3 | Derivatives and integrals | Velocity, acceleration, integration |
| M4 | Differential equations and numerical integration | Fixed-step physics, stability |
| M5 | Linear algebra: matrices, basis, determinants, rotations | Frames, quaternions, cameras |
| M6 | Mechanics: Newton, energy, momentum, torque, inertia | Rigid body, contact, landing |
| M7 | Orbital mechanics: Kepler, vis-viva, elements, conics | Propagator, transfers |
| M8 | Thermodynamics and atmosphere: ideal gas, lapse rate, pressure | Density, Mach, drag |
| M9 | Geometry and geodesy: spheres, great circles, ellipsoids | Cube-sphere, launch sites |
| M10 | Numerical methods: root finding, interpolation, error, statistics | Guidance, validation |

Rule: if a formula appears in code and cannot be drawn or derived, stop and make
the card.

## 19. Flight-grade engineering track

These gates run across all levels and distinguish a simulator developer from a
trusted mission-software engineer.

| Gate | Practice | Artifact |
| --- | --- | --- |
| G1 | Units, determinism, reproducibility | Fixed-step golden test in every project |
| G2 | Requirements to tests traceability | Matrix mapping each requirement to a test |
| G3 | Fault tolerance and FDIR | Fault-injection scenarios and safe-mode behavior |
| G4 | Real-time and resource budgets | CPU, memory, stack, timing evidence |
| G5 | Verification and statistics | Monte Carlo dispersion, sensitivity, tolerances |
| G6 | Configuration management and reviews | Versioned baselines, change log, review checklist |
| G7 | Safety toolchain and supply chain | Static analysis, fuzzing, SBOM, qualified-toolchain awareness |

G1 and G2 start in Level 1. G3 to G5 are added in Levels 3 and 4. G6 and G7
become habits by Level 5.

## 20. Project spine

| Project | Level | Mathematics | Rust and Bevy | Gate |
| --- | --- | --- | --- | --- |
| 2D orbital sandbox | 1 | Vectors, trig, gravity, integration | Rust basics, Bevy B0-B1 | Golden test, third body |
| Typed units and config | 2 | Units, validation | Newtypes, serde, errors | Invalid config rejected |
| Deterministic N-body | 2 | Integration, floating point | Testing, golden files | Byte-identical runs |
| 3D solar system | 2 | Quaternions, frames | Bevy B1-B2, plugins | Selectable bodies, fixed clock |
| Frame and precision library | 2 | Linear algebra, geodesy | Property tests | Round-trip and known values |
| 6-DOF rigid body | 2 | Inertia, torque, angular velocity | ECS composition | Analytical checks pass |
| Heightfield contact | 2 | Impulses, friction, contact | Fixed step, diagnostics | Stable landing |
| `no_std` sensor loop | 2 | Filters, bounded control | Embedded Rust | Runs without std |
| Orbital propagator | 3 | Kepler, conics, J2 | Force-model tiers | Reference comparison |
| Time and ephemeris | 3 | UTC/TDB, Julian date | Data validation | State validation |
| Atmosphere and aero | 3 | ISA, Mach, drag, lift | Pure domain functions | Plausible coefficients |
| Propulsion and staging | 3 | Rocket equation, mass flow | Component design | Budget accuracy |
| Ascent simulation | 3 | Dynamics, guidance | Headless execution | Baseline match |
| GNC stack | 3 | Control, allocation | Set ordering, state machines | Multi-site success |
| Entry and recovery | 3 | Heating, ballistic coefficient | Messages, lifecycle | Touchdown behavior |
| Terrain viewer | 3 | Spherical geometry, LOD | Bevy assets, streaming | Visual and timing evidence |
| EKF and Monte Carlo | 3 | Estimation, covariance, statistics | Numerics, parallelism | Consistency and dispersion |
| Real-time FDIR loop | 4 | Control, fault logic | `no_std`, RTIC/Embassy | Fault injection survives |
| CCSDS codec | 4 | Checksums, framing | Binary encoding, fuzzing | Malformed input rejected |
| Verification harness | 4 | Properties, bounds | proptest, Kani, fuzzing | Property report |
| Interop layer | 4 | - | FFI, cxx, PyO3 | Cross-language tests |

## 21. Capstones

Choose two or three, in a worktree, end to end.

1. Solar radiation pressure as a `ForceModelTier` term: domain mathematics,
   adapter, CLI key, baseline, documentation.
2. New telemetry channel: authoritative value, derived telemetry, HUD,
   recorder, CSV export.
3. New guidance phase: different terminal law with a deterministic headless
   test.
4. New terrain layer: catalog, streaming, render, collision, with visual and
   performance evidence.
5. Performance investigation: profile, hypothesize, measure, optimize, keep or
   revert.
6. Architectural refactor: remove duplicated rocket resource registration and
   prove all modes and baselines still pass.

Acceptance follows the repository's definition of done: coherent architecture,
existing behavior intact, tests pass, physics validated, no duplicate
implementation, acceptable performance, errors handled, documentation updated,
and all three modes run.

## 22. Repository module map

The architecture tour remains available and is used in dual-track mode: visit a
module after the corresponding lab unit, so production decisions are recognized
rather than memorized.

| Module | Subject | Roadmap level |
| ---: | --- | --- |
| 1 | App shell and scheduling | Level 1-2 |
| 2 | Time authority | Level 3 |
| 3 | Ephemeris and bodies | Level 3 |
| 4 | Frames and precision | Level 2 |
| 5 | Gravity and orbits | Level 2-3 |
| 6 | Solar presentation and rendering | Level 1-2 |
| 7 | Terrain domain I | Level 3 |
| 8 | Terrain domain II | Level 3 |
| 9 | Terrain runtime | Level 3 |
| 10 | Vehicle and setup | Level 2-3 |
| 11 | Flight physics | Level 2-3 |
| 12 | Guidance | Level 3 |
| 13 | Rocket runtime and mission | Level 3 |
| 14 | Craft, UI, and education | Level 2 |
| 15 | Validation and tooling | Level 4-5 |

## 23. Career strategy

### Target roles

- GNC engineer;
- flight software engineer;
- simulation and test engineer;
- autonomy engineer;
- ground software engineer;
- avionics or embedded software engineer.

### Standards and references to know by name

- NASA NPR 7150.2, NASA-STD-8719.13, NASA software engineering handbook, JPL
  institutional coding standard;
- cFS (NASA) and F Prime (JPL);
- CCSDS space packet standards;
- DO-178C, ISO 26262, IEC 61508, and Ada/SPARK as safety benchmarks;
- SPICE, GMAT, Orekit, Astropy, poliastro, Tudat, Basilisk, and 42 as tools and
  comparison sources.

### Portfolio rules

- Depth over breadth: two or three deep projects, not ten demos.
- Every project has design, tests, measurement, and limitations.
- Public write-ups of diagnosed failures are often more convincing than success
  demos.
- Open-source contributions show collaboration and review experience.

### Networking

- Open-source communities and conferences.
- Local rocketry, robotics, and CubeSat teams.
- Internships and research projects.
- Technical writing and talks.

### Constraints to check early

- Citizenship or export-control requirements.
- Security clearance.
- Degree expectations for the chosen subspecialty.
- Location and travel.

## 24. Validation and evidence

Every project runs the applicable checks:

- `cargo fmt --check`;
- `cargo check`;
- `cargo clippy -- -D warnings`;
- `cargo test`;
- `cargo build --release` for release-oriented work;
- headless or deterministic run for simulation behavior;
- visual or measurement evidence when presentation changes.

For Cosmic Systems capstones, also run:

- `cargo run`;
- `cargo run -- craft`;
- `cargo run -- rocket`;
- the relevant baseline or scientific validation.

No claim of success without running the relevant check.

## 25. Pacing and review

- Sessions are 45 to 90 minutes.
- Units are completed by artifact and gate, not by date.
- Every four to six sessions, run a review: re-test an older skill, inspect the
  portfolio, and choose the next unit.
- If a week produces no artifact, it produced recognition, not skill.
- If motivation drops, switch to a more visual unit; do not abandon the plan.

Suggested rhythms:

- relaxed: two sessions per week;
- focused: four sessions per week;
- intensive: daily sessions with one rest day.

## 26. Anti-patterns

- Reading the repository top to bottom before building anything.
- Copying production code into the lab.
- Following tutorials for a different Bevy version.
- Skipping mathematics and calling a library instead.
- Building a feature before writing its test.
- Using generated code that cannot be explained.
- Optimizing before measuring.
- Adding dependencies to feel productive.
- Rewriting working systems to look sophisticated.
- Treating compilation as completion.
- Collecting courses instead of finishing projects.

## 27. Reading list

Rust:

- The Rust Programming Language;
- Rust for Rustaceans;
- Programming Rust;
- Rust by Example;
- Rustlings and 100 Exercises to Learn Rust.

Bevy:

- official Bevy book and examples for 0.17;
- Bevy cheat book, checking the version;
- `cargo doc --open`.

Mathematics:

- 3Blue1Brown series on linear algebra and calculus;
- Mathematics for 3D Game Programming and Computer Graphics.

Physics and mechanics:

- Classical Mechanics, Taylor;
- Game Physics Engine Development, Millington.

Astrodynamics:

- Orbital Mechanics for Engineering Students, Curtis;
- Fundamentals of Astrodynamics, Bate, Mueller, and White;
- Analytical Mechanics of Space Systems, Schaub and Junkins.

Estimation and control:

- Estimation with Applications to Tracking and Navigation, Bar-Shalom;
- Space Vehicle Dynamics and Control, Wie;
- Optimal State Estimation, Simon.

Graphics:

- Real-Time Rendering;
- Physically Based Rendering, as a reference.

Engineering:

- Game Engine Architecture, Gregory;
- A Philosophy of Software Design, Ousterhout;
- NASA software engineering handbook and NPR 7150.2.

Repository:

- `AGENTS.md` as an architecture curriculum;
- `docs/progress/MASTER_ROADMAP.md` as an evidence-based planning example;
- `docs/scientific_solar_system_fidelity.md` for scientific limits.

## 28. Next actions

1. Run the Rust, mathematics, and physics diagnostics; record the results.
2. Create the lab workspace and the portfolio log.
3. Start the fourteen-day syntax bootcamp.
4. Start Unit 1.1: `Vec2` and trigonometry.
5. Add G1 and G2 to Unit 1: a fixed-step golden test and a requirements-to-test
   note.
6. Schedule the first review after six sessions.

## Appendix A - Session template

```text
Date:
Unit:
Prediction:
Concept and math card:
What I implemented:
Test and known value:
What surprised me:
What I can now do:
Next question:
```

## Appendix B - Architecture orientation

The simulator is one Bevy application with three compositions:

```text
main.rs
  parse CLI
  SharedSimulationPlugin
    time, ephemeris, solar bodies, terrain plumbing,
    performance, screenshots, starfield
    SolarSystemModePlugin
    CraftModePlugin
    RocketModePlugin
      RocketFixedSimulationPlugin (also used headless)
```

Five rules explain most file placement:

1. Domain is authoritative and Bevy-free; f64 SI units.
2. One authority per quantity: ephemeris, frames, gravity, time, terrain.
3. Simulation runs at a fixed 64 Hz; warp queues steps; rendering interpolates.
4. Two spatial worlds: solar display units and flight meters with a render
   origin.
5. Presentation observes; it never writes physics.

Authorities to locate:

| Authority | Location |
| --- | --- |
| Time | `src/domain/services/simulation_time.rs` |
| Ephemeris | `src/domain/services/ephemeris/` |
| Frames | `src/domain/services/reference_frames.rs` |
| Gravity | `src/domain/services/gravity.rs` |
| Terrain | `src/domain/services/terrain_source/` |
| Vehicle configuration | `src/application/rocket_config/` |
| Fixed pipeline order | `src/infrastructure/bevy_adapters/rocket/sets.rs` |

## Appendix C - Portfolio log template

```text
Project:
Level:
Purpose:
Design decisions:
Mathematics:
Tests:
Measurement:
Known limitations:
What I can now do:
Review notes:
```

## Appendix D - Requirements-to-test template

```text
Requirement:
Rationale:
Verification method:
Test or evidence:
Pass criterion:
Residual risk:
```

## Appendix E - Command reference

```text
cargo fmt --check
cargo check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run
cargo run -- craft
cargo run -- rocket
cargo doc --open
cargo tree
```

## Appendix F - Glossary

- ECS: entity-component-system.
- Fixed step: simulation advances by a constant timestep.
- Warp: requested simulation-time acceleration represented as queued steps.
- Determinism: identical inputs produce identical outputs.
- FDIR: fault detection, isolation, and recovery.
- GNC: guidance, navigation, and control.
- HIL/SIL: hardware-in-the-loop and software-in-the-loop.
- CCSDS: Consultative Committee for Space Data Systems.
- TDB: barycentric dynamical time.
- ICRF: International Celestial Reference Frame.
- LOD: level of detail.
- WCET: worst-case execution time.
- SBOM: software bill of materials.
