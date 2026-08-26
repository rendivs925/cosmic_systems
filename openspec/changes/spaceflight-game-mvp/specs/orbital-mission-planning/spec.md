## Purpose

Lets players understand a flight's predicted orbit, plan bounded orbital maneuvers, and execute them without replacing physical simulation.

## ADDED Requirements

### Requirement: Players can read an orbital map
The system SHALL present the active vehicle's predicted trajectory, body-relative apoapsis and periapsis, altitude, velocity, inclination, and impact state in a dedicated orbital view.

#### Scenario: Bound elliptical orbit
- **WHEN** the active vehicle has a bound non-circular orbit
- **THEN** the orbital view displays analytic apoapsis and periapsis markers at their physical planet-centered positions and labels their altitudes

#### Scenario: Impacting trajectory
- **WHEN** the predicted trajectory intersects the body's surface
- **THEN** the orbital view displays the impact path and does not present post-impact apsis information as reachable

### Requirement: Players can create and execute an MVP maneuver plan
The system SHALL let players place one planned prograde, retrograde, normal, anti-normal, radial-in, or radial-out maneuver at a future point on a valid predicted trajectory.

#### Scenario: Plan a circularization burn
- **WHEN** a player places a prograde or retrograde maneuver and adjusts its delta-v
- **THEN** the map updates the predicted post-burn trajectory and reports the planned burn time and delta-v

#### Scenario: Execute a maneuver
- **WHEN** the vehicle reaches the maneuver execution window and the player executes the plan or enables maneuver assistance
- **THEN** the vehicle changes trajectory only through its available propulsion and the plan records its achieved delta-v

### Requirement: Time warp protects active flight
The system SHALL prevent unsafe high time warp while a player is actively piloting, inside atmosphere, near terrain contact, or executing a maneuver.

#### Scenario: Unsafe time-warp request
- **WHEN** a player requests a prohibited time-warp rate
- **THEN** the system selects the highest safe rate and explains the constraint in the HUD
