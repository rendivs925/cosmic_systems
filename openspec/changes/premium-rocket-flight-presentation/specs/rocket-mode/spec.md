## ADDED Requirements

### Requirement: Rocket presentation systems remain mode-isolated
Rocket-specific visual and audio presentation systems SHALL be registered only in Rocket mode and SHALL consume the shared simulation and rendering infrastructure rather than constructing a separate flight world.

#### Scenario: Rocket presentation in Rocket mode
- **WHEN** the application starts in Rocket mode
- **THEN** Rocket presentation systems are available after the shared world and Rocket vehicle are initialized

#### Scenario: No Rocket presentation in other modes
- **WHEN** the application starts in solar or craft mode
- **THEN** Rocket-specific engine, ground-effect, and audio presentation systems are not registered

### Requirement: Default launch uses a Papua presentation site
The default Earth Rocket launch SHALL use a validated Papua, Indonesia coastal-lowland geodetic anchor, sample its launch elevation and normal from the existing authoritative `TerrainSource`, and continue to convert it through the existing reference-frame authority.

#### Scenario: Default Rocket spawn
- **WHEN** Rocket mode spawns its default vehicle
- **THEN** the vehicle and visual-only launch facility are anchored at the configured Papua site using the terrain-derived surface position and normal
