## ADDED Requirements

### Requirement: Deterministic baselines and external truth remain distinct
The system SHALL label internal replay baselines as deterministic regression
fixtures and SHALL not use agreement with those fixtures as evidence of physical
accuracy. Scientific acceptance SHALL additionally require the external
reference residual suite for every changed scientific model.

#### Scenario: Physics model change
- **WHEN** a scientific model changes an internal trajectory baseline
- **THEN** its audit records the intentional deterministic divergence and the
  affected external-reference residual results

#### Scenario: Reference suite unavailable
- **WHEN** required kernels or reference datasets are unavailable in a local
  development environment
- **THEN** deterministic regression may run, but the scientific-validation gate
  reports that external accuracy was not verified
