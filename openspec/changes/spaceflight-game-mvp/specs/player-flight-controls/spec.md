## Purpose

Gives desktop players responsive direct control of a rocket while preserving optional assists and the simulation's authoritative physics pipeline.

## ADDED Requirements

### Requirement: Players can issue direct flight commands
The system SHALL allow the player to command throttle, pitch, yaw, roll, RCS translation, staging, fairing deployment, landing gear, parachutes, and time warp during a flight when the relevant vehicle capability is available.

#### Scenario: Player commands throttle and attitude
- **WHEN** a player provides supported throttle or attitude input during flight
- **THEN** the commanded values are reflected in the flight HUD and take effect only through the vehicle's physical actuator limits

#### Scenario: Unavailable action
- **WHEN** a player requests an action unavailable on the active vehicle or in its current state
- **THEN** the system leaves the vehicle unchanged and provides a clear reason to the player

### Requirement: Players can choose an assistance level
The system SHALL offer direct, assisted, and autopilot flight modes with an explicit visible mode indicator.

#### Scenario: Assisted flight
- **WHEN** a player enables an assistance mode
- **THEN** the assistant stabilizes or follows the selected target while player commands retain the documented authority for that mode

#### Scenario: Direct flight
- **WHEN** a player selects direct flight mode
- **THEN** no guidance system replaces the player's valid throttle or attitude commands

### Requirement: Flight controls are usable on desktop
The system SHALL expose discoverable keyboard-and-mouse bindings, pause, time-warp controls, sensitivity settings, and rebinding for all MVP flight actions.

#### Scenario: View controls
- **WHEN** a player opens the in-flight controls help
- **THEN** the system displays the active bindings and the actions currently constrained by flight state
