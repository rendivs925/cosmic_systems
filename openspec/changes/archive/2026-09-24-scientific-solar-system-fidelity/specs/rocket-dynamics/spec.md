## ADDED Requirements

### Requirement: Long-arc propagation has an explicit accuracy contract
The system SHALL provide a deterministic long-arc propagation path distinct from
high-rate powered-flight and contact integration. It SHALL expose its force
model, step or error-control configuration, and documented validity envelope.

#### Scenario: Propagation configuration
- **WHEN** a long-arc trajectory is requested
- **THEN** the result identifies the integration method, tolerances or fixed
  step, maximum propagation step, and selected force-model tier

#### Scenario: Contact isolation
- **WHEN** a vehicle is in powered flight or ground contact
- **THEN** the high-rate fixed pipeline remains authoritative and long-arc
  propagation does not directly mutate its state

### Requirement: Numerical accuracy is validated by scenario
The system SHALL publish scenario-specific numerical error budgets for at least
LEO, J2-precessing orbit, lunar transfer, and escape or interplanetary cases.

#### Scenario: LEO checkpoint validation
- **WHEN** a stated LEO validation duration completes
- **THEN** position and velocity residuals at each checkpoint satisfy the LEO
  budget for the selected force model
