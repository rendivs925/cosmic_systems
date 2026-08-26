## Purpose

Makes rocket mode understandable and satisfying as a desktop game through clear flow, camera control, flight feedback, and failure recovery.

## ADDED Requirements

### Requirement: Rocket mode provides a game flow
The system SHALL present title, profile, vehicle assembly, mission briefing, flight, pause, and debrief states without affecting solar-system or craft modes.

#### Scenario: Enter a mission
- **WHEN** a player starts rocket mode and selects a mission
- **THEN** the system guides the player through vehicle selection and briefing before entering flight

### Requirement: Flight HUD communicates decisive state
The system SHALL display altitude, velocity, throttle, propellant, stage, flight assistance, mission objective, time warp, warnings, and major vehicle events during flight.

#### Scenario: Stage event
- **WHEN** staging, payload deployment, landing gear deployment, or parachute deployment occurs
- **THEN** the HUD confirms the event and updates the relevant vehicle state

### Requirement: Players can control the flight camera
The system SHALL offer chase, cockpit, orbital map, and free-look cameras, with a clear indication of the selected camera and no camera state used as physics authority.

#### Scenario: Switch camera during flight
- **WHEN** a player selects another supported flight camera
- **THEN** presentation changes immediately while the authoritative vehicle trajectory continues unchanged

### Requirement: Failure supports fast learning and retry
The system SHALL pause or conclude terminal vehicle failure with a concise explanation and let the player restart the current mission from its briefing.

#### Scenario: Terrain impact
- **WHEN** the active mission vehicle crashes on terrain
- **THEN** the game presents the crash result, relevant flight metrics, and a retry action
