## ADDED Requirements

### Requirement: Tile-backed terrain source preserves a resident fallback

The terrain authority SHALL always provide a resident coarse elevation surface
for every body with terrain, so height queries succeed even when no
high-resolution tile is resident.

#### Scenario: Query outside resident tile coverage

- **WHEN** a height or collision query is made where no high-resolution tile is
  resident
- **THEN** the authority returns the resident coarse surface value with its
  declared conservative error

#### Scenario: Query inside resident coverage

- **WHEN** a height or collision query is made where a high-resolution tile is
  resident
- **THEN** the authority returns the tile's measured elevation

### Requirement: Terrain height sampling never performs I/O or blocks

Authoritative terrain sampling used by collision, radar altitude, and physics
SHALL read only already-resident data and SHALL NOT load, decode, or await a
payload tile.

#### Scenario: Fixed-step collision during tile load

- **WHEN** the rocket queries terrain height while a payload tile is being
  decoded by a worker task
- **THEN** the query completes synchronously from resident data without
  blocking on the worker

### Requirement: Tile residency is bounded and deterministic

The set of resident elevation tiles SHALL be bounded by an explicit budget with
deterministic eviction, and a cache miss SHALL produce the same samples as a
cache hit for the same tile.

#### Scenario: Eviction then reload

- **WHEN** a resident tile is evicted and later reloaded
- **THEN** the reloaded tile produces identical elevation samples to the
  original

### Requirement: Measured local elevation overrides global elevation where covered

Where a reviewed measured local elevation package covers a region, the terrain
authority SHALL use its samples for that coverage and the global payload tile
elsewhere, with a deterministic transition between them.

#### Scenario: Local coverage boundary

- **WHEN** a query crosses from local measured coverage into global coverage
- **THEN** elevation transitions continuously without a seam or cliff artifact
