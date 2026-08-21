# Rocket Mode Specification

## Purpose

Defines the `cargo run -- rocket` application mode: how it is selected, how it composes the shared solar-system simulation, and which rocket-only systems it activates, without altering the existing solar-system or UFO modes.

## Requirements

### Requirement: Rocket mode is selectable via CLI

The system SHALL provide a `rocket` application mode selected by `cargo run -- rocket`.

#### Scenario: Launch rocket mode

- **WHEN** a user runs `cargo run -- rocket`
- **THEN** the application initializes the rocket flight mode and opens a window titled for rocket flight

#### Scenario: Solar mode remains default

- **WHEN** a user runs `cargo run` with no mode argument
- **THEN** the application initializes the normal solar-system simulation

#### Scenario: Craft mode is preserved

- **WHEN** a user runs `cargo run -- craft`
- **THEN** the application initializes the existing UFO/craft mode with its prior behavior

#### Scenario: Unknown mode argument

- **WHEN** a user passes a mode argument that is not `rocket`, `craft`, or `gyro`
- **THEN** the application falls back to the default solar-system mode and logs a warning

### Requirement: Rocket mode reuses shared solar-system infrastructure

The rocket mode SHALL compose the shared solar-system world (celestial bodies, gravity source data, rendering, camera plumbing, assets) rather than constructing a separate scene.

#### Scenario: Planets available in rocket mode

- **WHEN** rocket mode starts
- **THEN** the solar-system planets and moons are spawned in the shared world

#### Scenario: No duplicated world

- **WHEN** rocket mode starts
- **THEN** the application does not create a second, independent copy of the solar-system simulation

### Requirement: Rocket systems are isolated to rocket mode

Rocket-specific systems (physics, controls, camera, telemetry) SHALL be registered only in rocket mode and MUST NOT run in solar or craft modes.

#### Scenario: Rocket systems absent from solar mode

- **WHEN** the application runs in solar mode
- **THEN** rocket physics and control systems are not registered and do not execute

#### Scenario: Rocket systems absent from craft mode

- **WHEN** the application runs in craft mode
- **THEN** rocket physics and control systems are not registered and do not execute

### Requirement: Mode selection is explicit and robust

Mode selection SHALL use an explicit parseable mode value rather than substring matching of raw arguments.

#### Scenario: Argument ordering is irrelevant

- **WHEN** a user passes the mode token in any position among other arguments
- **THEN** the mode is still recognized correctly

#### Scenario: No accidental substring match

- **WHEN** a non-mode argument contains the substring `craft` or `rocket`
- **THEN** it is not treated as a mode selector