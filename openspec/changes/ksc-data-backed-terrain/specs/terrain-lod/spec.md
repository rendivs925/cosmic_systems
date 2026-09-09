## ADDED Requirements

### Requirement: Local measured coverage receives bounded refinement

The system SHALL permit finer terrain refinement within valid local elevation
coverage while preserving the global terrain memory, task, and lifecycle bounds.

#### Scenario: KSC close-range refinement
- **WHEN** the active camera or prelaunch rocket is within configured KSC local
  elevation coverage
- **THEN** the terrain hierarchy refines that coverage beyond the global maximum
  level without materializing high-resolution terrain elsewhere
