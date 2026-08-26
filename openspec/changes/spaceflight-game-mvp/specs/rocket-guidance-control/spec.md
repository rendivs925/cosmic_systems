## ADDED Requirements

### Requirement: Command ownership is explicit
The system SHALL give each actuator command an explicit owner selected from player input, an enabled assist, or autopilot guidance, and SHALL resolve ownership before control and actuation.

#### Scenario: Player direct-command ownership
- **WHEN** direct flight mode is active and the player provides a valid actuator command
- **THEN** guidance does not overwrite that command and control/actuation apply only the vehicle's physical limits

#### Scenario: Assistance ownership
- **WHEN** an enabled assist owns a documented command axis
- **THEN** it may supply that axis while retaining any player-owned axes unchanged

#### Scenario: Autopilot ownership
- **WHEN** autopilot mode is active
- **THEN** guidance owns the documented command axes until the player changes mode or disables the autopilot
