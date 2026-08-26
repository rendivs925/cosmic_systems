## ADDED Requirements

### Requirement: Rocket mode has a playable game entry flow
Rocket mode SHALL open its game flow before a mission flight begins while continuing to compose the shared solar-system infrastructure and keeping normal and craft modes unchanged.

#### Scenario: Start rocket game mode
- **WHEN** a player runs the rocket mode
- **THEN** the application presents the rocket game entry flow instead of immediately committing the player to a flight

#### Scenario: Preserve non-rocket modes
- **WHEN** a user runs the default solar-system mode or craft mode
- **THEN** the rocket game flow and its game-specific systems are not registered
