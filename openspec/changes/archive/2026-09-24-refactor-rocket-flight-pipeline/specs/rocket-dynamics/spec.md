## ADDED Requirements

### Requirement: Completed fixed ticks share a simulation epoch
The fixed flight pipeline SHALL advance the authoritative simulation epoch immediately after integration and before post-integration terrain, orbital, render-capture, and telemetry consumers execute.

#### Scenario: Post-integration terrain sample
- **WHEN** a fixed tick integrates motion on a rotating body
- **THEN** terrain contact samples the body-fixed surface at the completed tick epoch

#### Scenario: Post-integration telemetry
- **WHEN** a fixed tick completes
- **THEN** recorded telemetry and orbital elements describe the integrated state at that tick's epoch

### Requirement: Pause gates fixed flight simulation
The system SHALL not run fixed flight simulation stages or advance the simulation epoch while simulation time is paused.

#### Scenario: Paused powered vehicle
- **WHEN** simulation time is paused while engines are active
- **THEN** position, velocity, attitude, mass, propellant, and simulation time remain unchanged until unpaused
