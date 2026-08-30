## Purpose

Provides state-driven rocket flight effects and animation that improve visual
readability without becoming an alternate simulation or physics authority.

## ADDED Requirements

### Requirement: Flight effects derive from authoritative presentation state
The system SHALL derive rocket visual effects and animation from authoritative
fixed-state snapshots, lifecycle state, and explicit presentation configuration.
Effects MUST NOT modify vehicle dynamics, force accumulation, propellant,
simulation time, or coordinate authorities.

#### Scenario: Engine effect follows propulsion state
- **WHEN** an engine's authoritative state changes throttle or operating state
- **THEN** its visible effect updates from that state without changing engine thrust or propellant consumption

#### Scenario: Presentation runs between fixed ticks
- **WHEN** rendering runs at a different cadence from fixed simulation
- **THEN** effect animation uses presentation interpolation and does not create or alter a fixed simulation step

### Requirement: Flight lifecycle has coherent visual feedback
The system SHALL present visible, bounded feedback for engine operation,
atmospheric flight, staging, and recovery states when the corresponding
authoritative state is present.

#### Scenario: Stage separation feedback
- **WHEN** an authoritative stage-separation event occurs
- **THEN** the presentation shows separation feedback associated with the affected vehicle entities

#### Scenario: No fabricated lifecycle effect
- **WHEN** no authoritative lifecycle transition or eligible state exists
- **THEN** the presentation does not display the corresponding transition effect

### Requirement: Presentation quality is configurable and isolated
The system SHALL allow effect quality to be reduced or disabled without changing
physics, simulation time, authoritative state, or application mode selection.

#### Scenario: Reduced quality
- **WHEN** effect quality is reduced
- **THEN** presentation work is reduced while rocket trajectory and fixed-step results remain unchanged
