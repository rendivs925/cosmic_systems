## Purpose

Supplies versioned, offline-prepared cube-sphere elevation tile payloads with
explicit provenance, coverage, resolution, and per-tile geometric error so the
terrain authority can stream measured high-resolution relief without runtime
downloads or a second terrain pipeline.

## ADDED Requirements

### Requirement: Elevation payload packages declare provenance and coverage

An elevation payload package SHALL declare its body identifier, body-fixed
frame and vertical datum, source version, provenance, license, resolution,
coverage, tile layout, and content checksum before it is used by the terrain
authority.

#### Scenario: Valid package is accepted

- **WHEN** a payload manifest declares every required field with valid values
  and its content checksum matches
- **THEN** the terrain authority loads the package

#### Scenario: Invalid package is rejected

- **WHEN** a payload manifest omits a required field, declares an unsupported
  version, or fails checksum verification
- **THEN** loading fails with an explicit error and no partial terrain is used

### Requirement: Payload tiles declare per-tile elevation bounds and error

Each elevation payload tile SHALL declare its coverage, availability, minimum
and maximum elevation, and a conservative geometric error, so LOD selection and
culling do not rely on a global envelope.

#### Scenario: Tile metadata drives refinement

- **WHEN** a patch is selected for rendering
- **THEN** its geometric error is derived from the covering payload tile's
  declared error rather than a planet-wide bound

#### Scenario: Missing tile reports unavailability

- **WHEN** a requested tile is not produced in the package
- **THEN** the resolver reports that tile as unavailable and the caller uses the
  declared fallback

### Requirement: Payload decode is deterministic and version-checked

Decoding an elevation payload tile SHALL be a deterministic function of the
payload bytes and SHALL reject payloads whose format version is unsupported.

#### Scenario: Repeated decode is identical

- **WHEN** the same payload tile is decoded twice, including after cache
  eviction and reload
- **THEN** both decodes produce identical elevation samples

### Requirement: Offline conversion is reproducible and checksum-verified

The offline elevation converter SHALL resample reviewed source data onto the
cube-sphere deterministically and SHALL record enough metadata to verify the
generated runtime package.

#### Scenario: Reproducible package

- **WHEN** the converter runs twice over identical source data with identical
  parameters
- **THEN** it produces byte-identical payload content and a stable checksum
