## Purpose

Lets players create a valid, previewable launch vehicle from a deliberately small MVP part catalog before launching a mission.

## ADDED Requirements

### Requirement: Players can assemble an MVP launch vehicle
The system SHALL let players create, name, edit, duplicate, and delete a vehicle build composed only of unlocked compatible parts from the MVP catalog.

#### Scenario: Valid vehicle build
- **WHEN** a player combines a command capsule, a payload, tanks, engines, and stage separators into a valid stack
- **THEN** the assembly screen displays the resulting mass, thrust-to-weight ratio, staged delta-v estimate, and launch-ready status

#### Scenario: Invalid vehicle build
- **WHEN** a player creates a build with an incompatible attachment, missing control path, or no powered launch stage
- **THEN** the system identifies the invalid connection or requirement and prevents mission launch

### Requirement: Builds use the authoritative vehicle definition
The system SHALL validate and convert a launchable player build through the same authoritative vehicle configuration rules used by spawned rocket flight.

#### Scenario: Launch selected build
- **WHEN** a player selects a valid build and starts a mission
- **THEN** the spawned vehicle's mass, stages, engines, propellant, and physical limits match the validated build

### Requirement: Players can inspect their vehicle before flight
The system SHALL provide a rotatable, zoomable 3D vehicle preview and an ordered staging view before launch.

#### Scenario: Inspect staging order
- **WHEN** a player selects a stage in the assembly view
- **THEN** the preview highlights its attached parts and the staging view identifies the events that occur when it activates
