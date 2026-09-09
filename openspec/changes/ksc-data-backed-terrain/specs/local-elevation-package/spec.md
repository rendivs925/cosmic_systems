## Purpose

Defines validated offline local elevation packages so measured, high-resolution
terrain can augment a planetary DEM without runtime network or raster I/O.

## ADDED Requirements

### Requirement: Local elevation packages declare scientific provenance

The system SHALL accept a local elevation package only when it declares the
body-fixed coverage, horizontal and vertical datum, source resolution, nodata
behavior, source checksum, runtime checksum, license, and conversion version.

#### Scenario: Valid packaged elevation loads
- **WHEN** a complete local elevation package matches its manifest and checksum
- **THEN** the system loads its immutable samples at startup without network access

#### Scenario: Invalid present package fails explicitly
- **WHEN** a configured local elevation package is malformed, truncated, or has
  incompatible metadata
- **THEN** startup reports a configuration error instead of silently using it

### Requirement: Local elevation is sampled deterministically

The system SHALL sample valid local elevation coverage in meters through
deterministic interpolation and use its declared nodata behavior at every edge.

#### Scenario: Interior sample
- **WHEN** a terrain query lies inside valid local package coverage
- **THEN** it returns the deterministic interpolation of measured elevation samples

#### Scenario: Nodata or outside coverage
- **WHEN** a terrain query lies outside coverage or in declared nodata
- **THEN** the package reports no local elevation contribution
