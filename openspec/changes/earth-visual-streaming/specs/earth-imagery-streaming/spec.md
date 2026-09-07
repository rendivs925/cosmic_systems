## Purpose

Provide Earth with an offline-prepared imagery hierarchy that refines visible
terrain from a globe overview to locally detailed satellite presentation while
preserving the existing terrain and collision authorities.

## ADDED Requirements

### Requirement: Earth imagery has documented provenance and offline availability
The system SHALL accept an Earth imagery package only when its manifest records
the body-fixed datum, geographic coverage, source resolution, source version,
license, checksum, and local package location. The simulator MUST NOT download
imagery at runtime.

#### Scenario: Valid imagery package is available
- **WHEN** the configured Earth imagery package and manifest are present and valid
- **THEN** Earth presentation can request imagery from that package without a
  network request

#### Scenario: Imagery package is absent
- **WHEN** the configured Earth imagery package is unavailable
- **THEN** Earth continues to render with its existing global albedo and emits a
  startup-level availability status without changing terrain or collision data

### Requirement: Visible Earth terrain refines imagery progressively
The system SHALL use the existing terrain patch identity and visibility
selection to request the most detailed available imagery for visible Earth
terrain. Global albedo MUST remain visible until a replacement imagery tile is
ready, and imagery refinement MUST NOT block terrain geometry publication.

#### Scenario: Detailed imagery becomes ready
- **WHEN** a visible Earth terrain patch has a ready imagery tile at a more
  detailed level than the global overview
- **THEN** the patch renders the detailed imagery without exposing an untextured
  gap or replacing its authoritative terrain geometry

#### Scenario: Detailed imagery is pending
- **WHEN** a visible Earth terrain patch has no ready detailed imagery tile
- **THEN** the patch retains the global overview albedo while imagery work is
  pending

### Requirement: Imagery streaming is bounded and presentation-only
The system SHALL bound imagery residency, in-flight preparation, and GPU uploads
within explicit budgets owned by the existing terrain streaming lifecycle.
Imagery data MUST NOT become a terrain-height, collision, altitude, or physics
authority.

#### Scenario: Imagery pressure reaches a budget
- **WHEN** detailed imagery would exceed its configured cache or upload budget
- **THEN** the system retains visible imagery or its global fallback, evicts only
  non-visible cached imagery, and does not stall geometry streaming

#### Scenario: Terrain data is queried
- **WHEN** collision or altitude logic samples the Earth surface
- **THEN** it continues to use the active `TerrainSource` independently of
  imagery availability or residency

### Requirement: Imagery behavior is observable and visually accepted
The system SHALL report imagery residency, pending work, and upload backlog
through the existing terrain streaming telemetry. The Earth globe-to-ground path
MUST be accepted on a usable native display with continuous overview fallback,
no visible imagery seams at cube-face boundaries, and no long-lived blank
terrain patches.

#### Scenario: Profiling run is enabled
- **WHEN** an operator enables the existing performance and terrain telemetry
  for an Earth flight-camera run
- **THEN** the run reports imagery work alongside terrain geometry work and
  frame-time percentiles

#### Scenario: Native-display acceptance run
- **WHEN** the camera transitions from globe scale to the configured detailed
  Earth imagery coverage on a usable display
- **THEN** the overview remains continuous while detailed imagery replaces it
  without visible cube-face seams or blank patches
