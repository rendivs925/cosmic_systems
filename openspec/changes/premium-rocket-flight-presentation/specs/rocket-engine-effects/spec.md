## Purpose

Provides bounded, physically informed engine, ground-interaction, and lifecycle effects that make Rocket propulsion state legible without simulating fluid dynamics or altering flight authority.

## ADDED Requirements

### Requirement: Engine effects follow active propulsion state
The system SHALL render engine effects only for engines eligible to run in the authoritative propulsion state and SHALL derive their intensity and expansion from actual thrust-related state, throttle, and ambient atmospheric conditions.

#### Scenario: Engine ignition
- **WHEN** an authoritative engine transitions from off to running with nonzero throttle
- **THEN** its visual effect transitions from inactive to active with bounded intensity

#### Scenario: Vacuum expansion
- **WHEN** the active engine operates at lower ambient pressure with equivalent throttle
- **THEN** its plume representation expands relative to its sea-level representation

### Requirement: Engine effects remain attached through presentation lifecycle changes
The system SHALL place engine effects at catalog engine stations using the rocket's presentation hierarchy and SHALL rebuild or remove them on staging, separation, and relaunch.

#### Scenario: Stage separation
- **WHEN** a stage separation changes the active vehicle engine set
- **THEN** effects for removed engines are no longer attached to the active vehicle and effects for the active engine set remain correctly located

### Requirement: Ground interaction uses terrain-relative state
The system SHALL gate launch ground effects from authoritative terrain-relative distance, propulsion state, and available surface classification. Ground-effect presentation MUST NOT provide collision or force input.

#### Scenario: Liftoff near terrain
- **WHEN** active engines produce thrust near the authoritative terrain surface
- **THEN** bounded ground-effect presentation is enabled at the terrain-relative launch area

#### Scenario: Ascent away from terrain
- **WHEN** the rocket moves beyond the configured ground-interaction distance
- **THEN** ground-effect presentation fades out without altering terrain or rocket state
