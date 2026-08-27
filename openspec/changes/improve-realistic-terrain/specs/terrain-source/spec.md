## MODIFIED Requirements

### Requirement: Procedural generation is deterministic

The system SHALL generate procedural terrain deterministically from a seed, coordinates, resolution, and parameters, with identical inputs producing identical output. Earth-like terrain SHALL combine continent-scale relief with a deterministic local-detail layer so near-surface rendering has visible ridges and drainage-like variation without changing the source interface.

#### Scenario: Deterministic regeneration

- **WHEN** the same seed, coordinates, and resolution are used to generate terrain twice
- **THEN** the two outputs are identical

#### Scenario: Generation independence from runtime

- **WHEN** terrain is generated
- **THEN** the result does not depend on frame rate, spawn order, or camera movement

#### Scenario: Local relief outside launch sites

- **WHEN** a near-surface patch is sampled outside a launch-site clearance area
- **THEN** it contains deterministic local elevation variation at the active patch resolution

### Requirement: Launch-site patches reuse the shared source

Existing localized launch-site patches (KSC, RTLS, drone ship, lunar) SHALL continue to exist as detailed site objects but MUST sample their height from the shared terrain source architecture rather than a bespoke path. Site leveling SHALL be limited to the configured pad-scale clearance footprint and transition continuously to the surrounding source height.

#### Scenario: Site height via shared source

- **WHEN** a launch-site patch is rendered or queried for collision
- **THEN** its height comes from the shared terrain source interface

#### Scenario: Existing sites preserved

- **WHEN** the terrain source architecture is introduced
- **THEN** the existing launch-site patches remain available as before

#### Scenario: Nearby terrain remains visible

- **WHEN** the camera views terrain beyond a launch pad's clearance footprint
- **THEN** surrounding source relief is visible without a discontinuous edge at the site boundary
