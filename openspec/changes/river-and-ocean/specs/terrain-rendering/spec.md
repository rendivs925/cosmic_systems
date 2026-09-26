## ADDED Requirements

### Requirement: Terrain ocean patches present a displaced water surface

Terrain patches that contain ocean SHALL spawn a water surface with geometry
sufficient to carry vertex displacement, sharing the existing water material and
preserving the normalized depth vertex channel, instead of a flat sea-level cap.

#### Scenario: Ocean patch spawns a displaceable surface

- **WHEN** a ready terrain patch contains ocean
- **THEN** it spawns a water surface whose geometry supports vertex displacement
  and whose normalized depth channel is preserved for shading

#### Scenario: Displaced surface stays within the patch lifetime

- **WHEN** a terrain patch is evicted
- **THEN** its displaced water surface is released with the patch

#### Scenario: Coastline remains sealed

- **WHEN** the ocean surface is displaced near the coastline
- **THEN** the displaced surface continues to meet the land without holes

### Requirement: Terrain patch rivers follow the drainage network

Terrain patches crossed by an authoritative river channel SHALL build flow-directed
channel geometry with width scaled by discharge/flow accumulation and defined
banks, replacing the flat river ribbon.

#### Scenario: Flow-directed channel replaces ribbon

- **WHEN** a ready terrain patch is crossed by the authoritative drainage network
- **THEN** it spawns a flow-directed channel with banks rather than a flat ribbon

#### Scenario: Channel width reflects discharge

- **WHEN** the patch samples a higher discharge/flow accumulation
- **THEN** the generated channel is wider than at a lower discharge

#### Scenario: Dry patch allocates no channel

- **WHEN** a ready terrain patch has no authoritative drainage signal
- **THEN** no river channel geometry is allocated for that patch

### Requirement: Water presentation consumes authoritative terrain data only

Terrain water geometry SHALL derive its ocean depth, drainage alignment, and
discharge from the authoritative terrain source without maintaining a competing
hydrology or water field.

#### Scenario: Single drainage authority

- **WHEN** river geometry is built for a patch
- **THEN** its alignment and width come from the authoritative terrain source
  hydrology signal and not from an independent render-time computation

#### Scenario: Water does not feed terrain

- **WHEN** water geometry or materials are updated
- **THEN** no authoritative terrain height, collision, or streaming state changes
