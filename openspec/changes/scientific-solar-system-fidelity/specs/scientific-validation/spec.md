## Purpose

Provides versioned external scientific reference cases and quantitative residual
gates so model accuracy is evaluated independently from internal replay output.

## ADDED Requirements

### Requirement: External reference cases are versioned and reproducible
The system SHALL retain machine-readable reference cases with source, kernel or
dataset version, frame, center, time scale, units, and generation command for
ephemeris, orientation, force, and propagation validation.

#### Scenario: Reference provenance
- **WHEN** a reference case is loaded
- **THEN** the case identifies its external source and the exact time, frame,
  center, and units needed to reproduce the value

#### Scenario: Provisioned validation
- **WHEN** required local kernels and datasets are provisioned
- **THEN** the external-reference suite evaluates without network access

### Requirement: Scientific residuals have explicit acceptance budgets
The system SHALL compare evaluated states against external reference cases at
multiple epochs and publish residuals in physical units. A failure SHALL report
the body or vehicle, epoch, frame, quantity, residual, and applicable budget.

#### Scenario: Ephemeris residual failure
- **WHEN** a body position or velocity exceeds its stated reference tolerance
- **THEN** validation fails with the target, center, epoch, position residual,
  velocity residual, and source dataset

#### Scenario: Propagation error budget
- **WHEN** a long-arc propagation scenario completes
- **THEN** validation reports position and velocity residuals against its
  reference trajectory at every stated checkpoint
