## Purpose

Provides a compact Earth mission sequence that teaches construction, ascent, orbit, payload deployment, entry, and recovery through authoritative flight outcomes.

## ADDED Requirements

### Requirement: MVP provides an Earth mission sequence
The system SHALL provide four ordered missions: reach space, establish a safe low Earth orbit, deploy a satellite payload, and recover a capsule by atmospheric entry and parachute landing.

#### Scenario: Start unlocked mission
- **WHEN** a player selects an unlocked mission
- **THEN** the briefing states its objective, constraints, available vehicle parts, and success criteria before launch

#### Scenario: Locked mission
- **WHEN** a player selects a mission whose prerequisite is incomplete
- **THEN** the system displays its prerequisite and prevents launch until it is met

### Requirement: Mission results derive from authoritative state
The system SHALL determine objective completion and failure from physical simulation state, vehicle events, and deployed payload state rather than presentation transforms or player-declared outcomes.

#### Scenario: Safe-orbit completion
- **WHEN** a mission requires low Earth orbit and the vehicle reaches the defined minimum periapsis, maximum apoapsis, and orbit stability conditions
- **THEN** the mission marks the orbit objective complete

#### Scenario: Failed recovery
- **WHEN** a recovery mission's crew capsule enters a terminal crashed state
- **THEN** the mission records the failure and offers retry without awarding completion

### Requirement: Players receive a flight debrief
The system SHALL show mission success or failure, objective state, flight time, maximum altitude, achieved orbit, payload status, recovery result, and a score after a mission ends.

#### Scenario: Successful payload deployment
- **WHEN** a player deploys the required payload into the mission's accepted orbit
- **THEN** the debrief records the payload result and awards the mission reward
