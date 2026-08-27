# entry-physics Specification

## Purpose

Models aerothermal heating, ablation, plasma blackout, parachute deployment, and supersonic retro-propulsion during atmospheric entry so reentry physics is physically coherent from entry interface to touchdown.

## Requirements

### Requirement: Stagnation-point convective heating

The system SHALL compute convective heat flux at the stagnation point using a validated engineering correlation (e.g., Fay-Riddell, Sutton-Graves) from velocity, density, nose radius, and wall temperature.

#### Scenario: Heating peak at max q

- **WHEN** the vehicle traverses the peak dynamic pressure region
- **THEN** convective heat flux peaks and then decays as velocity drops

#### Scenario: Nose radius effect

- **WHEN** nose radius increases
- **THEN** peak heat flux decreases proportionally to 1/sqrt(R_nose)

### Requirement: Radiative heating

The system SHALL compute radiative heat flux for high-velocity entries (lunar return, Mars entry) using an engineering approximation from velocity and density.

#### Scenario: Lunar return radiative dominance

- **WHEN** entry velocity exceeds approximately 10 km/s
- **THEN** radiative heat flux becomes comparable to or exceeds convective flux

### Requirement: Ablation mass loss

The system SHALL model ablative TPS recession and mass loss from integrated heat load, updating vehicle mass and surface geometry.

#### Scenario: TPS recession

- **WHEN** cumulative heat load exceeds the material's heat of ablation
- **THEN** the TPS surface recedes and vehicle mass decreases

#### Scenario: Shape change effect

- **WHEN** the nose radius increases due to ablation
- **THEN** subsequent heat flux calculations use the updated radius

### Requirement: Plasma communications blackout

The system SHALL detect plasma blackout conditions (electron density > critical frequency) and signal comms loss to dependent systems.

#### Scenario: Blackout onset

- **WHEN** electron density around the vehicle exceeds the critical density for the comm frequency
- **THEN** the system reports blackout active

#### Scenario: Blackout clearance

- **WHEN** velocity and density drop below blackout threshold
- **THEN** the system reports comms restored

### Requirement: Parachute deployment and drag

The system SHALL model mortar deployment, reefing stages, and inflation dynamics for drogue and main parachutes, applying drag forces to the 6-DOF accumulator.

#### Scenario: Drogue deployment

- **WHEN** the vehicle reaches drogue deployment Mach/altitude
- **THEN** a mortar fires, the drogue inflates through reefing stages, and drag increases

#### Scenario: Main deployment

- **WHEN** the vehicle reaches main deployment altitude
- **THEN** the main parachute deploys, reefs, and inflates to terminal descent velocity

#### Scenario: Parachute drag in accumulator

- **WHEN** parachutes are inflated
- **THEN** their drag forces are added to the translational force accumulator for 6-DOF integration

### Requirement: Supersonic retro-propulsion

The system SHALL model engine plume interaction with the supersonic freestream (plume-induced separation, base pressure changes) during powered descent initiation.

#### Scenario: Retro-propulsion at supersonic speed

- **WHEN** main engines ignite above Mach 1
- **THEN** the system computes effective thrust and base pressure modification from plume-freestream interaction
