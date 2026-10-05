## ADDED Requirements

### Requirement: Terrain shading samples baked self-shadow and occlusion terms

Terrain rendering SHALL sample the baked self-shadow and ambient/sky-occlusion
terms produced from the authoritative height field and apply them to the direct
and indirect lighting contributions respectively. Sampling SHALL use the
existing terrain patch identity and material path and SHALL NOT re-run a
per-pixel real-time terrain shadow pass where the baked term is available.

#### Scenario: Direct light is scaled by self-shadow

- **WHEN** a terrain fragment samples a reduced self-shadow term
- **THEN** only its direct-sun contribution is attenuated by that term

#### Scenario: Indirect light is scaled by sky occlusion

- **WHEN** a terrain fragment samples a reduced ambient/sky-occlusion term
- **THEN** its sky/ambient contribution is attenuated while its direct-sun
  contribution is unaffected by that term

### Requirement: Water receives terrain shadow

Ocean and river water surfaces SHALL receive the shared directional shadow and
the terrain-landscape shadow so that terrain occludes water instead of water
being excluded from shadow receiving. Enabling water shadow receiving SHALL NOT
change water geometry, water simulation, or terrain authority.

#### Scenario: Hill casts shadow on the water surface

- **WHEN** terrain lies between a water fragment and the shared ephemeris Sun
- **THEN** the water fragment's direct-sun contribution is attenuated

#### Scenario: Water remains presentation-only

- **WHEN** water shadow receiving is enabled
- **THEN** water meshes still do not cast shadows and no terrain, collision, or
  simulation state changes
