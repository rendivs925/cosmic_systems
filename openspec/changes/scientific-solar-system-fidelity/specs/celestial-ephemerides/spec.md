## MODIFIED Requirements

### Requirement: Primary-body states have an explicit epoch and frame
The system SHALL evaluate each represented high-fidelity body from one local,
versioned kernel authority at an explicit TDB epoch. States SHALL remain f64
SSB/ICRF J2000 meters and meters-per-second until a consumer derives a
same-epoch relative state through the authoritative reference-frame boundary.

#### Scenario: Startup epoch

- **WHEN** the simulation has no elapsed fixed time
- **THEN** the shared snapshot resolves to the selected startup epoch in TDB

#### Scenario: Render projection

- **WHEN** a solar-map position is rendered
- **THEN** its f64 physical state is converted to display units only at the
  presentation boundary

#### Scenario: Flight-frame Sun presentation

- **WHEN** rocket-mode Sun geometry or directional lighting is updated
- **THEN** it consumes the same f64 snapshot epoch as dynamics rather than a
  solar-map transform or an artistic day/night orbit

#### Scenario: Physical primary-body state

- **WHEN** a solar-system flight feature needs a body state
- **THEN** it receives a same-epoch f64 relative state in meters and
  meters-per-second derived from the shared kernel snapshot

### Requirement: Primary bodies use one published secular-element authority
The system SHALL use the selected local kernel set as the sole high-fidelity
translation authority for all catalog bodies declared kernel-backed. Analytic
secular elements MAY remain only for explicitly labelled presentation markers or
catalog bodies outside the provisioned kernel coverage.

#### Scenario: Kernel-backed catalog body

- **WHEN** a catalog body is declared kernel-backed
- **THEN** its runtime translation is evaluated from the shared kernel snapshot
  and no separate analytic primary table supplies its state

#### Scenario: Explicit approximation

- **WHEN** a catalog body lacks required kernel coverage
- **THEN** the system labels its approximation and does not represent it as
  kernel-reference accurate
