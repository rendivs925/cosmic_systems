## MODIFIED Requirements

### Requirement: Rocket systems are isolated to rocket mode
Rocket-specific systems (physics, controls, camera, telemetry, and flight
presentation effects) SHALL be registered only in rocket mode and MUST NOT run
in solar or craft modes.

#### Scenario: Rocket systems absent from solar mode

- **WHEN** the application runs in solar mode
- **THEN** rocket physics, control, and flight-presentation systems are not registered and do not execute

#### Scenario: Rocket systems absent from craft mode

- **WHEN** the application runs in craft mode
- **THEN** rocket physics, control, and flight-presentation systems are not registered and do not execute
