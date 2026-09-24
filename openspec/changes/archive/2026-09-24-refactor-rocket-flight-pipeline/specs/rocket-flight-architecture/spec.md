## Purpose

Defines typed rocket-flight boundaries so domain calculations, ECS execution, and presentation share physical state without duplicate authority.

## ADDED Requirements

### Requirement: Rocket flight uses typed celestial-body bindings
Rocket flight interfaces SHALL identify their bound celestial body with a validated domain identifier rather than raw strings.

#### Scenario: Bound body lookup
- **WHEN** a rocket system resolves its bound celestial body
- **THEN** it uses the same typed identifier held by the rocket and the celestial-body registry

#### Scenario: Invalid body configuration
- **WHEN** vehicle or launch configuration names an unknown celestial body
- **THEN** startup validation reports the invalid configuration before simulation begins

### Requirement: Flight conditions have one authority
The system SHALL derive atmosphere-relative velocity, dynamic pressure, Mach, and ambient pressure once per fixed flight tick for each vehicle, and all flight consumers SHALL use that result.

#### Scenario: Consistent flight condition consumers
- **WHEN** guidance, aerodynamic forces, propulsion, telemetry, or entry physics evaluate one vehicle in a fixed tick
- **THEN** they observe flight-condition values derived from the same authoritative state and atmosphere sample

### Requirement: Presentation is isolated from simulation
Camera and render-transition state SHALL remain presentation-only and SHALL NOT mutate authoritative rocket motion, force, mass, or mission state.

#### Scenario: Camera transition during flight
- **WHEN** the user changes camera mode while the vehicle is moving
- **THEN** the camera transition changes only rendered camera state and the rocket simulation continues unchanged
