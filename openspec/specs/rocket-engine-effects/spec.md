# rocket-engine-effects Specification

## Purpose
Provides bounded, physically informed engine, ground-interaction, and lifecycle effects that make Rocket propulsion state legible without simulating fluid dynamics or altering flight authority.
## Requirements
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

### Requirement: Plume intensity follows burned propellant

Engine effect intensity SHALL derive from the propellant mass flow actually
burned in the authoritative fixed tick, relative to the full-throttle mass flow
of the same running engines. It SHALL NOT treat an engine as either fully on or
fully off, and it SHALL NOT use the entry-flow force multiplier, which reduces
net axial force without reducing combustion.

#### Scenario: Propellant-limited final tick

- **WHEN** the active stage can only deliver part of a tick's mass flow
- **THEN** the plume fades continuously with the delivered fraction

#### Scenario: Reduced throttle

- **WHEN** the learned or commanded throttle is below full
- **THEN** the plume intensity scales with the resulting mass-flow fraction

#### Scenario: Entry-flow force reduction

- **WHEN** an entry-flow effectiveness multiplier reduces the applied axial force
- **THEN** a burning engine still presents its full plume

### Requirement: Plume length follows expansion

Plume length SHALL grow with the pressure-derived expansion ratio as well as
with the burned mass-flow fraction. The core layer SHALL stay anchored near the
nozzle; the inner and outer layers SHALL carry the visible altitude response.

#### Scenario: Vacuum expansion

- **WHEN** the same engine burns at lower ambient pressure
- **THEN** its outer plume is both wider and longer than at sea level

### Requirement: Plume direction follows the gimbaled thrust axis

Each engine effect SHALL point its exhaust opposite the engine's authoritative
gimbaled thrust direction, using the same clamp and direction functions as the
force path. A plume SHALL NOT remain aligned with the body axis when the engine
is gimbaled.

#### Scenario: Gimbaled engine

- **WHEN** the propulsion state applies a nonzero gimbal deflection
- **THEN** the plume tilts with the thrust vector and stays anchored at the
  engine station

### Requirement: Recovering hardware presents its own effects

A recovering stage that flies its own authoritative propulsion pipeline SHALL
expose its own burned mass flow and SHALL present its own engine effects. A
recovering stage SHALL NOT borrow the primary vehicle's propulsion state.

#### Scenario: Recovery burn

- **WHEN** a recovering first stage throttles up for a landing burn
- **THEN** its plume intensity follows its own mass flow, independently of the
  upper stage

