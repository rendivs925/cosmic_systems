## Purpose

Preserves a player's game progress and vehicle creations across local sessions without making save data an authority over simulation.

## ADDED Requirements

### Requirement: Game progress persists locally
The system SHALL save and restore player settings, unlocked parts, completed mission results, and saved vehicle builds using a versioned local save format.

#### Scenario: Resume a later session
- **WHEN** a player exits after completing missions or saving a vehicle build and launches the game again
- **THEN** the saved progress and build are available before the next mission begins

### Requirement: Invalid save data is recoverable
The system SHALL validate loaded save data and preserve the player's ability to start a new profile when a save is missing, corrupt, or from an incompatible version.

#### Scenario: Corrupt save file
- **WHEN** the game cannot validate a local save file
- **THEN** it reports the issue, retains a recoverable backup when possible, and starts with a new-profile option instead of crashing

### Requirement: Active physical flight is not silently restored
The MVP SHALL persist only pre-flight game state and completed-flight results; it MUST NOT silently restore an in-progress authoritative simulation state.

#### Scenario: Exit during flight
- **WHEN** a player exits during an active mission
- **THEN** the game warns that the active flight will be abandoned and does not claim it can resume exactly
