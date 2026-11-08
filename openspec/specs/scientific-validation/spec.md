# scientific-validation Specification

## Purpose

Provides versioned external scientific reference cases and quantitative residual
gates so model accuracy is evaluated independently from internal replay output.
## Requirements
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

### Requirement: The required scene matrix is machine-checked

The repository SHALL carry a manifest with one entry per required visual scene,
giving the frame file, the commit it was captured at, and the capture recipe when
known. A runnable command SHALL recompute a fixed set of image statistics from
the committed bytes and SHALL exit non-zero when a scene is missing, its commit
is not in the repository history, or a statistic leaves its tolerance.

The command SHALL offer a mode that refreshes the committed baseline and a mode
that re-captures a scene from its recorded recipe and compares the fresh frame.

The statistics SHALL be coarse enough to tolerate render noise and specific
enough to catch a blank, black, mis-exposed, cloudless or geometry-collapsed
frame.

#### Scenario: Missing scene

- **WHEN** a required scene has no committed frame
- **THEN** the check exits non-zero and names that scene

#### Scenario: Regression

- **WHEN** a committed frame is replaced by one that loses the cloud deck or the
  terrain detail
- **THEN** the affected statistic leaves its tolerance and the check exits
  non-zero

#### Scenario: Stale evidence

- **WHEN** a manifest entry names a commit that is not an ancestor of `HEAD`
- **THEN** the check exits non-zero

