## Purpose

Defines the offline, deterministic scientific source of solar-system body state
and orientation so every simulation and presentation consumer shares one
reviewed NAIF SPICE/JPL DE authority.

## ADDED Requirements

### Requirement: Kernel provenance is explicit and offline

The system SHALL evaluate known celestial bodies only from a versioned local
kernel set with a manifest declaring each source, version, checksum, coverage,
NAIF identifiers, frame, units, and license/provenance. Runtime evaluation
SHALL not depend on a network service.

#### Scenario: Valid local kernel set

- **WHEN** the application initializes its shared ephemeris authority
- **THEN** it validates the selected local kernel manifest and makes the
  resulting immutable provenance available to all application modes

#### Scenario: Missing or invalid kernel

- **WHEN** a required kernel is missing, has a checksum mismatch, or cannot
  cover a requested state
- **THEN** the system reports the specific failure and does not silently use an
  analytic, rendered, cached, or network-derived substitute

### Requirement: Body states have one scientific contract

The system SHALL evaluate known bodies by NAIF target and center identifiers at
a TDB epoch and return f64 SI position and velocity in ICRF/J2000. Derived
heliocentric, planet-centered, and render-space states SHALL be computed from
states evaluated at that same epoch.

#### Scenario: Barycentric primary state

- **WHEN** a consumer requests Earth relative to the Solar System barycenter at
  a supported TDB epoch
- **THEN** it receives an ICRF/J2000 f64 state in meters and meters-per-second

#### Scenario: Relative state

- **WHEN** a consumer requests the Sun relative to Earth at a supported epoch
- **THEN** the result uses the same evaluated epoch and preserves both position
  and velocity differences without a display-unit conversion

### Requirement: Ephemeris results are deterministic and independently validated

The system SHALL produce identical states for identical kernel provenance,
target, center, frame, and TDB epoch. The selected kernel set SHALL have
recorded reference-vector validation against JPL Horizons without using
Horizons during runtime.

#### Scenario: Repeated evaluation

- **WHEN** two application modes evaluate the same state from the same manifest
- **THEN** their f64 position and velocity values agree within the documented
  deterministic numerical tolerance

#### Scenario: External reference regression

- **WHEN** the supported kernel set is changed
- **THEN** recorded Horizons comparisons for multiple bodies and epochs verify
  position and velocity within declared SI tolerances
