## Purpose

Grounds the landscape with baked terrain self-shadowing and ambient occlusion
so hills, valleys, and rock contacts read with depth under the shared
ephemeris-derived Sun without introducing a second terrain or lighting
authority.

## ADDED Requirements

### Requirement: Terrain self-shadow is baked from the authoritative height field

The system SHALL derive a per-surface self-shadow visibility term by ray-marching
the authoritative terrain height field toward the shared ephemeris Sun
direction, using the same fixed-sun convention as the directional shadow
lighting. The term SHALL be produced from the existing terrain patch identity
and height field, and the system SHALL NOT introduce a second terrain
representation or height source.

#### Scenario: Hill shades the ground behind it

- **WHEN** a terrain sample lies behind a higher sample along the ephemeris Sun
  direction
- **THEN** its self-shadow term is reduced toward zero

#### Scenario: Sunlit slope keeps full direct light

- **WHEN** the ray toward the Sun is unobstructed above the height field
- **THEN** the self-shadow term remains at full visibility

### Requirement: Self-shadow affects only the direct-sun contribution

The system SHALL apply the terrain self-shadow term only to the direct-sun
contribution of terrain lighting. Indirect sky and ambient fill SHALL remain
available to shadowed terrain so occluded slopes retain readable fill light
rather than becoming black.

#### Scenario: Shadowed terrain retains sky fill

- **WHEN** a terrain fragment is fully self-shadowed but the sky/ambient
  contribution is non-zero
- **THEN** the fragment's final colour includes the indirect contribution and is
  not reduced to zero radiance

### Requirement: Ambient and sky occlusion darkens crevices and contacts

The system SHALL compute an ambient-occlusion/sky-occlusion term from the
authoritative height field for terrain concavities, crevices, and ground
contact, and SHALL apply it to the indirect sky/ambient contribution rather
than to the direct-sun contribution.

#### Scenario: Crevice is darker than an open slope

- **WHEN** two terrain samples share a similar orientation but one is enclosed
  by nearby higher terrain
- **THEN** the enclosed sample receives stronger indirect occlusion and less sky
  fill than the open sample

### Requirement: Occlusion bake reuses shared Sun and remains consistent with the day/night cycle

The self-shadow bake SHALL use the Sun direction from the shared ephemeris state
and the existing fixed-sun shadow convention, and SHALL remain consistent with
the rotating body producing the day/night cycle. The bake SHALL record the Sun
direction it used and SHALL be refreshed when that direction changes materially
or the terrain patch is regenerated.

#### Scenario: Baked term matches the active Sun direction

- **WHEN** the ephemeris Sun direction changes enough to alter the occlusion
  result for a resident patch
- **THEN** the patch's baked self-shadow term is refreshed against the new
  direction

#### Scenario: Body rotation does not require rebaking per frame

- **WHEN** the planet rotates through a fixed inertial Sun direction with an
  unchanged patch
- **THEN** no per-frame rebake occurs solely because of rotation

### Requirement: Occlusion bake and sampling are bounded and evidence-gated

The system SHALL bound the number of height-field samples per bake and the
number of occlusion samples per rendered fragment, and SHALL expose bake and
sampling cost through the existing terrain performance telemetry. Quality or
budget increases SHALL require measured evidence rather than being enabled by
default.

#### Scenario: Bake work is bounded per patch

- **WHEN** a terrain patch bakes its self-shadow term
- **THEN** the number of height-field ray-march samples is capped by a
  configured maximum

#### Scenario: Occlusion cost is reported

- **WHEN** terrain performance instrumentation is enabled
- **THEN** occlusion bake and sampling cost appear in the terrain performance
  telemetry

### Requirement: Occlusion presentation is deterministic and presentation-only

The occlusion term SHALL be a pure function of the authoritative height field,
the shared ephemeris Sun direction, and patch identity, and SHALL NOT depend on
render frame rate, camera pose, wall-clock time, or unordered iteration. It
SHALL affect only rendered appearance and SHALL NOT modify terrain source,
collision, physics, guidance, or any authoritative simulation state.

#### Scenario: Identical inputs reproduce identical occlusion

- **WHEN** the same patch, height field parameters, seed, and Sun direction are
  baked twice
- **THEN** the resulting occlusion terms are identical

#### Scenario: Occlusion never feeds simulation

- **WHEN** the occlusion term changes for visual tuning
- **THEN** terrain height, collision, and fixed-step simulation results are
  unchanged
